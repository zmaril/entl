//! entl-core — the engine: git + GitHub data into DuckDB.
//! Write path uses the raw `duckdb` crate (Appender for bulk ingest); the
//! schema is hand-written SQL migrations (no ORM in the core).

pub mod arrow_bridge;
pub mod binding;
pub mod conflicts;
pub mod db;
pub mod diff;
pub mod driver;
pub mod extract;
pub mod github;
pub mod gitops;
pub mod gitwrite;
pub mod ingest;
pub mod objects;
pub mod pull;
pub mod rebuild;
pub mod schema_gen;
pub mod sink;
pub mod stream;

/// The Arrow batch type the change stream carries — re-exported so the generated
/// bindings can name it (`entl_core::RecordBatch`) without their own arrow dep. This is
/// entl's OWN arrow (the `arrow` crate), which floats independently of the arrow the
/// `duckdb` crate bundles; duckdb-produced batches cross into it via `arrow_bridge`.
pub use arrow::record_batch::RecordBatch;
pub use conflicts::{analyze_conflicts, ConflictStats};
pub use db::Db;
pub use diff::{diff_commits, file_at, FileDiff};
pub use driver::{
    backfill, driver_tables, statement_channel, Dialect, DriverSink, Statement, StatementStream,
    StmtPoll,
};
pub use extract::{
    extract_duckdb, extract_json, extract_jsonl, extract_postgres, extract_sqlite, Snapshot,
};
pub use github::{ingest_github, ingest_github_streamed, GithubIngest};
pub use gitops::{branch_exists, commit_bodies, current_branch, ls_remote_heads};
pub use gitwrite::{fast_import_stream, git, import, SnapCommit, SnapRef};
pub use ingest::{ingest_git, ingest_git_streamed, GitIngest};
pub use objects::{ingest_git_objects, ObjIngest};
pub use pull::{build_sink, pull_into, PullOpts, SinkOutcome, SinkTarget};
pub use rebuild::{rebuild_from_snapshot, rebuild_from_store, rebuild_store};
pub use sink::{drain, JsonlSink, PostgresSink, Sink, SinkSelect, SqliteSink};
pub use stream::{
    batch_ipc, batch_to_ffi, change_channel, ChangeBatch, ChangeOp, ChangeSink, ChangeStream, Poll,
};
