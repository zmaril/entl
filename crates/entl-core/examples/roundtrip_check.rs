//! Sanity check for the extract machinery (pre-generator): ingest a real repo, then confirm the
//! DuckDB canonical snapshot (S0) equals the snapshot extracted from each sink store (S1).
//!
//!   cargo run -p entl-core --example roundtrip_check -- /path/to/repo
//!   ENTL_TEST_PG=postgres://... cargo run -p entl-core --example roundtrip_check -- /path/to/repo
//!
//! straitjacket-allow-file:duplication — the example harnesses share ingest +
//! snapshot setup boilerplate by design.

use entl_core::extract::{
    bool_columns, diff, extract_duckdb, extract_jsonl, extract_postgres, extract_sqlite,
    GIT_FULL_TABLES,
};
use entl_core::{build_sink, pull_into, Db, PullOpts, SinkSelect, SinkTarget};

fn main() -> anyhow::Result<()> {
    let repo = std::env::args()
        .nth(1)
        .expect("usage: roundtrip_check <repo>");
    let tmp = std::env::var("CLAUDE_JOB_DIR").unwrap_or_else(|_| "/tmp".into()) + "/tmp";
    std::fs::create_dir_all(&tmp).ok();

    let opts = || PullOpts {
        github: false,
        objects: true,
    };

    // --- SQLite ---
    let sqlite_path = format!("{tmp}/rt_check.sqlite");
    let _ = std::fs::remove_file(&sqlite_path);
    let db = Db::open(":memory:")?;
    db.migrate()?;
    let sink = build_sink(
        SinkTarget::Sqlite,
        Some(&sqlite_path),
        SinkSelect::default(),
    )?;
    pull_into(&db, &repo, sink, opts())?;
    let s0 = extract_duckdb(&db.conn, GIT_FULL_TABLES)?;
    let bcols = bool_columns(&db.conn)?;
    let s1 = extract_sqlite(&sqlite_path, GIT_FULL_TABLES, &bcols)?;
    report("sqlite", &s0, &s1);

    // --- JSONL ---
    let jsonl_dir = format!("{tmp}/rt_check_jsonl");
    let _ = std::fs::remove_dir_all(&jsonl_dir);
    let db2 = Db::open(":memory:")?;
    db2.migrate()?;
    let sink = build_sink(SinkTarget::Jsonl, Some(&jsonl_dir), SinkSelect::default())?;
    pull_into(&db2, &repo, sink, opts())?;
    let s0b = extract_duckdb(&db2.conn, GIT_FULL_TABLES)?;
    let s1b = extract_jsonl(&jsonl_dir, GIT_FULL_TABLES)?;
    report("jsonl", &s0b, &s1b);

    // --- Postgres (gated) ---
    if let Ok(url) = std::env::var("ENTL_TEST_PG") {
        let db3 = Db::open(":memory:")?;
        db3.migrate()?;
        // fresh schema
        let sel = SinkSelect {
            schema: Some("rt_check".into()),
            ..Default::default()
        };
        let sink = build_sink(SinkTarget::Postgres, Some(&url), sel)?;
        pull_into(&db3, &repo, sink, opts())?;
        let s0c = extract_duckdb(&db3.conn, GIT_FULL_TABLES)?;
        let s1c = extract_postgres(&url, "rt_check", GIT_FULL_TABLES)?;
        report("postgres", &s0c, &s1c);
    } else {
        eprintln!("postgres: skipped (set ENTL_TEST_PG to run)");
    }
    Ok(())
}

fn report(name: &str, s0: &entl_core::Snapshot, s1: &entl_core::Snapshot) {
    let d = diff(s0, s1);
    if d.is_empty() {
        let n: usize = s0.values().map(Vec::len).sum();
        println!(
            "{name}: OK  (S0 == S1, {n} rows across {} tables)",
            s0.len()
        );
    } else {
        println!("{name}: MISMATCH\n{d}");
    }
}
