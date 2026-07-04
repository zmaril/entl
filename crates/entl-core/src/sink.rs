//! Sinks — consumers of the change stream that write it into a target.
//!
//! straitjacket-allow-file:duplication — the SQLite and Postgres sinks are
//! deliberately parallel twins (same lifecycle, per-dialect details); the
//! genuinely shared pieces live in `cell_json`/`batch_to_json`/`upsert_sql`.
//!
//! See notes/design/multidb.md. A sink is "just a subscriber": it `poll`s the
//! [`ChangeStream`](crate::stream::ChangeStream) and applies each
//! [`ChangeBatch`](crate::stream::ChangeBatch) to its target. Every sink adapter
//! is Rust, here in `entl-core`; a binding just switches one on.
//!
//! The shared work is turning an Arrow batch into rows — [`cell_json`] +
//! [`batch_to_json`]. Object-ids (Arrow `Binary`) become **hex text**, matching
//! the design's "oids as hex text off DuckDB".

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use duckdb::arrow::array::{
    Array, ArrayRef, BinaryArray, BooleanArray, Float64Array, Int32Array, Int64Array,
    LargeBinaryArray, LargeStringArray, StringArray, TimestampMicrosecondArray,
};
use duckdb::arrow::datatypes::{DataType, TimeUnit};
use postgres::types::{ToSql, Type};
use postgres::Client;
use serde_json::{Map, Value};

use crate::stream::{ChangeBatch, ChangeOp, ChangeStream, Poll};

/// Per-sink table selection: which tables a sink writes, and how it names/places them.
/// The pull is the upper bound; a sink narrows from there (notes/design/multidb.md
/// §"Selecting tables"). All fields optional — the default writes every pulled table as-is.
#[derive(Debug, Default, Clone)]
pub struct SinkSelect {
    /// Include-list of source (canonical) table names. `None` = all.
    pub tables: Option<Vec<String>>,
    /// Source table names to skip.
    pub exclude: Vec<String>,
    /// Rename `source -> target` table name at the sink.
    pub rename: Vec<(String, String)>,
    /// Target schema (Postgres only; default `entl`). Ignored by other sinks.
    pub schema: Option<String>,
}

impl SinkSelect {
    /// Should this source table be written at all?
    pub fn included(&self, table: &str) -> bool {
        if self.exclude.iter().any(|t| t == table) {
            return false;
        }
        match &self.tables {
            Some(list) => list.iter().any(|t| t == table),
            None => true,
        }
    }

    /// The target table name for a source table (applies `rename`).
    pub fn target(&self, table: &str) -> String {
        self.rename
            .iter()
            .find(|(from, _)| from == table)
            .map(|(_, to)| to.clone())
            .unwrap_or_else(|| table.to_string())
    }
}

