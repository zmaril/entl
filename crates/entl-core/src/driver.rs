//! Driver sink — a sink whose *execute* is a host-provided step, so a binding can mirror entl
//! into a database entl-core doesn't link (PGlite in the browser, MySQL from Ruby, …).
//!
//! Every other [`Sink`](crate::sink::Sink) owns its connection and runs SQL itself. A
//! [`DriverSink`] owns none: it turns each change batch into a stream of [`Statement`]s — the
//! DDL, the `ON CONFLICT` upsert, the type mapping, the blob→hex, all the logic that used to live
//! in each language's hand-written mirror — and hands them, one at a time, to a callback. The host
//! executes them against whatever client it holds.
//!
//! The callback is deliberately *not* called synchronously across the FFI boundary (a JS/PGlite
//! client's `query` is async — you can't block a Rust thread on its Promise). Instead a binding
//! wires the callback to [`statement_channel`] and drains the [`StatementStream`] with the same
//! blocking `poll(timeout)` the change stream uses, executing each statement in its own event
//! loop. See notes/design/multidb.md — the data-*shaping* is all here in Rust; only the final
//! `exec(sql, params)` stays in the host.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender};
use serde_json::Value;
use std::time::Duration;

// Schema + keys come from the GENERATED module `schema_gen` — the entl catalog
// (crates/fluessig/entl.tsp) lowered to committed Rust source by fluessig-gen.
// No runtime parsing/rendering: the schema is code (like tables.gen.ts /
// models.py); a regenerates-identically test in fluessig guards drift.
use crate::schema_gen::{TableSchema, PG_TABLES};

/// The Postgres schema of a canonical table, from the generated catalog.
fn table_schema(canonical: &str) -> Option<&'static TableSchema> {
    PG_TABLES.iter().find(|t| t.name == canonical)
}
use crate::sink::{cell_json, Sink, SinkSelect};
use crate::stream::{ChangeBatch, ChangeOp};

/// One executable statement: SQL with positional placeholders + the parameters to bind. DDL
/// (`CREATE`/`DELETE`) carries no params. The host runs it verbatim — e.g. `pg.query(sql, params)`.
/// `table` names the canonical (source) table it acts on — `None` for cross-table DDL like
/// `CREATE SCHEMA` — so a host can route or tally per table without parsing the SQL.
#[derive(Debug, Clone)]
pub struct Statement {
    pub sql: String,
    pub params: Vec<Value>,
    pub table: Option<String>,
}

/// The SQL dialect a [`DriverSink`] emits. Only Postgres today (covers PGlite); the enum is the
/// seam for MySQL/others without touching the sink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// Postgres: `$n` placeholders, `ON CONFLICT … DO UPDATE … EXCLUDED`, `text`/`timestamptz`.
    Postgres,
}

/// A sink that emits [`Statement`]s to a callback instead of executing them itself.
///
/// `emit` is any `FnMut(Statement) -> Result<()>`; a binding usually sends into a
/// [`statement_channel`]. Schema/DDL/type mapping mirror [`PostgresSink`](crate::sink::PostgresSink)
/// — the two are the same logic, one executing locally, one delegating.
pub struct DriverSink<E: FnMut(Statement) -> Result<()>> {
    emit: E,
    dialect: Dialect,
    select: SinkSelect,
    schema: Option<String>,
    /// Have we emitted `CREATE SCHEMA` yet?
    schema_made: bool,
    /// Targets we've already emitted `CREATE TABLE` for, → their PK columns (empty = no PK).
    ensured: HashMap<String, Vec<String>>,
    /// Cached upsert SQL per target.
    insert_sql: HashMap<String, String>,
    /// Targets already cleared once this run (for `Replace`).
    cleared: HashSet<String>,
}

impl<E: FnMut(Statement) -> Result<()>> DriverSink<E> {
    /// A driver sink emitting `dialect` statements to `emit`. `select` narrows tables/renames and
    /// (Postgres) sets the target schema — default `entl`, so entl's tables never collide with the
    /// app's, exactly like [`PostgresSink`](crate::sink::PostgresSink).
    pub fn new(emit: E, dialect: Dialect, select: SinkSelect) -> Self {
        let schema = Some(select.schema.clone().unwrap_or_else(|| "entl".to_string()));
        Self {
            emit,
            dialect,
            select,
            schema,
            schema_made: false,
            ensured: HashMap::new(),
            insert_sql: HashMap::new(),
            cleared: HashSet::new(),
        }
    }

    /// `"schema"."target"` (or `"target"` with no schema).
    fn qualified(&self, target: &str) -> String {
        match &self.schema {
            Some(s) => format!("\"{s}\".\"{target}\""),
            None => format!("\"{target}\""),
        }
    }

