//! Git → DuckDB ingest via gix (notes/design/engine.md).
//!
//! Per-commit work (decode + tree-diff + numstat) runs in parallel across worker
//! threads (each with its own gix repo + object cache); rows are written serially
//! through the DuckDB **Appender** (DuckDB is single-writer). `refs` are bulk-
//! replaced, `repos` upserted. Incremental: the walk skips oids already in `commits`.

use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use duckdb::params;
use duckdb::types::{TimeUnit, Value};
use gix::bstr::ByteSlice;
use gix::object::tree::diff::Change;
use rayon::prelude::*;

use crate::db::Db;

#[derive(Debug, Default)]
pub struct GitIngest {
    pub new_commits: usize,
    pub file_changes: usize,
    pub refs: usize,
}

struct RefRecord {
    name: String,
    kind: &'static str,
    target_oid: gix::ObjectId,
    is_symbolic: bool,
}

// Owned row buffers. Oids are kept as `ObjectId` (Copy, 20 bytes, no heap alloc)
// and appended as raw BLOB bytes — no hex encoding (notes/design/engine.md / 0004_hex_views).
struct CommitRow {
    oid: gix::ObjectId,
    tree_oid: gix::ObjectId,
    message: String,
    summary: String,
    author_name: String,
    author_email: String,
    author_us: i64,
    author_tz: String,
    committer_name: String,
    committer_email: String,
    committer_us: i64,
    committer_tz: String,
    parent_count: i32,
    is_merge: bool,
    gpg_signed: bool,
}
struct ParentRow {
    commit_oid: gix::ObjectId,
    parent_oid: gix::ObjectId,
    idx: i32,
}
struct ChangeRow {
    commit_oid: gix::ObjectId,
    path: String,
    old_path: Option<String>,
    status: &'static str,
    additions: Option<i32>,
    deletions: Option<i32>,
    blob_oid: Option<gix::ObjectId>,
    old_blob_oid: Option<gix::ObjectId>,
}

/// git's "+HHMM" tz, from an offset in seconds east of UTC.
fn fmt_tz(offset_secs: i32) -> String {
    let sign = if offset_secs < 0 { '-' } else { '+' };
    let a = offset_secs.unsigned_abs();
    format!("{sign}{:02}{:02}", a / 3600, (a % 3600) / 60)
}

fn collect_refs(repo: &gix::Repository) -> Result<Vec<RefRecord>> {
    let mut out = Vec::new();
    // HEAD
    if let Ok(id) = repo.head_id() {
        out.push(RefRecord {
            name: "HEAD".into(),
            kind: "head",
            target_oid: id.detach(),
            is_symbolic: true,
        });
    }
    let platform = repo.references().context("open refs")?;
    for r in platform.all().context("iterate refs")? {
        let mut r = match r {
            Ok(r) => r,
            Err(_) => continue,
        };
        let kind = match r.name().category() {
            Some(gix::reference::Category::LocalBranch) => "branch",
            Some(gix::reference::Category::RemoteBranch) => "remote",
            Some(gix::reference::Category::Tag) => "tag",
            _ => continue, // skip notes, stash, etc.
        };
        let short = r.name().shorten().to_string();
        let Ok(id) = r.peel_to_id() else { continue };
        out.push(RefRecord {
            name: short,
            kind,
            target_oid: id.detach(),
            is_symbolic: false,
        });
    }
    Ok(out)
}

/// Stable repo id (hash of the canonical worktree path) shared by all passes,
/// plus the canonical path string. Keeps `conflicts`/`commits` join keys aligned.
pub fn compute_repo_id(repo: &gix::Repository) -> (String, String) {
    let abs: PathBuf = repo
        .workdir()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| repo.git_dir().to_path_buf());
    let canon = std::fs::canonicalize(&abs).unwrap_or(abs);
    let canon_str = canon.to_string_lossy().to_string();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    canon_str.hash(&mut h);
    (format!("{:016x}", h.finish()), canon_str)
}

/// All rows derived from one commit (computed in parallel, written serially).
struct Bundle {
    commit: CommitRow,
    parents: Vec<ParentRow>,
    changes: Vec<ChangeRow>,
}

const OBJ_CACHE: usize = 96 * 1024 * 1024; // per-thread object cache