/// A sink applies change batches to a target. `apply` returns the number of rows it actually
/// wrote (0 for a batch the sink's [`SinkSelect`] excluded), so the drain total reflects what
/// landed, not what streamed.
pub trait Sink {
    fn apply(&mut self, batch: &ChangeBatch) -> Result<u64>;
    /// Flush/close (called once when the stream ends).
    fn finish(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Drive a sink off a change stream until it closes. Returns rows actually written.
pub fn drain(stream: &ChangeStream, sink: &mut dyn Sink, idle: Duration) -> Result<u64> {
    let mut rows = 0u64;
    loop {
        match stream.poll(idle) {
            Poll::Batch(b) => rows += sink.apply(&b)?,
            Poll::Idle => {}
            Poll::Closed => break,
        }
    }
    sink.finish()?;
    Ok(rows)
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// One Arrow cell → a JSON value (the neutral form both sinks build on).
pub fn cell_json(col: &ArrayRef, i: usize) -> Value {
    if col.is_null(i) {
        return Value::Null;
    }
    macro_rules! d {
        ($t:ty) => {
            col.as_any().downcast_ref::<$t>().unwrap().value(i)
        };
    }
    match col.data_type() {
        DataType::Utf8 => Value::String(d!(StringArray).to_string()),
        DataType::LargeUtf8 => Value::String(d!(LargeStringArray).to_string()),
        DataType::Int32 => Value::from(d!(Int32Array)),
        DataType::Int64 => Value::from(d!(Int64Array)),
        DataType::Float64 => Value::from(d!(Float64Array)),
        DataType::Boolean => Value::Bool(d!(BooleanArray)),
        DataType::Binary => Value::String(hex(d!(BinaryArray))),
        DataType::LargeBinary => Value::String(hex(d!(LargeBinaryArray))),
        DataType::Timestamp(TimeUnit::Microsecond, _) => DateTime::from_timestamp_micros(d!(TimestampMicrosecondArray))
            .map(|dt| Value::String(dt.to_rfc3339()))
            .unwrap_or(Value::Null),
        other => Value::String(format!("<unsupported arrow type {other:?}>")),
    }
}

/// A batch → JSON row objects, each tagged with its `_op`.
pub fn batch_to_json(batch: &ChangeBatch) -> Vec<Value> {
    let schema = batch.batch.schema();
    let cols = batch.batch.columns();
    (0..batch.batch.num_rows())
        .map(|i| {
            let mut obj = Map::new();
            obj.insert("_op".to_string(), Value::String(batch.op.as_str().to_string()));
            for (c, f) in cols.iter().zip(schema.fields()) {
                obj.insert(f.name().clone(), cell_json(c, i));
            }
            Value::Object(obj)
        })
        .collect()
}

/// Append every change batch's rows to `<dir>/<table>.jsonl` — a change log (one
/// JSON object per row, tagged with `_op`). Append-only; a durable materialized
/// view would replay + dedup by key (a follow-up).
pub struct JsonlSink {
    dir: PathBuf,
    select: SinkSelect,
    files: HashMap<String, BufWriter<File>>,
}

impl JsonlSink {
    pub fn new(dir: impl AsRef<Path>, select: SinkSelect) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
        Ok(Self {
            dir,
            select,
            files: HashMap::new(),
        })
    }
}

impl Sink for JsonlSink {
    fn apply(&mut self, batch: &ChangeBatch) -> Result<u64> {
        if !self.select.included(&batch.table) {
            return Ok(0);
        }
        let target = self.select.target(&batch.table); // rename → filename
        if !self.files.contains_key(&target) {
            let path = self.dir.join(format!("{target}.jsonl"));
            let f = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .with_context(|| format!("open {}", path.display()))?;
            self.files.insert(target.clone(), BufWriter::new(f));
        }
        let w = self.files.get_mut(&target).unwrap();
        let rows = batch_to_json(batch);
        let n = rows.len() as u64;
        for row in rows {
            writeln!(w, "{}", serde_json::to_string(&row)?)?;
        }
        Ok(n)
    }

    fn finish(&mut self) -> Result<()> {
        for w in self.files.values_mut() {
            w.flush()?;
        }
        Ok(())
    }
}

/// serde_json scalar → a native SQLite value.
fn to_sql_value(v: &Value) -> rusqlite::types::Value {
    use rusqlite::types::Value as S;
    match v {
        Value::Null => S::Null,
        Value::Bool(b) => S::Integer(*b as i64),
        Value::Number(n) => n
            .as_i64()
            .map(S::Integer)
            .or_else(|| n.as_f64().map(S::Real))
            .unwrap_or(S::Null),
        Value::String(s) => S::Text(s.clone()),
        other => S::Text(other.to_string()),
    }
}

// Per-table DDL comes from the GENERATED schema module (the fluessig catalog as committed
// code); a sink instantiates a table lazily on first write, so it creates only the tables it
// writes and `rename` gets the real typed schema + PK at the new name. See notes/design/multidb.md.
use crate::schema_gen::{TableSchema, PG_TABLES, SQLITE_TABLES};

/// A dialect table's DDL at the (possibly renamed) target name.
fn instantiate(tables: &[TableSchema], name: &str, target: &str) -> Option<String> {
    tables.iter().find(|t| t.name == name).map(|t| t.ddl.replace("__table__", target))
}

/// Placeholder style for [`upsert_sql`].
pub(crate) enum Ph {
    /// `?` — SQLite / DuckDB.
    Question,
    /// `$1..$n` — Postgres.
    Dollar,
}

/// THE upsert builder — one implementation for every SQL consumer (both sinks
/// and the driver sink). `table_sql` arrives already quoted/qualified.
/// `excluded` is lowercase: valid in SQLite and Postgres alike.
pub(crate) fn upsert_sql(table_sql: &str, cols: &[String], pk: &[String], ph: Ph) -> String {
    let quoted = cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
    let placeholders = match ph {
        Ph::Question => std::iter::repeat("?").take(cols.len()).collect::<Vec<_>>().join(", "),
        Ph::Dollar => (1..=cols.len()).map(|i| format!("${i}")).collect::<Vec<_>>().join(", "),
    };
    if pk.is_empty() {
        return format!("INSERT INTO {table_sql} ({quoted}) VALUES ({placeholders})");
    }
    let conflict = pk.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
    let non_pk: Vec<&String> = cols.iter().filter(|c| !pk.contains(c)).collect();
    if non_pk.is_empty() {
        format!("INSERT INTO {table_sql} ({quoted}) VALUES ({placeholders}) ON CONFLICT ({conflict}) DO NOTHING")
    } else {
        let set = non_pk
            .iter()
            .map(|c| format!("\"{c}\" = excluded.\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ");
        format!("INSERT INTO {table_sql} ({quoted}) VALUES ({placeholders}) ON CONFLICT ({conflict}) DO UPDATE SET {set}")
    }
}

/// Build the INSERT/UPSERT for a table given its columns + PK columns.
fn insert_sql(table: &str, cols: &[String], pk: &[String]) -> String {
    upsert_sql(&format!("\"{table}\""), cols, pk, Ph::Question)
}

/// Write the change stream into a **SQLite** file, upserting by primary key.
///
/// Tables come from hand-written migrations (`migrations/sqlite/`), applied on
/// open and tracked in `entl_migrations`. Each batch upserts on the table's PK
/// (`INSERT … ON CONFLICT … DO UPDATE`), so re-runs are idempotent; `Replace`
/// clears the table once per run first. Object-ids + timestamps land as TEXT
/// (hex / RFC3339). A table with no migration falls back to a typeless
/// auto-create + plain insert.
pub struct SqliteSink {
    conn: rusqlite::Connection,
    select: SinkSelect,
    /// Cached PK columns per *target* table (empty = no PK → plain insert).
    pk: HashMap<String, Vec<String>>,
    cleared: HashSet<String>,
}

impl SqliteSink {
    pub fn open(path: impl AsRef<Path>, select: SinkSelect) -> Result<Self> {
        let conn = rusqlite::Connection::open(path.as_ref())
            .with_context(|| format!("open sqlite {}", path.as_ref().display()))?;
        conn.pragma_update(None, "journal_mode", "WAL").ok();
        Ok(Self {
            conn,
            select,
            pk: HashMap::new(),
            cleared: HashSet::new(),
        })
    }

    /// PK columns of a table (via pragma), or `None` if the table doesn't exist.
    fn pk_of(&self, table: &str) -> Result<Option<Vec<String>>> {
        let mut stmt = self.conn.prepare(&format!("PRAGMA table_info(\"{table}\")"))?;
        let mut rows: Vec<(i64, String)> = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(5)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        if rows.is_empty() {
            return Ok(None);
        }
        rows.retain(|(pk, _)| *pk > 0);
        rows.sort_by_key(|(pk, _)| *pk);
        Ok(Some(rows.into_iter().map(|(_, n)| n).collect()))
    }

    /// Ensure the `target` table exists (instantiating the `canonical` table's template at the
    /// target name) and return its PK columns. A table with no template is created typeless from
    /// the batch columns (no PK).
    fn ensure(&mut self, canonical: &str, target: &str, cols: &[String]) -> Result<Vec<String>> {
        if let Some(pk) = self.pk.get(target) {
            return Ok(pk.clone());
        }
        if self.pk_of(target)?.is_none() {
            if let Some(ddl) = instantiate(SQLITE_TABLES, canonical, target) {
                self.conn.execute_batch(&ddl)?;
            } else {
                // No template for this table — typeless create (SQLite is dynamically typed), no PK.
                let quoted = cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
                self.conn
                    .execute_batch(&format!("CREATE TABLE IF NOT EXISTS \"{target}\" ({quoted})"))?;
            }
        }
        let pk = self.pk_of(target)?.unwrap_or_default();
        self.pk.insert(target.to_string(), pk.clone());
        Ok(pk)
    }
}

impl Sink for SqliteSink {
    fn apply(&mut self, batch: &ChangeBatch) -> Result<u64> {
        if !self.select.included(&batch.table) {
            return Ok(0);
        }
        let target = self.select.target(&batch.table);
        let cols: Vec<String> = batch
            .batch
            .schema()
            .fields()
            .iter()
            .map(|f| f.name().clone())
            .collect();
        let pk = self.ensure(&batch.table, &target, &cols)?;

        if batch.op == ChangeOp::Replace && self.cleared.insert(target.clone()) {
            self.conn.execute(&format!("DELETE FROM \"{target}\""), [])?;
        }

        let sql = insert_sql(&target, &cols, &pk);
        let tx = self.conn.transaction()?;
        {
            let mut stmt = tx.prepare(&sql)?;
            let arr = batch.batch.columns();
            for i in 0..batch.batch.num_rows() {
                let vals: Vec<rusqlite::types::Value> =
                    arr.iter().map(|c| to_sql_value(&cell_json(c, i))).collect();
                stmt.execute(rusqlite::params_from_iter(vals.iter()))?;
            }
        }
        tx.commit()?;
        Ok(batch.batch.num_rows() as u64)
    }
}

/// Arrow type → Postgres column type, for creating tables with no template. Templated tables
/// carry their own types; this only covers unknown tables. Object-ids + timestamps mirror the
/// template mapping.
fn pg_type(dt: &DataType) -> &'static str {
    match dt {
        DataType::Int32 => "integer",
        DataType::Int64 => "bigint",
        DataType::Float64 => "double precision",
        DataType::Boolean => "boolean",
        DataType::Timestamp(TimeUnit::Microsecond, _) => "timestamptz",
        _ => "text", // Utf8 / Binary(hex) / everything else
    }
}

/// Build the INSERT/UPSERT for Postgres (`$n` placeholders).
fn pg_insert_sql(table: &str, cols: &[String], pk: &[String]) -> String {
    upsert_sql(&format!("\"{table}\""), cols, pk, Ph::Dollar)
}

/// One JSON cell → a boxed Postgres parameter of the column's inferred type. NULL and value use
/// the same `Option<T>` so the bound type always matches the target column.
fn pg_value(ty: &Type, v: &Value) -> Box<dyn ToSql + Sync> {
    fn b<T: ToSql + Sync + 'static>(o: Option<T>) -> Box<dyn ToSql + Sync> {
        Box::new(o)
    }
    match *ty {
        Type::INT8 => b(v.as_i64()),
        Type::INT4 => b(v.as_i64().map(|n| n as i32)),
        Type::INT2 => b(v.as_i64().map(|n| n as i16)),
        Type::FLOAT8 => b(v.as_f64()),
        Type::FLOAT4 => b(v.as_f64().map(|n| n as f32)),
        Type::BOOL => b(v.as_bool()),
        Type::TIMESTAMPTZ => b(v
            .as_str()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))),
        _ => b(v.as_str().map(|s| s.to_string())), // text / everything else
    }
}