    /// Ensure the target table exists (emit its `CREATE`) and cache its PK. Schema + keys come
    /// from the generated [`schema_gen`](crate::schema_gen) module — the fluessig catalog as
    /// committed code. (Previously: entl's SQL templates, re-parsed with `parse_pk`; the fluessig
    /// parity gate proved the two identical.)
    fn ensure(&mut self, canonical: &str, target: &str, cols: &[String]) -> Result<Vec<String>> {
        if let Some(pk) = self.ensured.get(target) {
            return Ok(pk.clone());
        }
        if let (Some(s), false) = (self.schema.clone(), self.schema_made) {
            let sql = format!("CREATE SCHEMA IF NOT EXISTS \"{s}\"");
            (self.emit)(Statement { sql, params: vec![], table: None })?;
            self.schema_made = true;
        }
        // Instantiate the table's DDL template at the (possibly renamed, schema-qualified) name.
        let repl = match &self.schema {
            Some(s) => format!("{s}\".\"{target}"),
            None => target.to_string(),
        };
        let pk = match table_schema(canonical) {
            Some(ts) => {
                let ddl = ts.ddl.replace("__table__", &repl);
                (self.emit)(Statement { sql: ddl, params: vec![], table: Some(canonical.to_string()) })?;
                ts.pk.iter().map(|s| s.to_string()).collect()
            }
            None => {
                // Not in the catalog — a typeless create from the batch columns (all text), no PK.
                let cols_ddl = cols.iter().map(|c| format!("\"{c}\" text")).collect::<Vec<_>>().join(", ");
                let sql = format!("CREATE TABLE IF NOT EXISTS {} ({cols_ddl})", self.qualified(target));
                (self.emit)(Statement { sql, params: vec![], table: Some(canonical.to_string()) })?;
                Vec::new()
            }
        };
        self.ensured.insert(target.to_string(), pk.clone());
        Ok(pk)
    }
}

impl<E: FnMut(Statement) -> Result<()>> Sink for DriverSink<E> {
    fn apply(&mut self, batch: &ChangeBatch) -> Result<u64> {
        if !self.select.included(&batch.table) {
            return Ok(0);
        }
        debug_assert_eq!(self.dialect, Dialect::Postgres);
        let target = self.select.target(&batch.table);
        let cols: Vec<String> =
            batch.batch.schema().fields().iter().map(|f| f.name().clone()).collect();
        let pk = self.ensure(&batch.table, &target, &cols)?;

        if batch.op == ChangeOp::Replace && self.cleared.insert(target.clone()) {
            let sql = format!("DELETE FROM {}", self.qualified(&target));
            (self.emit)(Statement { sql, params: vec![], table: Some(batch.table.clone()) })?;
        }

        // One upsert per row; params in column order. Cache the SQL per target.
        if !self.insert_sql.contains_key(&target) {
            let sql = pg_insert_sql(&self.qualified(&target), &cols, &pk);
            self.insert_sql.insert(target.clone(), sql);
        }
        let sql = self.insert_sql[&target].clone();
        let arr = batch.batch.columns();
        for i in 0..batch.batch.num_rows() {
            let params = arr.iter().map(|c| cell_json(c, i)).collect();
            (self.emit)(Statement { sql: sql.clone(), params, table: Some(batch.table.clone()) })?;
        }
        Ok(batch.batch.num_rows() as u64)
    }
}

