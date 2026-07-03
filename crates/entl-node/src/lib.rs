//! Node-API binding for the entl engine (napi-rs). The Rust sync engine runs
//! in-process inside Node/Bun, so the one DuckDB connection used for writes is the
//! same database the reads see — no cross-process file-lock fight (DESIGN §8).
//!
//! entl-core stays **synchronous** (a blocking engine); async is a per-binding
//! concern. Here each heavy method offloads onto libuv's threadpool via napi
//! `AsyncTask` and returns a `Promise`, so the JS event loop never blocks. Each
//! task runs on its own `try_clone()`d connection (shares the same database).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use entl_core::Db;
use napi::bindgen_prelude::{AsyncTask, Result};
use napi::threadsafe_function::{ErrorStrategy, ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi::{Env, Task};
use napi_derive::napi;

fn err(e: impl std::fmt::Display) -> napi::Error {
    napi::Error::from_reason(e.to_string())
}

/// Sanity probe — confirms the native addon loaded and links entl-core.
#[napi]
pub fn version() -> String {
    format!("entl-node {} (engine ready)", env!("CARGO_PKG_VERSION"))
}

pub struct DiffTask {
    repo_path: String,
    base: String,
    head: String,
    three_dot: bool,
}
impl Task for DiffTask {
    type Output = String;
    type JsValue = String;
    fn compute(&mut self) -> Result<Self::Output> {
        let diffs = entl_core::diff_commits(&self.repo_path, &self.base, &self.head, self.three_dot)
            .map_err(err)?;
        serde_json::to_string(&diffs).map_err(err)
    }
    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

/// Diff two commits locally → `Promise<string>` (JSON array of per-file changes +
/// full unified patches). `threeDot` shifts the base to the merge-base (GitHub's
/// `base...head` PR diff). Runs off the JS thread.
#[napi(ts_return_type = "Promise<string>")]
pub fn diff_commits(repo_path: String, base: String, head: String, three_dot: bool) -> AsyncTask<DiffTask> {
    AsyncTask::new(DiffTask { repo_path, base, head, three_dot })
}

pub struct FileAtTask {
    repo_path: String,
    commit: String,
    path: String,
}
impl Task for FileAtTask {
    type Output = Option<String>;
    type JsValue = Option<String>;
    fn compute(&mut self) -> Result<Self::Output> {
        entl_core::file_at(&self.repo_path, &self.commit, &self.path).map_err(err)
    }
    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

/// Full text of `path` at `commit` → `Promise<string | null>` (null if absent or
/// binary). Local; runs off the JS thread.
#[napi(ts_return_type = "Promise<string | null>")]
pub fn file_at(repo_path: String, commit: String, path: String) -> AsyncTask<FileAtTask> {
    AsyncTask::new(FileAtTask { repo_path, commit, path })
}

// ---- live git reads (operational helpers, fetch-first for freshness) ----

/// Lightweight `git fetch` of origin's branches (no PR refs / tags) so the
/// remote-tracking refs the reads below consult are current on every call.
fn git_fetch_branches(path: &str) {
    let _ = std::process::Command::new("git")
        .args(["-C", path, "fetch", "origin", "--prune", "--quiet"])
        .output();
}

pub struct BranchExistsTask {
    repo_path: String,
    name: String,
}
impl Task for BranchExistsTask {
    type Output = bool;
    type JsValue = bool;
    fn compute(&mut self) -> Result<bool> {
        git_fetch_branches(&self.repo_path);
        entl_core::branch_exists(&self.repo_path, &self.name).map_err(err)
    }
    fn resolve(&mut self, _env: Env, o: bool) -> Result<bool> {
        Ok(o)
    }
}
/// Does `name` (or `origin/<name>`) resolve to a commit? Fetches first → `Promise<boolean>`.
#[napi(ts_return_type = "Promise<boolean>")]
pub fn branch_exists(repo_path: String, name: String) -> AsyncTask<BranchExistsTask> {
    AsyncTask::new(BranchExistsTask { repo_path, name })
}

pub struct CurrentBranchTask {
    repo_path: String,
}
impl Task for CurrentBranchTask {
    type Output = String;
    type JsValue = String;
    fn compute(&mut self) -> Result<String> {
        entl_core::current_branch(&self.repo_path).map_err(err)
    }
    fn resolve(&mut self, _env: Env, o: String) -> Result<String> {
        Ok(o)
    }
}
/// The checked-out branch (HEAD's short name). Local read, no fetch → `Promise<string>`.
#[napi(ts_return_type = "Promise<string>")]
pub fn current_branch(repo_path: String) -> AsyncTask<CurrentBranchTask> {
    AsyncTask::new(CurrentBranchTask { repo_path })
}

pub struct CommitBodiesTask {
    repo_path: String,
    branch: String,
}
impl Task for CommitBodiesTask {
    type Output = String;
    type JsValue = String;
    fn compute(&mut self) -> Result<String> {
        git_fetch_branches(&self.repo_path);
        entl_core::commit_bodies(&self.repo_path, &self.branch).map_err(err)
    }
    fn resolve(&mut self, _env: Env, o: String) -> Result<String> {
        Ok(o)
    }
}
/// NUL-separated commit message bodies reachable from `branch` (∪ `origin/<branch>`).
/// Fetches first → `Promise<string>`.
#[napi(ts_return_type = "Promise<string>")]
pub fn commit_bodies(repo_path: String, branch: String) -> AsyncTask<CommitBodiesTask> {
    AsyncTask::new(CommitBodiesTask { repo_path, branch })
}

pub struct LsRemoteHeadsTask {
    repo_path: String,
    pattern: String,
}
impl Task for LsRemoteHeadsTask {
    type Output = Vec<String>;
    type JsValue = Vec<String>;
    fn compute(&mut self) -> Result<Vec<String>> {
        git_fetch_branches(&self.repo_path);
        entl_core::ls_remote_heads(&self.repo_path, &self.pattern).map_err(err)
    }
    fn resolve(&mut self, _env: Env, o: Vec<String>) -> Result<Vec<String>> {
        Ok(o)
    }
}
/// Remote branch names matching `pattern` (trailing-`*` glob). Fetches first → `Promise<string[]>`.
#[napi(ts_return_type = "Promise<string[]>")]
pub fn ls_remote_heads(repo_path: String, pattern: String) -> AsyncTask<LsRemoteHeadsTask> {
    AsyncTask::new(LsRemoteHeadsTask { repo_path, pattern })
}

#[napi(object)]
pub struct GitStats {
    pub new_commits: i64,
    pub file_changes: i64,
    pub refs: i64,
}
impl From<entl_core::GitIngest> for GitStats {
    fn from(r: entl_core::GitIngest) -> Self {
        Self {
            new_commits: r.new_commits as i64,
            file_changes: r.file_changes as i64,
            refs: r.refs as i64,
        }
    }
}

#[napi(object)]
pub struct GithubStats {
    pub events: i64,
    pub pull_requests: i64,
    pub reviews: i64,
    pub review_comments: i64,
    pub issues: i64,
    pub comments: i64,
    pub workflow_runs: i64,
    pub check_runs: i64,
    pub users: i64,
}
impl From<entl_core::GithubIngest> for GithubStats {
    fn from(r: entl_core::GithubIngest) -> Self {
        Self {
            events: r.events as i64,
            pull_requests: r.pull_requests as i64,
            reviews: r.reviews as i64,
            review_comments: r.review_comments as i64,
            issues: r.issues as i64,
            comments: r.comments as i64,
            workflow_runs: r.workflow_runs as i64,
            check_runs: r.check_runs as i64,
            users: r.users as i64,
        }
    }
}

/// A built-in sink target. Regenerates as a TS `enum SinkTarget` — callers write
/// `SinkTarget.Sqlite`. Maps onto the source-of-truth [`entl_core::SinkTarget`].
#[napi]
pub enum SinkTarget {
    Sqlite,
    Jsonl,
    Postgres,
}
impl From<SinkTarget> for entl_core::SinkTarget {
    fn from(t: SinkTarget) -> Self {
        match t {
            SinkTarget::Sqlite => entl_core::SinkTarget::Sqlite,
            SinkTarget::Jsonl => entl_core::SinkTarget::Jsonl,
            SinkTarget::Postgres => entl_core::SinkTarget::Postgres,
        }
    }
}

/// Rename a table at the sink (`from` → `to`).
#[napi(object)]
pub struct TableRename {
    pub from: String,
    pub to: String,
}

/// Options for `Entl.sink()`.
#[napi(object)]
pub struct SinkOptions {
    /// Which target to write.
    pub target: SinkTarget,
    /// The SQLite file, the JSONL output directory, or the Postgres connection URL.
    pub path: Option<String>,
    /// Also pull GitHub (default true). Requires a token (`gh auth token` / GH_TOKEN).
    pub github: Option<bool>,
    /// Only write these tables (default: all).
    pub tables: Option<Vec<String>>,
    /// Skip these tables.
    pub exclude: Option<Vec<String>>,
    /// Rename tables at the sink.
    pub rename: Option<Vec<TableRename>>,
    /// Target schema (Postgres only; default "entl").
    pub schema: Option<String>,
    /// Also store the object graph (trees/blobs + raw content) so the store can rebuild the repo.
    pub objects: Option<bool>,
}

impl SinkOptions {
    fn select(&self) -> entl_core::SinkSelect {
        entl_core::SinkSelect {
            tables: self.tables.clone(),
            exclude: self.exclude.clone().unwrap_or_default(),
            rename: self
                .rename
                .as_ref()
                .map(|rs| rs.iter().map(|r| (r.from.clone(), r.to.clone())).collect())
                .unwrap_or_default(),
            schema: self.schema.clone(),
        }
    }
}

/// What a `sink()` cycle produced: the git + forge counts, and rows applied to the target.
#[napi(object)]
pub struct SinkStats {
    pub new_commits: i64,
    pub file_changes: i64,
    pub refs: i64,
    pub pull_requests: i64,
    pub issues: i64,
    pub events: i64,
    pub workflow_runs: i64,
    pub check_runs: i64,
    /// Total rows the sink applied across all change batches.
    pub rows: i64,
}
impl From<entl_core::SinkOutcome> for SinkStats {
    fn from(o: entl_core::SinkOutcome) -> Self {
        let gh = o.github.as_ref();
        Self {
            new_commits: o.git.new_commits as i64,
            file_changes: o.git.file_changes as i64,
            refs: o.git.refs as i64,
            pull_requests: gh.map(|g| g.pull_requests).unwrap_or(0) as i64,
            issues: gh.map(|g| g.issues).unwrap_or(0) as i64,
            events: gh.map(|g| g.events).unwrap_or(0) as i64,
            workflow_runs: gh.map(|g| g.workflow_runs).unwrap_or(0) as i64,
            check_runs: gh.map(|g| g.check_runs).unwrap_or(0) as i64,
            rows: o.rows as i64,
        }
    }
}

// ---- AsyncTasks: blocking work, run on libuv's threadpool ----

pub struct SinkTask {
    db: Db,
    repo_path: String,
    target: entl_core::SinkTarget,
    path: Option<String>,
    github: bool,
    objects: bool,
    select: entl_core::SinkSelect,
}
impl Task for SinkTask {
    type Output = entl_core::SinkOutcome;
    type JsValue = SinkStats;
    fn compute(&mut self) -> Result<Self::Output> {
        let sink = entl_core::build_sink(self.target, self.path.as_deref(), self.select.clone())
            .map_err(err)?;
        entl_core::pull_into(
            &self.db,
            &self.repo_path,
            sink,
            entl_core::PullOpts { github: self.github, objects: self.objects },
        )
        .map_err(err)
    }
    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output.into())
    }
}

/// Options for `Entl.extract()`.
#[napi(object)]
pub struct ExtractOptions {
    /// Source store: `duckdb` | `sqlite` | `jsonl` | `postgres`.
    pub source: String,
    /// The store location (file / directory / Postgres URL).
    pub path: String,
    /// Which tables to read (default: the git tables).
    pub tables: Option<Vec<String>>,
    /// Postgres schema (default "entl").
    pub schema: Option<String>,
}

pub struct ExtractTask {
    source: String,
    path: String,
    tables: Vec<String>,
    schema: Option<String>,
}
impl Task for ExtractTask {
    type Output = String;
    type JsValue = String;
    fn compute(&mut self) -> Result<String> {
        entl_core::extract_json(&self.source, &self.path, &self.tables, self.schema.as_deref())
            .map_err(err)
    }
    fn resolve(&mut self, _env: Env, output: String) -> Result<String> {
        Ok(output)
    }
}

/// Options for `Entl.rebuild()`.
#[napi(object)]
pub struct RebuildOptions {
    /// Source store: `duckdb` | `sqlite` | `jsonl` | `postgres`.
    pub from: String,
    /// The store location (file / directory / Postgres URL).
    pub dest: String,
    /// Output directory for the reconstructed repo.
    pub out: String,
    /// Postgres schema (default "entl").
    pub schema: Option<String>,
}

pub struct RebuildTask {
    from: String,
    dest: String,
    out: String,
    schema: Option<String>,
}
impl Task for RebuildTask {
    type Output = usize;
    type JsValue = i64;
    fn compute(&mut self) -> Result<usize> {
        entl_core::rebuild_store(&self.from, &self.dest, self.schema.as_deref(), std::path::Path::new(&self.out))
            .map(|oids| oids.len())
            .map_err(err)
    }
    fn resolve(&mut self, _env: Env, output: usize) -> Result<i64> {
        Ok(output as i64)
    }
}

pub struct LoadGitTask {
    db: Db,
    repo_path: String,
}
impl Task for LoadGitTask {
    type Output = entl_core::GitIngest;
    type JsValue = GitStats;
    fn compute(&mut self) -> Result<Self::Output> {
        let counter = AtomicU64::new(0);
        entl_core::ingest_git(&self.db, &self.repo_path, &counter).map_err(err)
    }
    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output.into())
    }
}

