//! Hand-written, embedded SQL migrations applied by a tiny custom runner.
//! No ORM — the canonical schema is plain DuckDB DDL (notes/design/engine.md, §9.2).
//! Migrations are append-only and applied in order; each is tracked in
//! `_entl_migrations` so re-opening a DB is idempotent.

/// (name, sql) in apply order. `--> statement-breakpoint` markers are plain
/// `--` SQL comments and harmless to DuckDB's multi-statement execution.
pub const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_init", include_str!("../migrations/0001_init.sql")),
    ("0002_macros", include_str!("../migrations/0002_macros.sql")),
    ("0003_conflicts", include_str!("../migrations/0003_conflicts.sql")),
    ("0004_hex_views", include_str!("../migrations/0004_hex_views.sql")),
    ("0005_events", include_str!("../migrations/0005_events.sql")),
];
