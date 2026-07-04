//! DuckDB connection + the custom migration runner.

use anyhow::Result;
use duckdb::Connection;

use crate::schema_gen::DUCKDB_TABLES;

/// DuckDB-only helpers applied after the tables on every (re)build: DAG-walk macros + hex views.
/// Hand-written (genuinely dialect-specific — the extras mechanism); everything tabular is
/// generated (`schema_gen`).
pub const DUCKDB_EXTRAS: &str = include_str!("../migrations/duckdb/extras.sql");

/// An open entl database.
pub struct Db {
    pub conn: Connection,
}

impl Db {
    /// Open (or create) the DuckDB file. Use ":memory:" for ephemeral.
    pub fn open(path: &str) -> Result<Self> {
        let conn = if path == ":memory:" {
            Connection::open_in_memory()?
        } else {
            Connection::open(path)?
        };
        Ok(Self { conn })
    }

    /// Wrap an existing connection (e.g. a `try_clone()` for a worker thread).
    pub fn from_conn(conn: Connection) -> Self {
        Self { conn }
    }

    /// Apply the schema. The store is a *derived cache* (notes/design + AGENTS.md): there is no
    /// data migration. The schema (all per-table templates + the DuckDB extras) is content-hashed;
    /// if it's unchanged this is a no-op, otherwise every table is dropped and re-created and the
    /// caller re-ingests. Edit the templates freely — no numbered migrations to maintain.
    pub fn migrate(&self) -> Result<()> {
        let version = schema_hash();
        self.conn
            .execute_batch("CREATE TABLE IF NOT EXISTS _entl_schema (version TEXT);")?;
        let stored: Option<String> = self
            .conn
            .query_row("SELECT version FROM _entl_schema LIMIT 1", [], |r| r.get(0))
            .ok();
        if stored.as_deref() == Some(version.as_str()) {
            return Ok(()); // schema up to date
        }

        // Schema changed (or fresh DB) → drop every table and rebuild.
        let existing: Vec<String> = {
            let mut stmt = self.conn.prepare(
                "SELECT table_name FROM information_schema.tables \
                 WHERE table_schema = 'main' AND table_type = 'BASE TABLE' \
                 AND table_name <> '_entl_schema'",
            )?;
            stmt.query_map([], |r| r.get::<_, String>(0))?
                .collect::<duckdb::Result<_>>()?
        };
        for t in existing {
            self.conn.execute_batch(&format!("DROP TABLE IF EXISTS \"{t}\" CASCADE;"))?;
        }
        for t in DUCKDB_TABLES {
            self.conn.execute_batch(&t.ddl.replace("__table__", t.name))?;
        }
        self.conn.execute_batch(DUCKDB_EXTRAS)?;
        self.conn.execute("DELETE FROM _entl_schema", [])?;
        self.conn
            .execute("INSERT INTO _entl_schema (version) VALUES (?)", [&version])?;
        Ok(())
    }

    /// Number of base tables in the store (for diagnostics/tests).
    pub fn table_count(&self) -> Result<i64> {
        let n = self.conn.query_row(
            "SELECT count(*) FROM information_schema.tables \
             WHERE table_schema = 'main' AND table_type = 'BASE TABLE' AND table_name <> '_entl_schema'",
            [],
            |r| r.get(0),
        )?;
        Ok(n)
    }

    /// Run a query and render the result as a pretty text table (via Arrow).
    pub fn query_table(&self, sql: &str) -> Result<String> {
        let mut stmt = self.conn.prepare(sql)?;
        let batches: Vec<duckdb::arrow::record_batch::RecordBatch> =
            stmt.query_arrow([])?.collect();
        Ok(duckdb::arrow::util::pretty::pretty_format_batches(&batches)?.to_string())
    }

    /// Run a query; the whole result as one Arrow IPC stream (schema + every
    /// batch). The dataframe on-ramp: pyarrow / apache-arrow JS / red-arrow
    /// decode it directly. `stmt.schema()` (valid after execution) covers the
    /// zero-row case — the stream still carries the schema.
    pub fn query_arrow_ipc(&self, sql: &str) -> Result<Vec<u8>> {
        let mut stmt = self.conn.prepare(sql)?;
        let batches: Vec<duckdb::arrow::record_batch::RecordBatch> =
            stmt.query_arrow([])?.collect();
        let schema = stmt.schema();
        let mut buf = Vec::new();
        let mut w = arrow::ipc::writer::StreamWriter::try_new(&mut buf, schema.as_ref())?;
        for b in &batches {
            w.write(b)?;
        }
        w.finish()?;
        drop(w);
        Ok(buf)
    }
}

/// A stable content hash of the whole schema (the generated DuckDB tables + the extras). FNV-1a,
/// not std's `DefaultHasher` (which can drift across std versions and cause spurious rebuilds).
/// Regenerating `schema_gen.rs` with any change (i.e. editing `entl.tsp`) or editing `extras.sql`
/// bumps it, triggering a drop-&-rebuild on next open — no manual version bumping.
fn schema_hash() -> String {
    fn fnv(mut h: u64, s: &str) -> u64 {
        for b in s.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    }
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for t in DUCKDB_TABLES {
        h = fnv(h, t.ddl);
    }
    h = fnv(h, DUCKDB_EXTRAS);
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_arrow_ipc_round_trips_and_covers_zero_rows() {
        let db = Db::open(":memory:").unwrap();
        let decode = |ipc: Vec<u8>| {
            let r = arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(ipc), None)
                .unwrap();
            let schema = r.schema();
            let batches: Vec<_> = r.map(|b| b.unwrap()).collect();
            (schema, batches)
        };

        let (schema, batches) = decode(db.query_arrow_ipc("SELECT 1 AS x").unwrap());
        assert_eq!(schema.field(0).name(), "x");
        assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 1);

        // Zero rows: the stream still carries the schema and decodes cleanly.
        let (schema, batches) = decode(db.query_arrow_ipc("SELECT 1 AS y WHERE false").unwrap());
        assert_eq!(schema.field(0).name(), "y");
        assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 0);
    }
}
