//! DuckDB connection + the custom migration runner.

use anyhow::Result;
use duckdb::Connection;
use std::collections::HashSet;

use crate::migrations::MIGRATIONS;

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

    /// Apply pending migrations idempotently (tracked in `_entl_migrations`).
    pub fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS _entl_migrations (
                 name TEXT PRIMARY KEY,
                 applied_at TIMESTAMPTZ DEFAULT now()
             );",
        )?;

        let applied: HashSet<String> = {
            let mut stmt = self.conn.prepare("SELECT name FROM _entl_migrations")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect::<duckdb::Result<_>>()?
        };

        for (name, sql) in MIGRATIONS {
            if applied.contains(*name) {
                continue;
            }
            self.conn.execute_batch(sql)?;
            self.conn
                .execute("INSERT INTO _entl_migrations (name) VALUES (?)", [*name])?;
        }
        Ok(())
    }

    /// Count of applied migrations (for diagnostics/tests).
    pub fn applied_migrations(&self) -> Result<i64> {
        let n = self
            .conn
            .query_row("SELECT count(*) FROM _entl_migrations", [], |r| r.get(0))?;
        Ok(n)
    }

    /// Run a query and render the result as a pretty text table (via Arrow).
    pub fn query_table(&self, sql: &str) -> Result<String> {
        let mut stmt = self.conn.prepare(sql)?;
        let batches: Vec<duckdb::arrow::record_batch::RecordBatch> =
            stmt.query_arrow([])?.collect();
        Ok(duckdb::arrow::util::pretty::pretty_format_batches(&batches)?.to_string())
    }
}
