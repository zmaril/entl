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

// ---- AsyncTasks: blocking work, run on libuv's threadpool ----

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
