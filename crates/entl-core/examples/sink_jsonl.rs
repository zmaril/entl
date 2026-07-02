//! Stream a repo's git + forge changes into a directory of `<table>.jsonl` files.
//!
//!   cargo run -p entl-core --example sink_jsonl -- /path/to/repo /out/dir
//!
//! Read-only over the repo; forge needs a token (`gh auth token` / GH_TOKEN).

use entl_core::{build_sink, pull_into, Db, PullOpts, SinkSelect, SinkTarget};

fn main() -> anyhow::Result<()> {
    let repo = std::env::args().nth(1).expect("usage: sink_jsonl <repo> <outdir>");
    let outdir = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "/tmp/entl-jsonl".to_string());

    let db = Db::open(":memory:")?;
    db.migrate()?;

    let sink = build_sink(SinkTarget::Jsonl, Some(&outdir), SinkSelect::default())?;
    let outcome = pull_into(&db, &repo, sink, PullOpts::default())?;

    eprintln!(
        "streamed {} git commits + {} PRs into {} rows across {outdir}/*.jsonl",
        outcome.git.new_commits,
        outcome.github.as_ref().map(|g| g.pull_requests).unwrap_or(0),
        outcome.rows,
    );
    Ok(())
}
