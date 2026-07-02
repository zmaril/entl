//! P1 — store round-trip. For a generated git world: materialize → ingest into DuckDB (S0) →
//! sink into each store → extract (S1) → assert S0 == S1. DuckDB/SQLite/JSONL always run; Postgres
//! runs when `ENTL_TEST_PG` is set (a unique schema per case).

use std::sync::atomic::{AtomicUsize, Ordering};

use std::path::Path;

use entl_core::extract::{
    bool_columns, diff, extract_duckdb, extract_jsonl, extract_postgres, extract_sqlite,
    GIT_FULL_TABLES, GIT_TABLES,
};
use entl_core::{
    build_sink, pull_into, rebuild_from_snapshot, Db, PullOpts, SinkSelect, SinkTarget,
};
use entl_testkit::{arb_git_world, git, materialize};
use proptest::prelude::*;

static PG_SCHEMA: AtomicUsize = AtomicUsize::new(0);

/// Pull the repo into `db` (DuckDB) + `sink`; return the DuckDB reference snapshot S0.
fn pull(repo: &str, target: SinkTarget, dest: &str, sel: SinkSelect) -> (Db, entl_core::Snapshot) {
    let db = Db::open(":memory:").unwrap();
    db.migrate().unwrap();
    let sink = build_sink(target, Some(dest), sel).unwrap();
    pull_into(&db, repo, sink, PullOpts { github: false, objects: false }).unwrap();
    let s0 = extract_duckdb(&db.conn, GIT_TABLES).unwrap();
    (db, s0)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 32, max_shrink_iters: 200, ..ProptestConfig::default() })]

    #[test]
    fn p1_store_roundtrip(world in arb_git_world()) {
        let repo = tempfile::tempdir().unwrap();
        let oids = materialize(&world, repo.path()).unwrap();
        prop_assert_eq!(oids.len(), world.commits.len());
        let repo_str = repo.path().to_str().unwrap();

        // SQLite
        {
            let sdir = tempfile::tempdir().unwrap();
            let path = sdir.path().join("s.db");
            let path = path.to_str().unwrap();
            let (db, s0) = pull(repo_str, SinkTarget::Sqlite, path, SinkSelect::default());
            let bcols = bool_columns(&db.conn).unwrap();
            let s1 = extract_sqlite(path, GIT_TABLES, &bcols).unwrap();
            let d = diff(&s0, &s1);
            prop_assert!(d.is_empty(), "sqlite mismatch:\n{}", d);
        }

        // JSONL
        {
            let jdir = tempfile::tempdir().unwrap();
            let dir = jdir.path().to_str().unwrap();
            let (_db, s0) = pull(repo_str, SinkTarget::Jsonl, dir, SinkSelect::default());
            let s1 = extract_jsonl(dir, GIT_TABLES).unwrap();
            let d = diff(&s0, &s1);
            prop_assert!(d.is_empty(), "jsonl mismatch:\n{}", d);
        }

        // Postgres (gated)
        if let Ok(url) = std::env::var("ENTL_TEST_PG") {
            let schema = format!("rt_{}", PG_SCHEMA.fetch_add(1, Ordering::Relaxed));
            let sel = SinkSelect { schema: Some(schema.clone()), ..SinkSelect::default() };
            let (_db, s0) = pull(repo_str, SinkTarget::Postgres, &url, sel);
            let s1 = extract_postgres(&url, &schema, GIT_TABLES).unwrap();
            let d = diff(&s0, &s1);
            prop_assert!(d.is_empty(), "postgres mismatch:\n{}", d);
        }
    }
}

/// Branch/tag tips as `refname → oid`, sorted (byte-identical if reconstruction is faithful).
fn refs_of(repo: &Path) -> String {
    let mut v: Vec<String> = git(repo, &["for-each-ref", "--format=%(refname) %(objectname)", "refs/heads", "refs/tags"])
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    v.sort();
    v.join("\n")
}

/// The set of all reachable commit OIDs, sorted.
fn commits_of(repo: &Path) -> String {
    let mut v: Vec<String> = git(repo, &["rev-list", "--all"]).unwrap().lines().map(str::to_string).collect();
    v.sort();
    v.join("\n")
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, max_shrink_iters: 200, ..ProptestConfig::default() })]

    /// P2 — git reassembly. Full-fidelity ingest into a store, rebuild a repo from it, assert the
    /// rebuilt repo has byte-identical commit OIDs and refs (cryptographic round-trip).
    #[test]
    fn p2_git_reassembly(world in arb_git_world()) {
        let src = tempfile::tempdir().unwrap();
        materialize(&world, src.path()).unwrap();

        // Full-fidelity ingest (objects on) into a SQLite store.
        let sdir = tempfile::tempdir().unwrap();
        let spath = sdir.path().join("s.db");
        let spath = spath.to_str().unwrap();
        let db = Db::open(":memory:").unwrap();
        db.migrate().unwrap();
        let sink = build_sink(SinkTarget::Sqlite, Some(spath), SinkSelect::default()).unwrap();
        pull_into(&db, src.path().to_str().unwrap(), sink, PullOpts { github: false, objects: true }).unwrap();

        // Rebuild from the store and compare to the source repo.
        let bcols = bool_columns(&db.conn).unwrap();
        let snap = extract_sqlite(spath, GIT_FULL_TABLES, &bcols).unwrap();
        let dst = tempfile::tempdir().unwrap();
        rebuild_from_snapshot(&snap, dst.path()).unwrap();

        prop_assert_eq!(refs_of(src.path()), refs_of(dst.path()), "refs differ");
        prop_assert_eq!(commits_of(src.path()), commits_of(dst.path()), "reachable commits differ");
    }
}
