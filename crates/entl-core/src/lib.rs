//! entl-core — the engine: git + GitHub data into DuckDB.
//! Write path uses the raw `duckdb` crate (Appender for bulk ingest); the
//! schema is hand-written SQL migrations (no ORM in the core).

pub mod conflicts;
pub mod db;
pub mod diff;
pub mod github;
pub mod gitops;
pub mod extract;
pub mod gitwrite;
pub mod ingest;
pub mod migrations;
pub mod objects;
pub mod pull;
pub mod rebuild;
pub mod sink;
pub mod stream;

pub use conflicts::{analyze_conflicts, ConflictStats};
pub use db::Db;
pub use diff::{diff_commits, file_at, FileDiff};
pub use gitops::{branch_exists, commit_bodies, current_branch, ls_remote_heads};
pub use github::{ingest_github, ingest_github_streamed, GithubIngest};
pub use ingest::{ingest_git, ingest_git_streamed, GitIngest};
pub use objects::{ingest_git_objects, ObjIngest};
pub use extract::{
    extract_duckdb, extract_json, extract_jsonl, extract_postgres, extract_sqlite, Snapshot,
};
pub use gitwrite::{fast_import_stream, git, import, SnapCommit, SnapRef};
pub use pull::{build_sink, pull_into, PullOpts, SinkOutcome, SinkTarget};
pub use rebuild::{rebuild_from_snapshot, rebuild_from_store};
pub use sink::{drain, JsonlSink, PostgresSink, Sink, SinkSelect, SqliteSink};
pub use stream::{change_channel, ChangeBatch, ChangeOp, ChangeStream, ChangeSink, Poll};