pub struct LoadGithubTask {
    db: Db,
    repo_path: String,
}
impl Task for LoadGithubTask {
    type Output = entl_core::GithubIngest;
    type JsValue = GithubStats;
    fn compute(&mut self) -> Result<Self::Output> {
        entl_core::ingest_github(&self.db, &self.repo_path).map_err(err)
    }
    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output.into())
    }
}

pub struct QueryTask {
    db: Db,
    sql: String,
}
impl Task for QueryTask {
    type Output = String;
    type JsValue = String;
    fn compute(&mut self) -> Result<Self::Output> {
        // DuckDB serializes the result; all types come back as JSON.
        let wrapped = format!(
            "SELECT CAST(COALESCE(json_group_array(to_json(__t)), '[]') AS VARCHAR) \
             FROM ({}) AS __t",
            self.sql
        );
        self.db
            .conn
            .query_row(&wrapped, [], |row| row.get(0))
            .map_err(err)
    }
    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

/// The payload passed to the `watch` callback after each sync cycle.
#[napi(object)]
pub struct SyncStats {
    pub new_commits: i64,
    pub file_changes: i64,
    pub events: i64,
    pub pull_requests: i64,
    pub issues: i64,
    pub workflow_runs: i64,
}

/// A live change stream from one pull — the design's "stream plane". `next()` resolves to the
/// next change batch (`{table, op, rows}` JSON) or `null` when the pull is done. Dress it as an
/// async iterator in JS: `for await (const b of iterate(entl.changes(repo))) { … }`.
#[napi]
pub struct Changes {
    stream: Arc<entl_core::ChangeStream>,
}

pub struct NextChangeTask {
    stream: Arc<entl_core::ChangeStream>,
}
impl Task for NextChangeTask {
    type Output = Option<String>;
    type JsValue = Option<String>;
    fn compute(&mut self) -> Result<Option<String>> {
        loop {
            match self.stream.poll(Duration::from_millis(500)) {
                entl_core::Poll::Batch(b) => {
                    let out = serde_json::json!({
                        "table": b.table,
                        "op": b.op.as_str(),
                        "rows": entl_core::sink::batch_to_json(&b),
                    });
                    return serde_json::to_string(&out).map(Some).map_err(err);
                }
                entl_core::Poll::Idle => continue,
                entl_core::Poll::Closed => return Ok(None),
            }
        }
    }
    fn resolve(&mut self, _env: Env, o: Option<String>) -> Result<Option<String>> {
        Ok(o)
    }
}

#[napi]
impl Changes {
    /// The next change batch as JSON (`{table, op, rows}`), or `null` when the stream ends.
    #[napi(ts_return_type = "Promise<string | null>")]
    pub fn next(&self) -> AsyncTask<NextChangeTask> {
        AsyncTask::new(NextChangeTask { stream: self.stream.clone() })
    }
}

/// A driver-sink statement plan — the "mirror into a database entl-core doesn't link" surface
/// (notes/design/multidb.md). All the DDL / type-mapping / upsert logic lives in Rust core
/// (`DriverSink`); this streams the resulting `{sql, params}` statements so the JS side only
/// executes them against its own client (e.g. PGlite: `await pg.query(sql, params)`). `next()`
/// resolves to the next statement or `null` when the plan is complete.
#[napi]
pub struct DriverPlan {
    stream: Arc<entl_core::StatementStream>,
}

pub struct NextStmtTask {
    stream: Arc<entl_core::StatementStream>,
}
impl Task for NextStmtTask {
    type Output = Option<String>;
    type JsValue = Option<String>;
    fn compute(&mut self) -> Result<Option<String>> {
        loop {
            match self.stream.poll(Duration::from_millis(500)) {
                entl_core::StmtPoll::Statement(s) => {
                    let out = serde_json::json!({ "sql": s.sql, "params": s.params, "table": s.table });
                    return serde_json::to_string(&out).map(Some).map_err(err);
                }
                entl_core::StmtPoll::Idle => continue,
                entl_core::StmtPoll::Closed => return Ok(None),
            }
        }
    }
    fn resolve(&mut self, _env: Env, o: Option<String>) -> Result<Option<String>> {
        Ok(o)
    }
}

#[napi]
impl DriverPlan {
    /// The next statement as JSON (`{sql, params}`), or `null` when the plan is complete.
    #[napi(ts_return_type = "Promise<string | null>")]
    pub fn next(&self) -> AsyncTask<NextStmtTask> {
        AsyncTask::new(NextStmtTask { stream: self.stream.clone() })
    }
}

/// Options for `Entl.driverPlan()`.
#[napi(object)]
pub struct DriverPlanOptions {
    /// Only mirror these tables (default: all Postgres-eligible tables).
    pub tables: Option<Vec<String>>,
    /// Skip these tables.
    pub exclude: Option<Vec<String>>,
    /// Rename tables at the target.
    pub rename: Option<Vec<TableRename>>,
    /// Target schema (default "entl").
    pub schema: Option<String>,
}

/// Options for `Entl.changes()`.
#[napi(object)]
pub struct ChangesOptions {
    /// Also stream GitHub changes (needs a token). Default false.
    pub github: Option<bool>,
    /// Also stream the object graph (trees/blobs). Default false.
    pub objects: Option<bool>,
}

/// Handle to a running `watch` loop. Call `stop()` to end it.
#[napi]
pub struct WatchHandle {
    stop: Arc<AtomicBool>,
}
#[napi]
impl WatchHandle {
    /// Stop the watch loop (takes effect after the current cycle).
    #[napi]
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// `git fetch` (fetch-only, never merge) so the local repo tracks the remote.
/// Fetches branches/tags *and* `refs/pull/*/head` (not in the default refspec), so
/// every PR's commits are local — PR diffs then work even for merged/deleted-branch
/// PRs. The PR fetch is best-effort (no-op on non-GitHub remotes).
fn git_fetch(path: &str) {
    let _ = std::process::Command::new("git")
        .args(["-C", path, "fetch", "--all", "--prune", "--tags", "--quiet"])
        .output();
    let _ = std::process::Command::new("git")
        .args([
            "-C", path, "fetch", "origin",
            "+refs/pull/*/head:refs/remotes/origin/pull/*", "--quiet",
        ])
        .output();
}

/// An open entl database. Heavy methods return Promises and run off the JS thread.
#[napi]
pub struct Entl {
    db: Db,
}

#[napi]
impl Entl {
    /// Open (or create) the .duckdb at `dbPath` and apply migrations.
    #[napi(constructor)]
    pub fn new(db_path: String) -> Result<Self> {
        let db = Db::open(&db_path).map_err(err)?;
        db.migrate().map_err(err)?;
        Ok(Self { db })
    }