/// Build a Postgres upsert (`$n` placeholders) for `table` (already quoted/qualified).
fn pg_insert_sql(table: &str, cols: &[String], pk: &[String]) -> String {
    let quoted = cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
    let ph = (1..=cols.len()).map(|i| format!("${i}")).collect::<Vec<_>>().join(", ");
    if pk.is_empty() {
        return format!("INSERT INTO {table} ({quoted}) VALUES ({ph})");
    }
    let conflict = pk.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
    let non_pk: Vec<&String> = cols.iter().filter(|c| !pk.contains(c)).collect();
    if non_pk.is_empty() {
        format!("INSERT INTO {table} ({quoted}) VALUES ({ph}) ON CONFLICT ({conflict}) DO NOTHING")
    } else {
        let set = non_pk
            .iter()
            .map(|c| format!("\"{c}\" = EXCLUDED.\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ");
        format!("INSERT INTO {table} ({quoted}) VALUES ({ph}) ON CONFLICT ({conflict}) DO UPDATE SET {set}")
    }
}

/// The result of a [`StatementStream`] poll — mirrors [`Poll`](crate::stream::Poll).
#[derive(Debug)]
pub enum StmtPoll {
    Statement(Statement),
    /// Timed out; the producer is still running.
    Idle,
    /// The producer dropped its sender and the buffer is drained — the plan is complete.
    Closed,
}

/// The consumer side of a driver sink's statement plan: drain it with a blocking, bounded
/// `poll(timeout)` — the same primitive as the change stream, so a binding wraps it the same way.
pub struct StatementStream {
    rx: Receiver<Statement>,
}

impl StatementStream {
    pub fn poll(&self, timeout: Duration) -> StmtPoll {
        match self.rx.recv_timeout(timeout) {
            Ok(s) => StmtPoll::Statement(s),
            Err(RecvTimeoutError::Timeout) => StmtPoll::Idle,
            Err(RecvTimeoutError::Disconnected) => StmtPoll::Closed,
        }
    }
}

/// A bounded channel for a driver sink's statements: `(sender, stream)`. The `sender` becomes the
/// sink's `emit` (`move |s| sender.send(s)`); the caller drains `stream`. Bounded = backpressure,
/// so the producer paces to the host's execution.
pub fn statement_channel(capacity: usize) -> (Sender<Statement>, StatementStream) {
    let (tx, rx) = bounded(capacity);
    (tx, StatementStream { rx })
}

/// Replay an existing DuckDB store into `sink` — "a sink added months later backfills itself up"
/// (notes/design/multidb.md). Each present table is read via Arrow and applied as one `Replace`
/// batch, so the sink recreates it from scratch. This is the *pull* counterpart to the streaming
/// [`pull_into`](crate::pull::pull_into): it drives any sink — including a [`DriverSink`] — off a
/// store that's already populated, no repo or network needed.
pub fn backfill(conn: &duckdb::Connection, sink: &mut dyn Sink, tables: &[&str]) -> Result<u64> {
    let present: HashSet<String> = {
        let mut stmt = conn
            .prepare("SELECT table_name FROM information_schema.tables WHERE table_schema = 'main'")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<duckdb::Result<_>>()?
    };
    let mut total = 0u64;
    for &t in tables {
        if !present.contains(t) {
            continue;
        }
        let mut stmt = conn.prepare(&format!("SELECT * FROM \"{t}\""))?;
        let batches: Vec<duckdb::arrow::record_batch::RecordBatch> = stmt.query_arrow([])?.collect();
        for b in batches {
            total += sink.apply(&ChangeBatch::new(t, ChangeOp::Replace, b))?;
        }
    }
    Ok(total)
}

/// The tables a driver sink can mirror — the default backfill set: every physical table the
/// catalog describes (entity + association/edge tables). Catalog-driven, so a table added to
/// `entl.tsp` joins the mirror on regeneration.
pub fn driver_tables() -> Vec<&'static str> {
    PG_TABLES.iter().map(|t| t.name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use duckdb::arrow::array::{BinaryArray, Int64Array, StringArray};
    use duckdb::arrow::datatypes::{DataType, Field, Schema};
    use duckdb::arrow::record_batch::RecordBatch;
    use std::sync::Arc;

    #[test]
    fn keys_come_from_the_generated_catalog() {
        // parse_pk is gone: the generated schema module is the source of truth for keys.
        assert_eq!(table_schema("commits").unwrap().pk, ["oid"]);
        assert_eq!(table_schema("commit_parents").unwrap().pk, ["commit_oid", "idx"]);
        assert_eq!(table_schema("gh_pull_requests").unwrap().pk, ["repo_id", "number"]);
        // and the backfill set is catalog-driven (all 28 physical tables)
        assert_eq!(driver_tables().len(), 28);
    }

    #[test]
    fn driver_sink_emits_schema_ddl_and_upserts() {
        // A `commits`-shaped batch (oid is Binary → hex text; the PK).
        let schema = Arc::new(Schema::new(vec![
            Field::new("oid", DataType::Binary, false),
            Field::new("message", DataType::Utf8, false),
        ]));
        let oid = BinaryArray::from(vec![&[0xab, 0xcd][..]]);
        let msg = StringArray::from(vec!["hello"]);
        let batch = RecordBatch::try_new(schema, vec![Arc::new(oid), Arc::new(msg)]).unwrap();

        let mut out: Vec<Statement> = Vec::new();
        {
            let mut sink = DriverSink::new(
                |s| {
                    out.push(s);
                    Ok(())
                },
                Dialect::Postgres,
                SinkSelect { schema: Some("mine".into()), ..Default::default() },
            );
            sink.apply(&ChangeBatch::new("commits", ChangeOp::Replace, batch)).unwrap();
        }

        let sqls: Vec<&str> = out.iter().map(|s| s.sql.as_str()).collect();
        assert_eq!(sqls[0], "CREATE SCHEMA IF NOT EXISTS \"mine\"");
        assert!(sqls[1].contains("CREATE TABLE IF NOT EXISTS \"mine\".\"commits\""));
        assert_eq!(sqls[2], "DELETE FROM \"mine\".\"commits\"");
        assert!(sqls[3].contains("INSERT INTO \"mine\".\"commits\""));
        assert!(sqls[3].contains("ON CONFLICT (\"oid\") DO UPDATE SET \"message\" = EXCLUDED.\"message\""));
        // The oid param is hex text, not raw bytes.
        assert_eq!(out[3].params[0], Value::String("abcd".into()));
        assert_eq!(out[3].params[1], Value::String("hello".into()));
    }

    #[test]
    fn integer_ids_pass_through_as_numbers() {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let batch = RecordBatch::try_new(schema, vec![Arc::new(Int64Array::from(vec![7i64]))]).unwrap();
        let mut out = Vec::new();
        {
            let mut sink =
                DriverSink::new(|s| { out.push(s); Ok(()) }, Dialect::Postgres, SinkSelect::default());
            sink.apply(&ChangeBatch::new("gh_users", ChangeOp::Upsert, batch)).unwrap();
        }
        let insert = out.iter().find(|s| s.sql.contains("INSERT")).unwrap();
        assert_eq!(insert.params[0], Value::from(7i64));
    }
}