/// The `sslmode` from a Postgres URL's query string (libpq default `prefer`).
fn sslmode(url: &str) -> &str {
    url.split(['?', '&'])
        .find_map(|kv| kv.strip_prefix("sslmode="))
        .unwrap_or("prefer")
}

/// Write the change stream into a **Postgres** database, upserting by primary key.
///
/// Tables come from hand-written migrations (`migrations/postgres/`), applied on open into the
/// target `schema` (default `entl`, so entl's tables never collide with the app's) and tracked
/// in `entl_migrations`. Each batch upserts on the table's PK; `Replace` clears once per run.
/// Object-ids land as `text` (hex), timestamps as `timestamptz`. TLS follows the URL's `sslmode`
/// (libpq semantics): `disable` = plaintext; `prefer` (default)/`require`/`allow` = encrypt
/// without verifying the cert (works with managed Postgres' private-CA certs out of the box);
/// `verify-ca`/`verify-full` = encrypt *and* verify the server certificate.
pub struct PostgresSink {
    client: Client,
    schema: String,
    select: SinkSelect,
    /// Cached PK columns per *target* table (empty = no PK → plain insert).
    pk: HashMap<String, Vec<String>>,
    cleared: HashSet<String>,
}

impl PostgresSink {
    pub fn connect(url: &str, select: SinkSelect) -> Result<Self> {
        let schema = select.schema.clone().unwrap_or_else(|| "entl".to_string());
        // A TLS connector is always supplied; the URL's `sslmode` decides whether it's used and
        // whether the cert is verified. Only verify-ca/verify-full verify (libpq semantics), so
        // `require` encrypts against managed Postgres' private-CA certs without extra config.
        let verify = matches!(sslmode(url), "verify-ca" | "verify-full");
        let tls = native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(!verify)
            .danger_accept_invalid_hostnames(!verify)
            .build()
            .context("build TLS connector")?;
        let tls = postgres_native_tls::MakeTlsConnector::new(tls);
        let mut client = Client::connect(url, tls).with_context(|| format!("connect postgres {url}"))?;
        // All unqualified DDL/DML lands in the target schema.
        client.batch_execute(&format!(
            "CREATE SCHEMA IF NOT EXISTS \"{schema}\"; SET search_path TO \"{schema}\";"
        ))?;
        Ok(Self {
            client,
            schema,
            select,
            pk: HashMap::new(),
            cleared: HashSet::new(),
        })
    }