    /// Load git history from `repoPath` (one-way, incremental). → `Promise<GitStats>`.
    #[napi(ts_return_type = "Promise<GitStats>")]
    pub fn load_git(&self, repo_path: String) -> Result<AsyncTask<LoadGitTask>> {
        let db = Db::from_conn(self.db.conn.try_clone().map_err(err)?);
        Ok(AsyncTask::new(LoadGitTask { db, repo_path }))
    }

    /// Load GitHub data (events/PRs/issues/Actions). → `Promise<GithubStats>`.
    #[napi(ts_return_type = "Promise<GithubStats>")]
    pub fn load_github(&self, repo_path: String) -> Result<AsyncTask<LoadGithubTask>> {
        let db = Db::from_conn(self.db.conn.try_clone().map_err(err)?);
        Ok(AsyncTask::new(LoadGithubTask { db, repo_path }))
    }

    /// Run a SQL query → `Promise<string>` (JSON array of rows). Runs off-thread,
    /// so a big query (Linux-kernel scale) never blocks the event loop.
    #[napi(ts_return_type = "Promise<string>")]
    pub fn query(&self, sql: String) -> Result<AsyncTask<QueryTask>> {
        let db = Db::from_conn(self.db.conn.try_clone().map_err(err)?);
        Ok(AsyncTask::new(QueryTask { db, sql }))
    }

