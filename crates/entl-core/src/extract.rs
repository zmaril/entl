//! Extract — read canonical rows back out of any store (the reverse of a [`sink`](crate::sink)).
//!
//! The round-trip test's linchpin (notes/design, plan hashed-wishing-rose): every store is read
//! into the *same* canonical form the sinks write — object-ids as lowercase hex, timestamps as
//! RFC3339, booleans as JSON bools — so a DuckDB snapshot and a SQLite/Postgres/JSONL snapshot of
//! the same data compare equal. DuckDB reuses [`cell_json`](crate::sink::cell_json); the portable
//! stores already hold hex/RFC3339 text and only need boolean coercion (SQLite stores 0/1).

use std::collections::{BTreeMap, HashSet};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::sink::cell_json;

/// One row: column name → canonical value.
pub type Row = BTreeMap<String, Value>;
/// A store snapshot: table name → rows, sorted canonically for order-independent comparison.
pub type Snapshot = BTreeMap<String, Vec<Row>>;

/// The git-derived tables (present in every pull).
pub const GIT_TABLES: &[&str] = &["commits", "commit_parents", "file_changes", "refs"];

/// The object tables (present only when object ingest / `--blobs` ran).
pub const OBJ_TABLES: &[&str] = &["blobs", "trees", "tree_entries"];

/// Git metadata + objects — the full set needed to reconstruct a repo.
pub const GIT_FULL_TABLES: &[&str] = &[
    "commits", "commit_parents", "file_changes", "refs", "blobs", "trees", "tree_entries",
];

/// The forge tables the ingest writes that have sink templates (GraphQL-derived + events).
pub const FORGE_TABLES: &[&str] = &[
    "gh_pull_requests", "gh_issues", "gh_events", "gh_comments", "gh_labeled", "gh_labels",
    "gh_pr_reviews", "gh_pr_commits", "gh_requested_reviewers", "gh_review_comments", "gh_users",
    "gh_workflow_runs", "gh_check_runs",
];

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Sort a table's rows by their canonical JSON so two extractions compare regardless of DB order.
fn sort_rows(rows: &mut [Row]) {
    rows.sort_by_cached_key(|r| serde_json::to_string(r).unwrap_or_default());
}

/// Every BOOLEAN `(table, column)` in the entl schema — for coercing SQLite's 0/1 when no DuckDB
/// reference connection is available (e.g. reading a standalone SQLite/Postgres store).
pub const BOOL_COLUMNS: &[(&str, &str)] = &[
    ("commits", "is_merge"),
    ("commits", "gpg_signed"),
    ("refs", "is_symbolic"),
    ("blobs", "is_binary"),
    ("gh_pull_requests", "is_draft"),
];

fn static_bool_columns() -> HashSet<(String, String)> {
    BOOL_COLUMNS.iter().map(|(t, c)| (t.to_string(), c.to_string())).collect()
}

/// Extract a canonical snapshot from any store and serialize it to deterministic JSON — the shared
/// read surface the language bindings expose (`entl.extract`). `source` is
/// `duckdb` | `sqlite` | `jsonl` | `postgres`; `schema` applies to Postgres (default `entl`).
pub fn extract_json(source: &str, dest: &str, tables: &[String], schema: Option<&str>) -> Result<String> {
    let trefs: Vec<&str> = tables.iter().map(String::as_str).collect();
    let snap = match source {
        "duckdb" => {
            let d = crate::db::Db::open(dest)?;
            extract_duckdb(&d.conn, &trefs)?
        }
        "sqlite" => extract_sqlite(dest, &trefs, &static_bool_columns())?,
        "jsonl" => extract_jsonl(dest, &trefs)?,
        "postgres" | "postgresql" | "pg" => extract_postgres(dest, schema.unwrap_or("entl"), &trefs)?,
        other => anyhow::bail!("unknown extract source: {other} (duckdb|sqlite|jsonl|postgres)"),
    };
    Ok(serde_json::to_string(&snap)?)
}

/// The `(table, column)` pairs typed BOOLEAN in the DuckDB schema — used to coerce SQLite's 0/1.
pub fn bool_columns(conn: &duckdb::Connection) -> Result<HashSet<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT table_name, column_name FROM information_schema.columns \
         WHERE data_type = 'BOOLEAN'",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    Ok(rows.collect::<duckdb::Result<_>>()?)
}

/// Canonical snapshot of `tables` from a DuckDB connection (the reference form, S0). Missing
/// tables are skipped.
pub fn extract_duckdb(conn: &duckdb::Connection, tables: &[&str]) -> Result<Snapshot> {
    let present = table_set(conn)?;
    let mut snap = Snapshot::new();
    for &t in tables {
        if !present.contains(t) {
            snap.insert(t.to_string(), Vec::new());
            continue;
        }
        let mut stmt = conn.prepare(&format!("SELECT * FROM \"{t}\""))?;
        let batches: Vec<duckdb::arrow::record_batch::RecordBatch> = stmt.query_arrow([])?.collect();
        let mut rows = Vec::new();
        for batch in &batches {
            let schema = batch.schema();
            for i in 0..batch.num_rows() {
                let mut row = Row::new();
                for (c, field) in batch.columns().iter().zip(schema.fields()) {
                    row.insert(field.name().clone(), cell_json(c, i));
                }
                rows.push(row);
            }
        }
        sort_rows(&mut rows);
        snap.insert(t.to_string(), rows);
    }
    Ok(snap)
}

fn table_set(conn: &duckdb::Connection) -> Result<HashSet<String>> {
    let mut stmt = conn.prepare(
        "SELECT table_name FROM information_schema.tables WHERE table_schema = 'main'",
    )?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<duckdb::Result<_>>()?)
}

