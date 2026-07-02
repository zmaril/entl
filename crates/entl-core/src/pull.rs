//! Pull-and-sink — the one-shot "sync a repo into a target database" entry point.
//!
//! Composes the streaming ingest ([`ingest_git_streamed`](crate::ingest_git_streamed) +
//! [`ingest_github_streamed`](crate::ingest_github_streamed)) with a [`Sink`] over the
//! [change stream](crate::stream): a consumer thread [`drain`]s the stream into the sink while
//! this thread pulls git + forge and emits batches. See notes/design/multidb.md — the
//! data-moving is all here in Rust; a binding just names the [`SinkTarget`] to switch on.

use std::str::FromStr;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use anyhow::{anyhow, Result};

use crate::db::Db;
use crate::github::{ingest_github_streamed, GithubIngest};
use crate::ingest::{ingest_git_streamed, GitIngest};
use crate::sink::{drain, JsonlSink, PostgresSink, Sink, SinkSelect, SqliteSink};
use crate::stream::change_channel;

/// A built-in sink target. The source-of-truth enum; each language binding declares its own
/// native enum (napi / PyO3) and maps into this one, so a new target is added here once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinkTarget {
    /// A SQLite database file (upsert by primary key; see [`SqliteSink`]).
    Sqlite,
    /// A directory of append-only per-table `.jsonl` change logs (see [`JsonlSink`]).
    Jsonl,
    /// A Postgres database (upsert by primary key, into a target schema; see [`PostgresSink`]).
    Postgres,
}

impl SinkTarget {
    pub fn as_str(&self) -> &'static str {
        match self {
            SinkTarget::Sqlite => "sqlite",
            SinkTarget::Jsonl => "jsonl",
            SinkTarget::Postgres => "postgres",
        }
    }
}

impl FromStr for SinkTarget {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "sqlite" => Ok(SinkTarget::Sqlite),
            "jsonl" => Ok(SinkTarget::Jsonl),
            "postgres" | "postgresql" | "pg" => Ok(SinkTarget::Postgres),
            other => Err(anyhow!("unknown sink target: {other} (expected sqlite | jsonl | postgres)")),
        }
    }
}

/// Build a boxed sink for `target`. `path` is the SQLite file, the JSONL directory, or the
/// Postgres connection URL — required for all. `select` narrows the tables written (and, for
/// Postgres, the target schema).
pub fn build_sink(target: SinkTarget, path: Option<&str>, select: SinkSelect) -> Result<Box<dyn Sink + Send>> {
    let path = path.ok_or_else(|| anyhow!("sink target {} needs a path/url", target.as_str()))?;
    Ok(match target {
        SinkTarget::Sqlite => Box::new(SqliteSink::open(path, select)?),
        SinkTarget::Jsonl => Box::new(JsonlSink::new(path, select)?),
        SinkTarget::Postgres => Box::new(PostgresSink::connect(path, select)?),
    })
}

/// What to pull in a [`pull_into`] cycle.
pub struct PullOpts {
    /// Also pull GitHub (events/PRs/issues/Actions). Requires a token; see the github module.
    pub github: bool,
    /// Also ingest the object graph (trees/tree_entries/blobs + raw content) — the full-fidelity
    /// mirror needed to `rebuild` a repo. Heavy for large repos; off by default.
    pub objects: bool,
}

impl Default for PullOpts {
    fn default() -> Self {
        Self { github: true, objects: false }
    }
}

/// The result of a [`pull_into`] cycle: what git + forge produced, and how many rows the sink
/// applied.
pub struct SinkOutcome {
    pub git: GitIngest,
    /// `None` when `PullOpts::github` was false.
    pub github: Option<GithubIngest>,
    /// Total rows the sink applied across all change batches.
    pub rows: u64,
}

/// Pull `repo` into both `db` (the streamed ingest always writes DuckDB) and `sink`.
///
/// A consumer thread drains the change stream into `sink` while this thread runs the ingest.
/// Git is required; GitHub is attempted when `opts.github` and its error propagates (same
/// contract as loading git + github separately). `db` may be an in-memory DuckDB if the caller
/// only wants the target sink.
pub fn pull_into(db: &Db, repo: &str, sink: Box<dyn Sink + Send>, opts: PullOpts) -> Result<SinkOutcome> {
    let (csink, stream) = change_channel(512);

    // Consumer owns the sink; it drains until the producer drops `csink` (channel closes).
    let mut sink = sink;
    let consumer = std::thread::spawn(move || drain(&stream, &mut *sink, Duration::from_secs(5)));

    let counter = AtomicU64::new(0);
    let git = ingest_git_streamed(db, repo, &counter, Some(&csink))?;
    if opts.objects {
        crate::objects::ingest_git_objects(db, repo, Some(&csink))?;
    }
    let github = if opts.github {
        Some(ingest_github_streamed(db, repo, Some(&csink))?)
    } else {
        None
    };

    // Close the stream so the consumer sees `Closed` and returns.
    drop(csink);
    let rows = consumer
        .join()
        .map_err(|_| anyhow!("sink consumer thread panicked"))??;

    Ok(SinkOutcome { git, github, rows })
}