    /// Pull `repoPath` and sync it into `options.target` (SQLite file / JSONL dir), in one
    /// call. Writes both this handle's DuckDB (the default store) and the chosen target. Runs
    /// off the JS thread → `Promise<SinkStats>`.
    #[napi(ts_return_type = "Promise<SinkStats>")]
    pub fn sink(&self, repo_path: String, options: SinkOptions) -> Result<AsyncTask<SinkTask>> {
        let db = Db::from_conn(self.db.conn.try_clone().map_err(err)?);
        let select = options.select();
        Ok(AsyncTask::new(SinkTask {
            db,
            repo_path,
            target: options.target.into(),
            path: options.path,
            github: options.github.unwrap_or(true),
            objects: options.objects.unwrap_or(false),
            select,
        }))
    }

    /// Read a store back into canonical rows → `Promise<string>` (JSON: table → rows, normalized
    /// like the sinks write — oids hex, timestamps RFC3339). The reverse of `sink`. Off the JS
    /// thread.
    #[napi(ts_return_type = "Promise<string>")]
    pub fn extract(&self, options: ExtractOptions) -> AsyncTask<ExtractTask> {
        let tables = options.tables.unwrap_or_else(|| {
            entl_core::extract::GIT_TABLES.iter().map(|s| s.to_string()).collect()
        });
        AsyncTask::new(ExtractTask {
            source: options.source,
            path: options.path,
            tables,
            schema: options.schema,
        })
    }