/// Canonical snapshot from a SQLite file. `bool_cols` (from [`bool_columns`]) tells us which
/// integer columns are really booleans.
pub fn extract_sqlite(
    path: &str,
    tables: &[&str],
    bool_cols: &HashSet<(String, String)>,
) -> Result<Snapshot> {
    use rusqlite::types::ValueRef;
    let conn = rusqlite::Connection::open(path).with_context(|| format!("open sqlite {path}"))?;
    let mut snap = Snapshot::new();
    for &t in tables {
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?",
                [t],
                |_| Ok(true),
            )
            .unwrap_or(false);
        if !exists {
            snap.insert(t.to_string(), Vec::new());
            continue;
        }
        let mut stmt = conn.prepare(&format!("SELECT * FROM \"{t}\""))?;
        let cols: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let mut rows_out = Vec::new();
        let mut q = stmt.query([])?;
        while let Some(r) = q.next()? {
            let mut row = Row::new();
            for (i, name) in cols.iter().enumerate() {
                let is_bool = bool_cols.contains(&(t.to_string(), name.clone()));
                let v = match r.get_ref(i)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(n) if is_bool => Value::Bool(n != 0),
                    ValueRef::Integer(n) => Value::from(n),
                    ValueRef::Real(f) => Value::from(f),
                    ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
                    ValueRef::Blob(b) => Value::String(hex(b)),
                };
                row.insert(name.clone(), v);
            }
            rows_out.push(row);
        }
        sort_rows(&mut rows_out);
        snap.insert(t.to_string(), rows_out);
    }
    Ok(snap)
}

/// Canonical snapshot from a Postgres schema. Typed reads → the same canonical form.
pub fn extract_postgres(url: &str, schema: &str, tables: &[&str]) -> Result<Snapshot> {
    use postgres::types::Type;
    use postgres::{Client, NoTls};
    let mut client = Client::connect(url, NoTls).with_context(|| format!("connect {url}"))?;
    let mut snap = Snapshot::new();
    for &t in tables {
        let exists: bool = client
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM information_schema.tables \
                 WHERE table_schema=$1 AND table_name=$2)",
                &[&schema, &t],
            )?
            .get(0);
        if !exists {
            snap.insert(t.to_string(), Vec::new());
            continue;
        }
        let stmt = format!("SELECT * FROM \"{schema}\".\"{t}\"");
        let pg_rows = client.query(&stmt, &[])?;
        let mut rows_out = Vec::new();
        for pr in &pg_rows {
            let mut row = Row::new();
            for (i, col) in pr.columns().iter().enumerate() {
                let v = match *col.type_() {
                    Type::BOOL => opt(pr.get::<_, Option<bool>>(i), Value::Bool),
                    Type::INT2 => opt(pr.get::<_, Option<i16>>(i), |n| Value::from(n)),
                    Type::INT4 => opt(pr.get::<_, Option<i32>>(i), |n| Value::from(n)),
                    Type::INT8 => opt(pr.get::<_, Option<i64>>(i), |n| Value::from(n)),
                    Type::FLOAT4 => opt(pr.get::<_, Option<f32>>(i), |n| Value::from(n)),
                    Type::FLOAT8 => opt(pr.get::<_, Option<f64>>(i), |n| Value::from(n)),
                    Type::TIMESTAMPTZ => opt(pr.get::<_, Option<DateTime<Utc>>>(i), |dt| {
                        Value::String(dt.to_rfc3339())
                    }),
                    _ => opt(pr.get::<_, Option<String>>(i), Value::String),
                };
                row.insert(col.name().to_string(), v);
            }
            rows_out.push(row);
        }
        sort_rows(&mut rows_out);
        snap.insert(t.to_string(), rows_out);
    }
    Ok(snap)
}

fn opt<T>(o: Option<T>, f: impl FnOnce(T) -> Value) -> Value {
    o.map(f).unwrap_or(Value::Null)
}

/// Canonical snapshot from a JSONL sink directory. Each `<table>.jsonl` is the op-tagged change
/// log; for a single fresh pull the final state is just the rows with `_op` stripped.
pub fn extract_jsonl(dir: &str, tables: &[&str]) -> Result<Snapshot> {
    use std::io::{BufRead, BufReader};
    let mut snap = Snapshot::new();
    for &t in tables {
        let path = std::path::Path::new(dir).join(format!("{t}.jsonl"));
        if !path.exists() {
            snap.insert(t.to_string(), Vec::new());
            continue;
        }
        let f = std::fs::File::open(&path).with_context(|| format!("open {}", path.display()))?;
        let mut rows = Vec::new();
        for line in BufReader::new(f).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let mut obj: Row = serde_json::from_str(&line)?;
            obj.remove("_op");
            rows.push(obj);
        }
        sort_rows(&mut rows);
        snap.insert(t.to_string(), rows);
    }
    Ok(snap)
}

/// Human-readable diff of two snapshots (empty string ⇒ equal), for test failures.
pub fn diff(a: &Snapshot, b: &Snapshot) -> String {
    let mut out = String::new();
    let tables: std::collections::BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    for t in tables {
        let ea = a.get(t).map(Vec::as_slice).unwrap_or(&[]);
        let eb = b.get(t).map(Vec::as_slice).unwrap_or(&[]);
        if ea != eb {
            out.push_str(&format!(
                "table {t}: {} rows vs {} rows differ\n",
                ea.len(),
                eb.len()
            ));
            for (i, (ra, rb)) in ea.iter().zip(eb).enumerate() {
                if ra != rb {
                    out.push_str(&format!(
                        "  row {i}:\n    A={}\n    B={}\n",
                        serde_json::to_string(ra).unwrap_or_default(),
                        serde_json::to_string(rb).unwrap_or_default()
                    ));
                    break;
                }
            }
        }
    }
    out
}