    /// PK columns of a table in the target schema (catalog lookup). `None` = table doesn't exist.
    fn pk_of(&mut self, table: &str) -> Result<Option<Vec<String>>> {
        let rows = self.client.query(
            "SELECT a.attname \
             FROM pg_index i \
             JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) \
             WHERE i.indrelid = format('%I.%I', $1::text, $2::text)::regclass AND i.indisprimary \
             ORDER BY array_position(i.indkey::int2[], a.attnum)",
            &[&self.schema, &table],
        );
        match rows {
            Ok(rows) if !rows.is_empty() => {
                Ok(Some(rows.iter().map(|r| r.get::<_, String>(0)).collect()))
            }
            Ok(_) => Ok(Some(Vec::new())), // exists, no PK
            // Only "relation does not exist" means absent; any other error is a real failure
            // we must not swallow into an empty PK (which would silently drop the upsert).
            Err(e) if e.code() == Some(&postgres::error::SqlState::UNDEFINED_TABLE) => Ok(None),
            Err(e) => Err(e).context(format!("pk lookup for {table}")),
        }
    }

    /// Ensure the `target` table exists (instantiating the `canonical` table's template at the
    /// target name, in the current schema) and return its PK columns. A table with no template is
    /// created from the batch's Arrow schema (no PK).
    fn ensure(&mut self, canonical: &str, target: &str, batch: &ChangeBatch) -> Result<Vec<String>> {
        if let Some(pk) = self.pk.get(target) {
            return Ok(pk.clone());
        }
        if self.pk_of(target)?.is_none() {
            if let Some(ddl) = instantiate(PG_TABLES, canonical, target) {
                self.client.batch_execute(&ddl)?;
            } else {
                // No template — create from the Arrow schema (typed), no PK.
                let cols_ddl = batch
                    .batch
                    .schema()
                    .fields()
                    .iter()
                    .map(|f| format!("\"{}\" {}", f.name(), pg_type(f.data_type())))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.client
                    .batch_execute(&format!("CREATE TABLE IF NOT EXISTS \"{target}\" ({cols_ddl})"))?;
            }
        }
        let pk = self.pk_of(target)?.unwrap_or_default();
        self.pk.insert(target.to_string(), pk.clone());
        Ok(pk)
    }
}

