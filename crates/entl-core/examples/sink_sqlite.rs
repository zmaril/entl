//! Stream a repo's git + forge changes into a SQLite database file.
//!
//!   cargo run -p entl-core --example sink_sqlite -- /path/to/repo /out/file.db
//!
//! Read-only over the repo; forge needs a token (`gh auth token` / GH_TOKEN).

use entl_core::{build_sink, pull_into, Db, PullOpts, SinkSelect, SinkTarget};

fn main() -> anyhow::Result<()> {
    let repo = std::env::args()
        .nth(1)
        .expect("usage: sink_sqlite <repo> <out.db>");
    let out = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "/tmp/entl.db".to_string());
    let _ = std::fs::remove_file(&out);

    let db = Db::open(":memory:")?;
    db.migrate()?;

    let sink = build_sink(SinkTarget::Sqlite, Some(&out), SinkSelect::default())?;
    let outcome = pull_into(&db, &repo, sink, PullOpts::default())?;

    eprintln!(
        "streamed {} git commits + {} PRs into {} rows → {out}",
        outcome.git.new_commits,
        outcome
            .github
            .as_ref()
            .map(|g| g.pull_requests)
            .unwrap_or(0),
        outcome.rows,
    );
    Ok(())
}
