//! Manual verification: stream a real repo's git + forge changes through `poll`.
//!
//!   cargo run -p entl-core --example verify_stream -- /path/to/repo
//!
//! Read-only (no `git fetch`); forge needs a token (`gh auth token` / GH_TOKEN).

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use entl_core::{change_channel, ingest_git_streamed, ingest_github_streamed, Db, Poll};

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .expect("usage: verify_stream <repo path>");

    let db = Db::open(":memory:")?;
    db.migrate()?;
    let (sink, changes) = change_channel(512);

    // Consumer thread: tally batches/rows per (table, op), and print the first
    // batch of each table so we can eyeball real data.
    let consumer = std::thread::spawn(move || {
        let mut tally: BTreeMap<(String, String), (usize, usize)> = BTreeMap::new();
        let mut shown: HashSet<String> = HashSet::new();
        loop {
            match changes.poll(Duration::from_secs(60)) {
                Poll::Batch(b) => {
                    let e = tally
                        .entry((b.table.clone(), b.op.as_str().to_string()))
                        .or_default();
                    e.0 += 1;
                    e.1 += b.len();
                    if shown.insert(b.table.clone()) {
                        println!("\n── first {} batch ({}, {} rows) ──", b.table, b.op.as_str(), b.len());
                        let p = b.pretty();
                        for line in p.lines().take(6) {
                            println!("{line}");
                        }
                    }
                }
                Poll::Idle => eprintln!("(idle — still pulling…)"),
                Poll::Closed => break,
            }
        }
        tally
    });

    let counter = AtomicU64::new(0);
    eprintln!("== streaming git ({path}) ==");
    let g = ingest_git_streamed(&db, &path, &counter, Some(&sink))?;
    eprintln!(
        "git ingest: {} commits, {} file_changes, {} refs",
        g.new_commits, g.file_changes, g.refs
    );

    eprintln!("== streaming forge ==");
    let f = ingest_github_streamed(&db, &path, Some(&sink))?;
    eprintln!(
        "forge ingest: {} PRs, {} reviews, {} issues, {} comments, {} events, {} runs, {} checks",
        f.pull_requests, f.reviews, f.issues, f.comments, f.events, f.workflow_runs, f.check_runs
    );

    drop(sink); // close the stream → consumer exits
    let tally = consumer.join().unwrap();

    eprintln!("\n== change-stream tally (table · op → batches, rows) ==");
    for ((t, op), (batches, rows)) in &tally {
        eprintln!("  {t:<22} {op:<8} {batches:>3} batches, {rows:>6} rows");
    }
    Ok(())
}