impl Sink for PostgresSink {
    fn apply(&mut self, batch: &ChangeBatch) -> Result<u64> {
        if !self.select.included(&batch.table) {
            return Ok(0);
        }
        let target = self.select.target(&batch.table);
        let cols: Vec<String> = batch
            .batch
            .schema()
            .fields()
            .iter()
            .map(|f| f.name().clone())
            .collect();
        let pk = self.ensure(&batch.table, &target, batch)?;

        if batch.op == ChangeOp::Replace && self.cleared.insert(target.clone()) {
            self.client.execute(&format!("DELETE FROM \"{target}\""), &[])?;
        }

        let sql = pg_insert_sql(&target, &cols, &pk);
        let stmt = self.client.prepare(&sql)?; // PG infers each $n's type from the target column
        let ptypes: Vec<Type> = stmt.params().to_vec();
        let arr = batch.batch.columns();

        let mut tx = self.client.transaction()?;
        for i in 0..batch.batch.num_rows() {
            let vals: Vec<Box<dyn ToSql + Sync>> = (0..cols.len())
                .map(|c| pg_value(&ptypes[c], &cell_json(&arr[c], i)))
                .collect();
            let params: Vec<&(dyn ToSql + Sync)> = vals.iter().map(|v| v.as_ref()).collect();
            tx.execute(&stmt, &params)?;
        }
        tx.commit()?;
        Ok(batch.batch.num_rows() as u64)
    }
}
