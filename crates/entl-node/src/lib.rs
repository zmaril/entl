//! Node-API binding for the entl engine (napi-rs). The Rust sync engine runs
//! in-process inside Node/Bun, so the one DuckDB connection used for writes is the
//! same database the reads see — no cross-process file-lock fight (DESIGN §8).
//!
//! **The napi surface is GENERATED** (`generated.rs`, from the fluessig catalog's
//! op layer — classes, AsyncTasks→Promises, poll-stream dressing); the engine
//! wiring is hand-written once in `core_impl.rs` (the `GitCore`/`EntlCore` trait
//! impls). This file holds only what stays bespoke: `version()` and the
//! `@manual` op (`watch`'s ThreadsafeFunction callback — host-callback
//! re-entry the shape templates deliberately exclude).

mod core_impl;
mod generated;

pub use generated::*;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use entl_core::Db;
use napi::bindgen_prelude::Result;
use napi::threadsafe_function::{ErrorStrategy, ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi_derive::napi;

fn err(e: impl std::fmt::Display) -> napi::Error {
    napi::Error::from_reason(e.to_string())
}

/// Sanity probe — confirms the native addon loaded and links entl-core.
#[napi]
pub fn version() -> String {
    format!("entl-node {} (engine ready)", env!("CARGO_PKG_VERSION"))
}

// ---- @manual: watch (ThreadsafeFunction callback — host re-entry) ----

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

#[napi]
impl Entl {
    /// Watch `repoPath`: on a background thread, every `intervalSecs`, `git fetch`
    /// + load git + load GitHub into the DB, then call `onSync(stats)` so the JS
    /// side can mirror (e.g. into PGlite, whose `live` drives realtime). Returns a
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
        let conn = self.core.worker().map_err(err)?.conn;
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