    /// Stream the change batches from one pull of `repoPath` (the "stream plane"). Returns a
    /// `Changes` handle; call `.next()` for each `{table, op, rows}` batch until it yields `null`.
    /// A background thread runs the pull and feeds the stream (backpressured).
    #[napi]
    pub fn changes(&self, repo_path: String, options: Option<ChangesOptions>) -> Result<Changes> {
        let (sink, stream) = entl_core::change_channel(256);
        let db = Db::from_conn(self.db.conn.try_clone().map_err(err)?);
        let github = options.as_ref().and_then(|o| o.github).unwrap_or(false);
        let objects = options.as_ref().and_then(|o| o.objects).unwrap_or(false);
        std::thread::spawn(move || {
            let counter = AtomicU64::new(0);
            let _ = entl_core::ingest_git_streamed(&db, &repo_path, &counter, Some(&sink));
            if objects {
                let _ = entl_core::ingest_git_objects(&db, &repo_path, Some(&sink));
            }
            if github {
                let _ = entl_core::ingest_github_streamed(&db, &repo_path, Some(&sink));
            }
            drop(sink); // closes the stream → next() resolves null
        });
        Ok(Changes { stream: Arc::new(stream) })
    }

    /// Backfill this handle's DuckDB store into a **driver** target — a database entl-core doesn't
    /// link (PGlite, or any client you hold). Returns a `DriverPlan`; call `.next()` for each
    /// `{sql, params}` statement and run it against your client until it yields `null`. The DDL,
    /// type mapping and upserts are all generated in Rust (`DriverSink`) — the JS side only
    /// executes. A background thread reads the store and feeds the plan (backpressured).
    #[napi]
    pub fn driver_plan(&self, options: Option<DriverPlanOptions>) -> Result<DriverPlan> {
        let opts = options.unwrap_or(DriverPlanOptions {
            tables: None,
            exclude: None,
            rename: None,
            schema: None,
        });
        let select = entl_core::SinkSelect {
            tables: opts.tables.clone(),
            exclude: opts.exclude.clone().unwrap_or_default(),
            rename: opts
                .rename
                .as_ref()
                .map(|rs| rs.iter().map(|r| (r.from.clone(), r.to.clone())).collect())
                .unwrap_or_default(),
            schema: opts.schema.clone(),
        };
        let (tx, stream) = entl_core::statement_channel(256);
        let db = Db::from_conn(self.db.conn.try_clone().map_err(err)?);
        std::thread::spawn(move || {
            let mut sink = entl_core::DriverSink::new(
                move |s| tx.send(s).map_err(|_| anyhow::anyhow!("driver plan consumer dropped")),
                entl_core::Dialect::Postgres,
                select,
            );
            let tables = entl_core::driver_tables();
            let _ = entl_core::backfill(&db.conn, &mut sink, &tables);
            // `tx` drops here → the plan stream closes → next() resolves null.
        });
        Ok(DriverPlan { stream: Arc::new(stream) })
    }

