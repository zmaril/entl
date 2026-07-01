//! entl CLI (notes/design/cli.md). The primary product: point it at a repo, get a
//! queryable .duckdb file. v0 (Rust): init / query / tables. Sync next.

// The ingest pipeline allocates row bundles on compute threads and frees them on
// the writer thread; mimalloc handles that cross-thread alloc/free far better
// than the system allocator (measured ~1.4x on the compute stage).
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::{Parser, Subcommand};
use entl_core::Db;
use indicatif::{ProgressBar, ProgressStyle};
use notify::{RecursiveMode, Watcher};

#[derive(Parser)]
#[command(name = "entl", version, about = "git + GitHub data in DuckDB")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create/open the DB and apply migrations.
    Init {
        #[arg(default_value = "entl.duckdb")]
        db: String,
    },
    /// Load git history + GitHub data into the DB (one-way, incremental).
    Load {
        #[arg(default_value = ".")]
        path: String,
        #[arg(long, default_value = "entl.duckdb")]
        db: String,
        /// Only load git (skip GitHub).
        #[arg(long, conflicts_with = "github_only")]
        git_only: bool,
        /// Only load GitHub (skip git).
        #[arg(long)]
        github_only: bool,
    },
    /// Run continuously: fetch + load git on ref changes + GitHub on a timer.
    Watch {
        #[arg(default_value = ".")]
        path: String,
        #[arg(long, default_value = "entl.duckdb")]
        db: String,
        /// GitHub poll interval in seconds.
        #[arg(long, default_value_t = 60)]
        interval: u64,
    },
    /// Run an analysis over already-loaded data.
    Analysis {
        #[command(subcommand)]
        cmd: AnalysisCmd,
    },
    /// Run a SQL query and print the result.
    Query {
        sql: String,
        #[arg(long, default_value = "entl.duckdb")]
        db: String,
    },
    /// List the tables in the DB.
    Tables {
        #[arg(long, default_value = "entl.duckdb")]
        db: String,
    },
}

#[derive(Subcommand)]
enum AnalysisCmd {
    /// Replay every merge to find conflict hot zones (needs `load` first).
    MergeConflicts {
        #[arg(default_value = ".")]
        path: String,
        #[arg(long, default_value = "entl.duckdb")]
        db: String,
    },
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Init { db } => {
            let d = Db::open(&db)?;
            d.migrate()?;
            println!("initialized {db} ({} migrations applied)", d.applied_migrations()?);
        }
        Cmd::Load { path, db, git_only, github_only } => {
            let d = Db::open(&db)?;
            d.migrate()?;
            if !github_only {
                let pb = ProgressBar::new_spinner();
                pb.set_style(
                    ProgressStyle::with_template("{spinner:.green} {human_pos} commits  {per_sec}  {elapsed}")
                        .unwrap(),
                );
                pb.enable_steady_tick(Duration::from_millis(120));
                // Worker threads bump `counter`; a ticker mirrors it onto the bar.
                let counter = Arc::new(AtomicU64::new(0));
                let done = Arc::new(AtomicBool::new(false));
                let ticker = {
                    let (c, dn, pb) = (counter.clone(), done.clone(), pb.clone());
                    std::thread::spawn(move || {
                        while !dn.load(Ordering::Relaxed) {
                            pb.set_position(c.load(Ordering::Relaxed));
                            std::thread::sleep(Duration::from_millis(120));
                        }
                    })
                };
                let t0 = std::time::Instant::now();
                let r = entl_core::ingest_git(&d, &path, &counter)?;
                done.store(true, Ordering::Relaxed);
                ticker.join().ok();
                pb.finish_and_clear();
                eprintln!(
                    "git: +{} commits, {} file changes, {} refs in {:.1}s",
                    r.new_commits, r.file_changes, r.refs, t0.elapsed().as_secs_f64(),
                );
            }
            if !git_only {
                let t0 = std::time::Instant::now();
                let r = entl_core::ingest_github(&d, &path)?;
                eprintln!(
                    "github: {} events, {} PRs, {} reviews, {} review-comments, {} issues, {} comments, {} runs, {} checks, {} users in {:.1}s",
                    r.events, r.pull_requests, r.reviews, r.review_comments, r.issues,
                    r.comments, r.workflow_runs, r.check_runs, r.users,
                    t0.elapsed().as_secs_f64(),
                );
            }
        }
        Cmd::Watch { path, db, interval } => {
            run_watch(&path, &db, interval)?;
        }
        Cmd::Analysis { cmd } => match cmd {
            AnalysisCmd::MergeConflicts { path, db } => {
                let d = Db::open(&db)?;
                d.migrate()?;
                let pb = ProgressBar::new_spinner();
                pb.set_style(
                    ProgressStyle::with_template("{spinner:.green} {human_pos} merges replayed  {per_sec}  {elapsed}")
                        .unwrap(),
                );
                pb.enable_steady_tick(Duration::from_millis(120));
                let t0 = std::time::Instant::now();
                let s = entl_core::analyze_conflicts(&d, &path, |n| pb.set_position(n))?;
                pb.finish_and_clear();
                eprintln!(
                    "merges: {} replayed ({} octopus, {} no-base skipped) → {} conflicting paths ({} needed manual resolution) in {:.1}s",
                    s.merges_analyzed, s.octopus_skipped, s.no_base_skipped,
                    s.conflict_paths, s.unresolved_paths, t0.elapsed().as_secs_f64(),
                );
                eprintln!("\ntop merge-conflict hot zones (unresolved):");
                println!(
                    "{}",
                    d.query_table(
                        "SELECT path, count(DISTINCT merge_oid) AS conflicting_merges
                         FROM conflicts WHERE unresolved
                         GROUP BY path ORDER BY conflicting_merges DESC LIMIT 15"
                    )?
                );
            }
        },
        Cmd::Query { sql, db } => {
            let d = Db::open(&db)?;
            d.migrate()?;
            println!("{}", d.query_table(&sql)?);
        }
        Cmd::Tables { db } => {
            let d = Db::open(&db)?;
            d.migrate()?;
            println!(
                "{}",
                d.query_table(
                    "SELECT table_name FROM information_schema.tables \
                     WHERE table_schema = 'main' AND table_name NOT LIKE '\\_%' ESCAPE '\\' \
                     ORDER BY table_name"
                )?
            );
        }
    }
    Ok(())
}

