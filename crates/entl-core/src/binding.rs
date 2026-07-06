//! The shared binding core — the `GitCore`/`EntlCore` trait impls every
//! language binding expands locally via [`binding_core_impls!`].
//!
//! The traits and DTO types are GENERATED per binding crate (fluessig bindgen
//! emits them into each crate's `generated` module), so the impl bodies can't
//! live here as ordinary items — but they are logically ONE implementation.
//! This macro is that implementation, written once: bare type names resolve at
//! the expansion site (the binding's `generated` module), `$crate::` resolves
//! to entl-core. The single per-binding delta — Python renames `TableRename`'s
//! `from` field to `from_` (a Python keyword) — is the macro's one parameter.
//!
//! Requirements at the call site: `anyhow` and `serde_json` as dependencies,
//! and a `crate::generated` module from fluessig bindgen.

/// Expand the `GitImpl`/`EntlImpl` trait implementations over entl-core.
///
/// ```ignore
/// // in a binding crate's core_impl.rs:
/// entl_core::binding_core_impls!(rename_from = from);   // node / ruby
/// entl_core::binding_core_impls!(rename_from = from_);  // python
/// ```
// This macro deliberately uses `crate::generated` (not `$crate::generated`) so it
// resolves at the EXPANSION site — each binding's own generated module. `$crate`
// would point at entl-core, which has no `generated` module. Hence two guards:
//  - `#[rustfmt::skip]`: rustfmt rewrites `crate::` → `$crate::` in macro bodies.
//  - `#[allow(clippy::crate_in_macro_def)]`: clippy flags `crate::` in a macro def
//    and suggests `$crate::` — correct in general, wrong here (we want the caller).
#[macro_export]
#[rustfmt::skip]
#[allow(clippy::crate_in_macro_def)]
macro_rules! binding_core_impls {
    (rename_from = $rename_from:ident) => {
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;
    use std::time::Duration;

    use anyhow::Result;

    use crate::generated::*;

    /// Lightweight `git fetch` of origin's branches (no PR refs / tags) so the
    /// remote-tracking refs the live reads consult are current on every call.
    pub fn git_fetch_branches(path: &str) {
        let _ = std::process::Command::new("git")
            .args(["-C", path, "fetch", "origin", "--prune", "--quiet"])
            .output();
    }

    /// The stateless git helpers.
    pub struct GitImpl;

    impl GitCore for GitImpl {
        fn diff_commits(repo_path: String, base: String, head: String, three_dot: bool) -> Result<Vec<FileDiff>> {
            let diffs = $crate::diff_commits(&repo_path, &base, &head, three_dot)?;
            Ok(diffs
                .into_iter()
                .map(|d| FileDiff {
                    path: d.path,
                    old_path: d.old_path,
                    status: d.status.to_string(),
                    additions: d.additions,
                    deletions: d.deletions,
                    patch: d.patch,
                })
                .collect())
        }

        fn file_at(repo_path: String, commit: String, path: String) -> Result<Option<String>> {
            $crate::file_at(&repo_path, &commit, &path)
        }

        fn branch_exists(repo_path: String, name: String) -> Result<bool> {
            git_fetch_branches(&repo_path);
            $crate::branch_exists(&repo_path, &name)
        }

        fn current_branch(repo_path: String) -> Result<String> {
            $crate::current_branch(&repo_path)
        }

        fn commit_bodies(repo_path: String, branch: String) -> Result<String> {
            git_fetch_branches(&repo_path);
            $crate::commit_bodies(&repo_path, &branch)
        }

        fn ls_remote_heads(repo_path: String, pattern: String) -> Result<Vec<String>> {
            git_fetch_branches(&repo_path);
            $crate::ls_remote_heads(&repo_path, &pattern)
        }
    }

    /// An open entl database. `duckdb::Connection` is Send but not Sync, so the
    /// shared handle keeps it behind a Mutex and every heavy call `try_clone()`s a
    /// worker connection (same database) under a brief lock — the generated
    /// AsyncTasks then never contend on one handle.
    pub struct EntlImpl {
        pub db: std::sync::Mutex<$crate::Db>,
    }

    impl EntlImpl {
        pub fn worker(&self) -> Result<$crate::Db> {
            let db = self.db.lock().expect("entl handle poisoned");
            Ok($crate::Db::from_conn(db.conn.try_clone()?))
        }
    }

    fn select_of(
        tables: Option<Vec<String>>,
        exclude: Option<Vec<String>>,
        rename: Option<Vec<TableRename>>,
        schema: Option<String>,
    ) -> $crate::SinkSelect {
        $crate::SinkSelect {
            tables,
            exclude: exclude.unwrap_or_default(),
            rename: rename
                .map(|rs| rs.into_iter().map(|r| (r.$rename_from, r.to)).collect())
                .unwrap_or_default(),
            schema,
        }
    }

    /// Adapt an entl-core change stream to the generated poll contract. The rows
    /// stay columnar — the DTO holds the RecordBatch and each language surfaces
    /// it lazily (IPC getter / PyCapsules), so nothing is encoded here.
    struct ChangesStream($crate::ChangeStream);
    impl PollStream<ChangeBatch> for ChangesStream {
        fn poll(&self, timeout: Duration) -> Poll<ChangeBatch> {
            match self.0.poll(timeout) {
                $crate::Poll::Batch(b) => Poll::Item(ChangeBatch {
                    table: b.table,
                    op: b.op.as_str().to_string(),
                    batch: b.batch,
                }),
                $crate::Poll::Idle => Poll::Idle,
                $crate::Poll::Closed => Poll::Closed,
            }
        }
    }

    /// Adapt the driver sink's statement stream to the generated poll contract.
    struct PlanStream($crate::StatementStream);
    impl PollStream<Statement> for PlanStream {
        fn poll(&self, timeout: Duration) -> Poll<Statement> {
            match self.0.poll(timeout) {
                $crate::StmtPoll::Statement(s) => Poll::Item(Statement {
                    sql: s.sql,
                    params: serde_json::to_string(&s.params).unwrap_or_else(|_| "[]".into()),
                    table: s.table,
                }),
                $crate::StmtPoll::Idle => Poll::Idle,
                $crate::StmtPoll::Closed => Poll::Closed,
            }
        }
    }

    impl EntlCore for EntlImpl {
        fn open(db_path: String) -> Result<Self> {
            let db = $crate::Db::open(&db_path)?;
            db.migrate()?;
            Ok(Self { db: std::sync::Mutex::new(db) })
        }

        fn load_git(&self, repo_path: String) -> Result<GitStats> {
            let db = self.worker()?;
            let counter = AtomicU64::new(0);
            let r = $crate::ingest_git(&db, &repo_path, &counter)?;
            Ok(GitStats {
                new_commits: r.new_commits as i64,
                file_changes: r.file_changes as i64,
                refs: r.refs as i64,
            })
        }

        fn load_github(&self, repo_path: String) -> Result<GithubStats> {
            let db = self.worker()?;
            let r = $crate::ingest_github(&db, &repo_path)?;
            Ok(GithubStats {
                events: r.events as i64,
                pull_requests: r.pull_requests as i64,
                reviews: r.reviews as i64,
                review_comments: r.review_comments as i64,
                issues: r.issues as i64,
                comments: r.comments as i64,
                workflow_runs: r.workflow_runs as i64,
                check_runs: r.check_runs as i64,
                users: r.users as i64,
            })
        }

        fn query(&self, sql: String) -> Result<String> {
            let db = self.worker()?;
            let wrapped = format!(
                "SELECT CAST(COALESCE(json_group_array(to_json(__t)), '[]') AS VARCHAR) FROM ({sql}) AS __t"
            );
            Ok(db.conn.query_row(&wrapped, [], |r| r.get(0))?)
        }

        fn query_arrow(&self, sql: String) -> Result<Bytes> {
            let db = self.worker()?;
            Ok(db.query_arrow_ipc(&sql)?.into())
        }

        fn sink(&self, repo_path: String, options: SinkOptions) -> Result<SinkStats> {
            let db = self.worker()?;
            let target = match options.target {
                SinkTarget::Sqlite => $crate::SinkTarget::Sqlite,
                SinkTarget::Jsonl => $crate::SinkTarget::Jsonl,
                SinkTarget::Postgres => $crate::SinkTarget::Postgres,
            };
            let select = select_of(options.tables, options.exclude, options.rename, options.schema);
            let sink = $crate::build_sink(target, options.path.as_deref(), select)?;
            let out = $crate::pull_into(
                &db,
                &repo_path,
                sink,
                $crate::PullOpts {
                    github: options.github.unwrap_or(true),
                    objects: options.objects.unwrap_or(false),
                },
            )?;
            let gh = out.github.as_ref();
            Ok(SinkStats {
                new_commits: out.git.new_commits as i64,
                file_changes: out.git.file_changes as i64,
                refs: out.git.refs as i64,
                pull_requests: gh.map(|g| g.pull_requests).unwrap_or(0) as i64,
                issues: gh.map(|g| g.issues).unwrap_or(0) as i64,
                events: gh.map(|g| g.events).unwrap_or(0) as i64,
                workflow_runs: gh.map(|g| g.workflow_runs).unwrap_or(0) as i64,
                check_runs: gh.map(|g| g.check_runs).unwrap_or(0) as i64,
                rows: out.rows as i64,
            })
        }

        fn extract(&self, options: ExtractOptions) -> Result<String> {
            let tables = options.tables.unwrap_or_else(|| {
                $crate::extract::GIT_TABLES.iter().map(|s| s.to_string()).collect()
            });
            $crate::extract_json(&options.source, &options.path, &tables, options.schema.as_deref())
                .map_err(Into::into)
        }

        fn changes(
            &self,
            repo_path: String,
            options: Option<ChangesOptions>,
        ) -> Result<Box<dyn PollStream<ChangeBatch>>> {
            let (sink, stream) = $crate::change_channel(256);
            let db = self.worker()?;
            let github = options.as_ref().and_then(|o| o.github).unwrap_or(false);
            let objects = options.as_ref().and_then(|o| o.objects).unwrap_or(false);
            std::thread::spawn(move || {
                let counter = AtomicU64::new(0);
                let _ = $crate::ingest_git_streamed(&db, &repo_path, &counter, Some(&sink));
                if objects {
                    let _ = $crate::ingest_git_objects(&db, &repo_path, Some(&sink));
                }
                if github {
                    let _ = $crate::ingest_github_streamed(&db, &repo_path, Some(&sink));
                }
                drop(sink); // closes the stream → next() resolves null
            });
            Ok(Box::new(ChangesStream(stream)))
        }

        fn driver_plan(&self, options: Option<DriverPlanOptions>) -> Result<Box<dyn PollStream<Statement>>> {
            let o = options.unwrap_or(DriverPlanOptions {
                tables: None,
                exclude: None,
                rename: None,
                schema: None,
            });
            let select = select_of(o.tables, o.exclude, o.rename, o.schema);
            let (tx, stream) = $crate::statement_channel(256);
            let db = self.worker()?;
            std::thread::spawn(move || {
                let mut sink = $crate::DriverSink::new(
                    move |s| tx.send(s).map_err(|_| anyhow::anyhow!("driver plan consumer dropped")),
                    $crate::Dialect::Postgres,
                    select,
                );
                let tables = $crate::driver_tables();
                let _ = $crate::backfill(&db.conn, &mut sink, &tables);
            });
            Ok(Box::new(PlanStream(stream)))
        }

        fn rebuild(&self, options: RebuildOptions) -> Result<i64> {
            let refs = $crate::rebuild_store(
                &options.source,
                &options.dest,
                options.schema.as_deref(),
                std::path::Path::new(&options.out),
            )?;
            Ok(refs.len() as i64)
        }
    }

    /// Shared handle type for the @manual ops in lib.rs.
    pub type SharedCore = Arc<EntlImpl>;
    };
}
