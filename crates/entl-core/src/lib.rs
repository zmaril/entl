//! entl-core — the engine: git + GitHub data into DuckDB.
//! Write path uses the raw `duckdb` crate (Appender for bulk ingest); the
//! schema is hand-written SQL migrations (no ORM in the core).

pub mod conflicts;
pub mod db;
pub mod diff;
pub mod github;
pub mod gitops;
pub mod ingest;
pub mod migrations;

pub use conflicts::{analyze_conflicts, ConflictStats};
pub use db::Db;
pub use diff::{diff_commits, file_at, FileDiff};
pub use gitops::{branch_exists, commit_bodies, current_branch, ls_remote_heads};
pub use github::{ingest_github, GithubIngest};
pub use ingest::{ingest_git, GitIngest};