/// Ingest git history. `counter` is incremented per commit for live progress.
pub fn ingest_git(db: &Db, path: &str, counter: &AtomicU64) -> Result<GitIngest> {
    let mut repo = gix::discover(path).context("discover git repo")?;
    repo.object_cache_size(OBJ_CACHE);
    let (repo_id, canon_str) = compute_repo_id(&repo);
    let remote_url: Option<String> = repo
        .config_snapshot()
        .string("remote.origin.url")
        .map(|s| s.to_string());

    db.conn.execute(
        "INSERT INTO repos (id, path, remote_url, first_synced_at, last_synced_at)
         VALUES (?, ?, ?, now()::TIMESTAMP, now()::TIMESTAMP)
         ON CONFLICT (id) DO UPDATE SET
           path = excluded.path, remote_url = excluded.remote_url,
           last_synced_at = now()::TIMESTAMP",
        params![repo_id, canon_str, remote_url],
    )?;

    // Seen-set for the incremental walk (oids are BLOB -> read as bytes).
    let mut seen: HashSet<gix::ObjectId> = HashSet::new();
    {
        let mut stmt = db.conn.prepare("SELECT oid FROM commits WHERE repo_id = ?")?;
        let rows = stmt.query_map([&repo_id], |r| r.get::<_, Vec<u8>>(0))?;
        for r in rows {
            if let Ok(oid) = gix::ObjectId::try_from(r?.as_slice()) {
                seen.insert(oid);
            }
        }
    }

    // Refs: collect + seed walk tips. A repo can have thousands of refs (ksql:
    // 17k tags), so we bulk-replace (delete + one Appender batch) rather than run
    // a per-ref upsert — the latter was the real ingest bottleneck.
    let refs = collect_refs(&repo)?;
    let mut tips: Vec<gix::ObjectId> = Vec::new();
    let mut by_name: std::collections::HashMap<&str, &RefRecord> =
        std::collections::HashMap::new();
    for rr in &refs {
        // Tags can point at non-commit objects (the kernel has tags on trees);
        // only seed the walk with tips that actually resolve to commits.
        if repo.find_commit(rr.target_oid).is_ok() {
            tips.push(rr.target_oid);
        }
        by_name.insert(&rr.name, rr); // dedup by name (branch vs tag collisions)
    }
    db.conn
        .execute("DELETE FROM refs WHERE repo_id = ?", params![repo_id])?;
    {
        let mut app = db.conn.appender("refs")?;
        for rr in by_name.values() {
            app.append_row(params![
                repo_id,
                rr.name,
                rr.kind,
                rr.target_oid.as_bytes(),
                rr.is_symbolic,
                None::<String>,
            ])?;
        }
        app.flush()?;
    }

    // Walk once (cheap) to collect new commit oids; the heavy per-commit work
    // (decode + tree-diff + numstat) is parallelized below.
    let mut new_oids: Vec<gix::ObjectId> = Vec::new();
    for info in repo.rev_walk(tips.iter().copied()).all()? {
        let oid = info?.id;
        if seen.insert(oid) {
            new_oids.push(oid);
        }
    }

    // Pipeline: rayon workers compute bundles in parallel → bounded channel → a
    // single writer thread (its own cloned connection) appends them as they
    // arrive. This overlaps the serial DuckDB write under the parallel compute,
    // and bounds memory (no collecting all rows first — matters at kernel scale).
    let refs_n = refs.len();
    let repo_id_w = repo_id.clone();
    let writer_conn = db.conn.try_clone()?;
    let (tx, rx) = crossbeam_channel::bounded::<Bundle>(2048);

    let (new_commits, file_changes) = std::thread::scope(|scope| -> Result<(usize, usize)> {
        let writer = scope.spawn(move || -> Result<(usize, usize)> {
            let mut ca = writer_conn.appender("commits")?;
            let mut pa = writer_conn.appender("commit_parents")?;
            let mut fa = writer_conn.appender("file_changes")?;
            let (mut nc, mut nfc) = (0usize, 0usize);
            for b in rx.iter() {
                let r = &b.commit;
                ca.append_row(params![
                    r.oid.as_bytes(), repo_id_w, r.tree_oid.as_bytes(), r.message, r.summary,
                    r.author_name, r.author_email,
                    Value::Timestamp(TimeUnit::Microsecond, r.author_us), r.author_tz,
                    r.committer_name, r.committer_email,
                    Value::Timestamp(TimeUnit::Microsecond, r.committer_us), r.committer_tz,
                    r.parent_count, r.is_merge, r.gpg_signed,
                ])?;
                for p in &b.parents {
                    pa.append_row(params![p.commit_oid.as_bytes(), p.parent_oid.as_bytes(), p.idx])?;
                }
                for c in &b.changes {
                    nfc += 1;
                    fa.append_row(params![
                        c.commit_oid.as_bytes(), c.path, c.old_path, c.status,
                        c.additions, c.deletions,
                        c.blob_oid.as_ref().map(|o| o.as_bytes()),
                        c.old_blob_oid.as_ref().map(|o| o.as_bytes()),
                    ])?;
                }
                nc += 1;
            }
            ca.flush()?;
            pa.flush()?;
            fa.flush()?;
            Ok((nc, nfc))
        });

        let tsr = repo.into_sync();
        new_oids.par_iter().for_each_init(
            || {
                let mut trepo = tsr.to_thread_local();
                trepo.object_cache_size(OBJ_CACHE);
                let rcache = trepo
                    .diff_resource_cache_for_tree_diff()
                    .expect("resource cache");
                (trepo, rcache)
            },
            |(trepo, rcache), &oid| {
                if let Ok(b) = compute_commit(trepo, rcache, oid) {
                    counter.fetch_add(1, Ordering::Relaxed);
                    let _ = tx.send(b);
                }
            },
        );
        drop(tx); // close channel so the writer's loop ends
        writer
            .join()
            .map_err(|_| anyhow::anyhow!("writer thread panicked"))?
    })?;

    Ok(GitIngest {
        refs: refs_n,
        new_commits,
        file_changes,
    })
}

