//! The schema machinery, shared by the DuckDB store ([`crate::db`]) and the portable sinks
//! ([`crate::sink`]). Every table is one hand-written DDL template per dialect in
//! `migrations/<dialect>/tables/<table>.sql`, with `__table__` substituted for the (possibly
//! renamed) target table name.
//!
//! Same machinery, three dialects. The DuckDB store applies *all* of its templates + the
//! DuckDB-only `extras.sql` on open and rebuilds on any schema change; the portable sinks
//! instantiate a template lazily on first write (and support rename). The store is a *derived
//! cache*: there is no data migration — a schema change drops the tables and the caller
//! re-ingests (see [`crate::db::Db::migrate`] and AGENTS.md).

/// Expand to `&[(name, ddl_template)]` for a dialect, embedding each per-table `.sql` file.
macro_rules! table_templates {
    ($dialect:literal, $($name:literal),* $(,)?) => {
        &[$(($name, include_str!(concat!("../migrations/", $dialect, "/tables/", $name, ".sql")))),*]
    };
}

/// The DuckDB store's tables — a superset of the sink tables (it also holds `repos`, `sync_state`,
/// `conflicts`, and the Actions children `gh_jobs`/`gh_steps`/`gh_workflows`/`gh_commit_statuses`/
/// `gh_assignees`).
pub const DUCKDB_TABLES: &[(&str, &str)] = table_templates!(
    "duckdb",
    "commits", "commit_parents", "file_changes", "refs", "blobs", "trees", "tree_entries",
    "repos", "conflicts", "sync_state",
    "gh_pull_requests", "gh_issues", "gh_events", "gh_workflow_runs", "gh_check_runs", "gh_jobs",
    "gh_steps", "gh_workflows", "gh_commit_statuses", "gh_assignees",
    "gh_comments", "gh_labeled", "gh_labels", "gh_pr_reviews", "gh_pr_commits",
    "gh_requested_reviewers", "gh_review_comments", "gh_users",
);

/// DuckDB-only helpers applied after the tables on every (re)build: DAG-walk macros + hex views.
pub const DUCKDB_EXTRAS: &str = include_str!("../migrations/duckdb/extras.sql");

/// The portable-sink tables (the streamed subset) — same machinery, per-dialect DDL.
pub const SQLITE_TABLES: &[(&str, &str)] = table_templates!(
    "sqlite",
    "commits", "commit_parents", "file_changes", "refs", "blobs", "trees", "tree_entries",
    "gh_pull_requests", "gh_issues", "gh_events", "gh_workflow_runs", "gh_check_runs",
    "gh_comments", "gh_labeled", "gh_labels", "gh_pr_reviews", "gh_pr_commits",
    "gh_requested_reviewers", "gh_review_comments", "gh_users",
);

pub const PG_TABLES: &[(&str, &str)] = table_templates!(
    "postgres",
    "commits", "commit_parents", "file_changes", "refs", "blobs", "trees", "tree_entries",
    "gh_pull_requests", "gh_issues", "gh_events", "gh_workflow_runs", "gh_check_runs",
    "gh_comments", "gh_labeled", "gh_labels", "gh_pr_reviews", "gh_pr_commits",
    "gh_requested_reviewers", "gh_review_comments", "gh_users",
);

/// The DDL template for `table`, with `__table__` replaced by `target`.
pub fn instantiate(templates: &[(&str, &str)], table: &str, target: &str) -> Option<String> {
    templates
        .iter()
        .find(|(n, _)| *n == table)
        .map(|(_, tmpl)| tmpl.replace("__table__", target))
}