enum Tick {
    Git,
    Github,
}

/// A `.git` change worth re-syncing on: a ref moved (new commit, fetch, branch).
/// Ignores object/index churn so we only sync when history actually advances.
fn is_ref_change(p: &Path) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    p.to_string_lossy().contains("/refs/") || name == "HEAD" || name == "packed-refs"
}

/// `git fetch` to pull new commits/refs from the remote. Fetch only (never merge),
/// so the user's working tree + checked-out branch are untouched — we just update
/// remote-tracking refs + objects, which the next ingest walks. Also fetches
/// `refs/pull/*/head` (not in the default refspec) so every PR's commits are local —
/// PR diffs then work even for merged/deleted-branch PRs.
fn git_fetch(path: &str) {
    match std::process::Command::new("git")
        .args(["-C", path, "fetch", "--all", "--prune", "--tags", "--quiet"])
        .output()
    {
        Ok(o) if !o.status.success() => {
            eprintln!("git fetch failed: {}", String::from_utf8_lossy(&o.stderr).trim())
        }
        Err(e) => eprintln!("git fetch error: {e}"),
        _ => {}
    }
    // PR heads — best-effort (no-op on non-GitHub remotes / repos without PRs).
    let _ = std::process::Command::new("git")
        .args([
            "-C", path, "fetch", "origin",
            "+refs/pull/*/head:refs/remotes/origin/pull/*", "--quiet",
        ])
        .output();
}

fn sync_git_once(d: &Db, path: &str) {
    let counter = Arc::new(AtomicU64::new(0));
    match entl_core::ingest_git(d, path, &counter) {
        Ok(r) if r.new_commits > 0 => {
            eprintln!("git: +{} commits, {} file changes", r.new_commits, r.file_changes)
        }
        Ok(_) => {}
        Err(e) => eprintln!("git sync error: {e:#}"),
    }
}

fn sync_github_once(d: &Db, path: &str) {
    match entl_core::ingest_github(d, path) {
        Ok(r) if r.events + r.pull_requests + r.issues + r.workflow_runs > 0 => eprintln!(
            "github: +{} events, +{} PRs, +{} issues, +{} runs",
            r.events, r.pull_requests, r.issues, r.workflow_runs
        ),
        Ok(_) => {}
        Err(e) => eprintln!("github sync error: {e:#}"),
    }
}

/// Continuous sync: file-watch `.git` refs (debounced) + poll GitHub on a timer.
/// One thread owns all writes (DuckDB is single-writer); events are coalesced.
fn run_watch(path: &str, db: &str, interval: u64) -> Result<()> {
    let d = Db::open(db)?;
    d.migrate()?;
    eprintln!("watching {path} → {db}  (git: fetch + ingest every {interval}s, instant on local commits; github every {interval}s; Ctrl-C to stop)");

    // Initial refresh so the DB is current before we start watching.
    git_fetch(path);
    sync_git_once(&d, path);
    sync_github_once(&d, path);

    let (tx, rx) = crossbeam_channel::unbounded::<Tick>();

    // Git: watch the repo's `.git` dir, filter to ref changes.
    let git_dir = {
        let g = Path::new(path).join(".git");
        if g.is_dir() {
            g
        } else {
            Path::new(path).to_path_buf()
        }
    };
    let txw = tx.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res {
            if ev.paths.iter().any(|p| is_ref_change(p)) {
                let _ = txw.send(Tick::Git);
            }
        }
    })?;
    watcher.watch(&git_dir, RecursiveMode::Recursive)?;

    // GitHub: tick on a timer.
    let txt = tx.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(interval));
        if txt.send(Tick::Github).is_err() {
            break;
        }
    });

    // Coalesce bursts: block for the first event, then drain for a short window.
    loop {
        let Ok(first) = rx.recv() else { break };
        let (mut git, mut github) = (false, false);
        match first {
            Tick::Git => git = true,
            Tick::Github => github = true,
        }
        let deadline = Instant::now() + Duration::from_millis(400);
        while let Ok(ev) = rx.recv_deadline(deadline) {
            match ev {
                Tick::Git => git = true,
                Tick::Github => github = true,
            }
        }
        // A timer tick means "refresh from remotes": fetch git, then ingest any
        // new commits (local or fetched), then poll GitHub. A notify-only tick is a
        // local commit — just ingest.
        if github {
            git_fetch(path);
        }
        if git || github {
            sync_git_once(&d, path);
        }
        if github {
            sync_github_once(&d, path);
        }
    }
    Ok(())
}