    /// Reconstruct a git repo from a store into `options.out` → `Promise<number>` (commits
    /// rebuilt). Needs the store to have been sunk with `objects: true`. Off the JS thread.
    #[napi(ts_return_type = "Promise<number>")]
    pub fn rebuild(&self, options: RebuildOptions) -> AsyncTask<RebuildTask> {
        AsyncTask::new(RebuildTask {
            from: options.from,
            dest: options.dest,
            out: options.out,
            schema: options.schema,
        })
    }

    /// Watch `repoPath`: on a background thread, every `intervalSecs`, `git fetch`
    /// + load git + load GitHub into the DB, then call `onSync(stats)` so the JS
    /// side can mirror into PGlite (whose `live` then drives realtime). Returns a
    /// handle; call `.stop()` to end it. The first cycle runs immediately.
    #[napi]
    pub fn watch(
        &self,
        repo_path: String,
        interval_secs: u32,
        #[napi(ts_arg_type = "(stats: SyncStats) => void")] on_sync: ThreadsafeFunction<
            SyncStats,
            ErrorStrategy::Fatal,
        >,
    ) -> Result<WatchHandle> {
        let conn = self.db.conn.try_clone().map_err(err)?;
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();

        std::thread::spawn(move || {
            let db = Db::from_conn(conn);
            loop {
                git_fetch(&repo_path);
                let counter = AtomicU64::new(0);
                let g = entl_core::ingest_git(&db, &repo_path, &counter).unwrap_or_default();
                let gh = match entl_core::ingest_github(&db, &repo_path) {
                    Ok(x) => x,
                    Err(e) => {
                        eprintln!("entl watch: github sync error: {e:#}");
                        Default::default()
                    }
                };
                on_sync.call(
                    SyncStats {
                        new_commits: g.new_commits as i64,
                        file_changes: g.file_changes as i64,
                        events: gh.events as i64,
                        pull_requests: gh.pull_requests as i64,
                        issues: gh.issues as i64,
                        workflow_runs: gh.workflow_runs as i64,
                    },
                    ThreadsafeFunctionCallMode::NonBlocking,
                );

                // Sleep the interval in small slices so stop() is responsive.
                for _ in 0..(interval_secs.max(1) * 5) {
                    if stop_thread.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        });

        Ok(WatchHandle { stop })
    }
}