/// Per-commit extraction (runs on a worker thread).
fn compute_commit(
    repo: &gix::Repository,
    rcache: &mut gix::diff::blob::Platform,
    oid: gix::ObjectId,
) -> Result<Bundle> {
    let object = repo.find_commit(oid)?;
    let c = object.decode()?;
    let full_msg = c.message.to_str_lossy().into_owned();
    let summary = full_msg.lines().next().unwrap_or("").to_string();
    let author = c.author()?;
    let committer = c.committer()?;
    let a_time = author.time().unwrap_or_default();
    let c_time = committer.time().unwrap_or_default();
    let parents: Vec<gix::ObjectId> = c.parents().collect();

    // file_changes (+ numstat) vs first parent — gix `Tree::stats()` pattern.
    let mut changes: Vec<ChangeRow> = Vec::new();
    let new_tree = object.tree()?;
    let old_tree = match parents.first() {
        Some(p0) => repo.find_commit(*p0).ok().and_then(|c| c.tree().ok()),
        None => Some(repo.empty_tree()),
    };
    if let Some(old_tree) = old_tree {
        old_tree.changes()?.for_each_to_obtain_tree(&new_tree, |change| {
            if let Some(row) = map_change(oid, change, rcache) {
                changes.push(row);
            }
            rcache.clear_resource_cache_keep_allocation();
            Ok::<_, std::convert::Infallible>(ControlFlow::Continue(()))
        })?;
    }

    let parent_rows = parents
        .iter()
        .enumerate()
        .map(|(idx, p)| ParentRow {
            commit_oid: oid,
            parent_oid: *p,
            idx: idx as i32,
        })
        .collect();

    let commit = CommitRow {
        oid,
        tree_oid: c.tree(),
        message: full_msg,
        summary,
        author_name: author.name.to_str_lossy().into_owned(),
        author_email: author.email.to_str_lossy().into_owned(),
        author_us: a_time.seconds.saturating_mul(1_000_000),
        author_tz: fmt_tz(a_time.offset),
        committer_name: committer.name.to_str_lossy().into_owned(),
        committer_email: committer.email.to_str_lossy().into_owned(),
        committer_us: c_time.seconds.saturating_mul(1_000_000),
        committer_tz: fmt_tz(c_time.offset),
        parent_count: parents.len() as i32,
        is_merge: parents.len() > 1,
        gpg_signed: c.extra_headers().pgp_signature().is_some(),
    };
    Ok(Bundle {
        commit,
        parents: parent_rows,
        changes,
    })
}

fn map_change(
    commit_oid: gix::ObjectId,
    change: Change<'_, '_, '_>,
    rcache: &mut gix::diff::blob::Platform,
) -> Option<ChangeRow> {
    // numstat via gix's diff pipeline (its content normalization is what keeps
    // exact `git --numstat` parity). None -> NULL for binaries.
    let (additions, deletions) = match change
        .diff(rcache)
        .ok()
        .and_then(|mut p| p.line_counts().ok())
        .flatten()
    {
        Some(c) => (Some(c.insertions as i32), Some(c.removals as i32)),
        None => (None, None),
    };

    match change {
        Change::Addition { location, entry_mode, id, .. } => entry_mode.is_blob().then(|| ChangeRow {
            commit_oid,
            path: location.to_string(),
            old_path: None,
            status: "A",
            additions,
            deletions,
            blob_oid: Some(id.detach()),
            old_blob_oid: None,
        }),
        Change::Deletion { location, entry_mode, id, .. } => entry_mode.is_blob().then(|| ChangeRow {
            commit_oid,
            path: location.to_string(),
            old_path: None,
            status: "D",
            additions,
            deletions,
            blob_oid: None,
            old_blob_oid: Some(id.detach()),
        }),
        Change::Modification { location, entry_mode, id, previous_id, .. } => {
            entry_mode.is_blob().then(|| ChangeRow {
                commit_oid,
                path: location.to_string(),
                old_path: None,
                status: "M",
                additions,
                deletions,
                blob_oid: Some(id.detach()),
                old_blob_oid: Some(previous_id.detach()),
            })
        }
        Change::Rewrite { location, source_location, id, source_id, .. } => Some(ChangeRow {
            commit_oid,
            path: location.to_string(),
            old_path: Some(source_location.to_string()),
            status: "R",
            additions,
            deletions,
            blob_oid: Some(id.detach()),
            old_blob_oid: Some(source_id.detach()),
        }),
    }
}

