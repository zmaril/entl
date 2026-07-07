//! Generate a shared round-trip corpus for the cross-language matrix (Phase 4, notes/design/
//! testing.md). Writes, per world, a materialized git repo + the reference canonical snapshot
//! (`expected.json`, from DuckDB) that every language's `sink` + `extract` must reproduce.
//!
//!   cargo run -p entl-testkit --bin gen_corpus -- <outdir> [count]
//!
//! Worlds are drawn from the proptest generator with deterministic seeds, so the corpus is stable.

use std::path::Path;
use std::sync::atomic::AtomicU64;

use entl_core::extract::GIT_TABLES;
use entl_core::{extract_duckdb, ingest_git, Db};
use entl_testkit::{arb_git_world, materialize};
use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

fn main() -> anyhow::Result<()> {
    let outdir = std::env::args()
        .nth(1)
        .expect("usage: gen_corpus <outdir> [count]");
    let count: u32 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(8);
    std::fs::create_dir_all(&outdir)?;

    let tables: Vec<&str> = GIT_TABLES.to_vec();
    for i in 0..count {
        // Deterministic per-world seed → a stable, varied corpus.
        let seed = [(i as u8).wrapping_add(1); 32];
        let rng = TestRng::from_seed(RngAlgorithm::ChaCha, &seed);
        let mut runner = TestRunner::new_with_rng(Config::default(), rng);
        let world = arb_git_world().new_tree(&mut runner).unwrap().current();

        let dir = format!("{outdir}/w{i}");
        let repo = format!("{dir}/repo");
        std::fs::create_dir_all(&repo)?;
        materialize(&world, Path::new(&repo))?;

        // Reference snapshot from DuckDB (git-only) — what every language must reproduce.
        let db = Db::open(":memory:")?;
        db.migrate()?;
        ingest_git(&db, &repo, &AtomicU64::new(0))?;
        let snap = extract_duckdb(&db.conn, &tables)?;
        std::fs::write(
            format!("{dir}/expected.json"),
            serde_json::to_string(&snap)?,
        )?;
    }
    eprintln!("wrote {count} worlds → {outdir}");
    Ok(())
}
