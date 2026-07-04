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
use crate::stream::{ChangeBatch, ChangeOp, ChangeSink};
use duckdb::arrow::array::{
    ArrayRef, BinaryBuilder, BooleanBuilder, Int32Builder, StringBuilder,
    TimestampMicrosecondBuilder,
};
use duckdb::arrow::datatypes::{DataType, Field, Schema};
use duckdb::arrow::record_batch::RecordBatch;
use std::sync::Arc;

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
// and appended as raw BLOB bytes — no hex encoding (the hex views in migrations/duckdb/extras.sql
// project them to hex on read; see notes/design/engine.md).
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
            _ => continue, // skip stash etc. (notes are collected separately)
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

/// One git note: which notes ref it lives under, the object it annotates, and
/// the note text.
struct NoteRecord {
    notes_ref: String,
    annotated_oid: gix::ObjectId,
    note: String,
}

/// Walk every `refs/notes/*` ref: its tip commit's tree maps the annotated
/// object's hex oid — possibly fanned out across subdirectories — to a note
/// blob; joining the path segments back together recovers the oid.
fn collect_notes(repo: &gix::Repository) -> Result<Vec<NoteRecord>> {
    let mut out = Vec::new();
    let platform = repo.references().context("open refs")?;
    let Ok(iter) = platform.prefixed("refs/notes/") else {
        return Ok(out);
    };
    for r in iter {
        let Ok(mut r) = r else { continue };
        let name = r.name().as_bstr().to_string();
        let Ok(id) = r.peel_to_id() else { continue };
        let Ok(obj) = repo.find_object(id.detach()) else { continue };
        let Ok(commit) = obj.try_into_commit() else { continue };
        let Ok(tree) = commit.tree() else { continue };
        walk_notes(repo, &tree, String::new(), &name, &mut out)?;
    }
    Ok(out)
}

fn walk_notes(
    repo: &gix::Repository,
    tree: &gix::Tree<'_>,
    prefix: String,
    notes_ref: &str,
    out: &mut Vec<NoteRecord>,
) -> Result<()> {
    for e in tree.iter() {
        let e = e?;
        let seg = e.filename().to_str_lossy().into_owned();
        match e.mode().kind() {
            gix::object::tree::EntryKind::Tree => {
                let sub = repo.find_object(e.oid().to_owned())?.into_tree();
                walk_notes(repo, &sub, format!("{prefix}{seg}"), notes_ref, out)?;
            }
            gix::object::tree::EntryKind::Blob
            | gix::object::tree::EntryKind::BlobExecutable => {
                let hex = format!("{prefix}{seg}");
                let Ok(oid) = gix::ObjectId::from_hex(hex.as_bytes()) else { continue };
                let Ok(blob) = repo.find_object(e.oid().to_owned()) else { continue };
                out.push(NoteRecord {
                    notes_ref: notes_ref.to_string(),
                    annotated_oid: oid,
                    note: String::from_utf8_lossy(&blob.data).into_owned(),
                });
            }
            _ => {}
        }
    }
    Ok(())
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

/// Ingest git history (no change stream) — see [`ingest_git_streamed`].
pub fn ingest_git(db: &Db, path: &str, counter: &AtomicU64) -> Result<GitIngest> {
    ingest_git_streamed(db, path, counter, None)
}

/// Ingest git history, teeing the new rows into `sink` as Arrow change batches
/// (notes/design/engine.md, "The change stream"). `counter` is incremented per
/// commit for live progress.
pub fn ingest_git_streamed(
    db: &Db,
    path: &str,
    counter: &AtomicU64,
    sink: Option<&ChangeSink>,
) -> Result<GitIngest> {
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

    // Stream refs from memory — the repo's refs are wholesale-replaced each pull.
    if let Some(sink) = sink {
        let rows: Vec<&RefRecord> = by_name.values().copied().collect();
        sink.emit(ChangeBatch::new("refs", ChangeOp::Replace, refs_batch(&rows, &repo_id)?));
    }

    // Git notes (refs/notes/*): mutable per-repo state like refs — bulk-replace.
    let notes = collect_notes(&repo)?;
    db.conn
        .execute("DELETE FROM git_notes WHERE repo_id = ?", params![repo_id])?;
    if !notes.is_empty() {
        let mut app = db.conn.appender("git_notes")?;
        for n in &notes {
            app.append_row(params![repo_id, n.notes_ref, n.annotated_oid.as_bytes(), n.note])?;
        }
        app.flush()?;
    }
    if let Some(sink) = sink {
        if !notes.is_empty() {
            sink.emit(ChangeBatch::new(
                "git_notes",
                ChangeOp::Replace,
                notes_batch(&notes, &repo_id)?,
            ));
        }
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
            let (mut cbuf, mut pbuf, mut fbuf) =
                (Vec::<CommitRow>::new(), Vec::<ParentRow>::new(), Vec::<ChangeRow>::new());
            let mut live = true;
            for b in rx.iter() {
                {
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
                        // generated column order: (commit_oid, idx, parent_oid) — edge tables emit
                        // source, LOCAL-KEY props, then target (schema_gen is canonical)
                        pa.append_row(params![p.commit_oid.as_bytes(), p.idx, p.parent_oid.as_bytes()])?;
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
                // Feed the change stream from these in-memory rows *during* the pull.
                if let (Some(sink), true) = (sink, live) {
                    cbuf.push(b.commit);
                    pbuf.extend(b.parents);
                    fbuf.extend(b.changes);
                    if cbuf.len() >= STREAM_CHUNK {
                        live = flush_git_stream(sink, &repo_id_w, &mut cbuf, &mut pbuf, &mut fbuf)?;
                    }
                }
            }
            ca.flush()?;
            pa.flush()?;
            fa.flush()?;
            if let (Some(sink), true) = (sink, live) {
                flush_git_stream(sink, &repo_id_w, &mut cbuf, &mut pbuf, &mut fbuf)?;
            }
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

// ── Arrow batch builders: the change stream is fed from these in-memory rows
// *during* the pull (not by reading DuckDB back). oids stay raw `Binary` (sinks
// hex them); timestamps are `Timestamp(us)`; nullable columns use append_option.

/// How many commits to accumulate before emitting a batch (bounds memory).
const STREAM_CHUNK: usize = 2000;

fn commits_batch(rows: &[CommitRow], repo_id: &str) -> Result<RecordBatch> {
    let (mut oid, mut tree) = (BinaryBuilder::new(), BinaryBuilder::new());
    let (mut rid, mut msg, mut summ) = (StringBuilder::new(), StringBuilder::new(), StringBuilder::new());
    let (mut an, mut ae, mut atz) = (StringBuilder::new(), StringBuilder::new(), StringBuilder::new());
    let (mut cn, mut ce, mut ctz) = (StringBuilder::new(), StringBuilder::new(), StringBuilder::new());
    let (mut aw, mut cw) = (TimestampMicrosecondBuilder::new(), TimestampMicrosecondBuilder::new());
    let mut pc = Int32Builder::new();
    let (mut merge, mut gpg) = (BooleanBuilder::new(), BooleanBuilder::new());
    for r in rows {
        oid.append_value(r.oid.as_bytes());
        rid.append_value(repo_id);
        tree.append_value(r.tree_oid.as_bytes());
        msg.append_value(&r.message);
        summ.append_value(&r.summary);
        an.append_value(&r.author_name);
        ae.append_value(&r.author_email);
        aw.append_value(r.author_us);
        atz.append_value(&r.author_tz);
        cn.append_value(&r.committer_name);
        ce.append_value(&r.committer_email);
        cw.append_value(r.committer_us);
        ctz.append_value(&r.committer_tz);
        pc.append_value(r.parent_count);
        merge.append_value(r.is_merge);
        gpg.append_value(r.gpg_signed);
    }
    let ts = || DataType::Timestamp(duckdb::arrow::datatypes::TimeUnit::Microsecond, None);
    let schema = Arc::new(Schema::new(vec![
        Field::new("oid", DataType::Binary, false),
        Field::new("repo_id", DataType::Utf8, false),
        Field::new("tree_oid", DataType::Binary, false),
        Field::new("message", DataType::Utf8, false),
        Field::new("summary", DataType::Utf8, false),
        Field::new("author_name", DataType::Utf8, true),
        Field::new("author_email", DataType::Utf8, true),
        Field::new("author_when", ts(), true),
        Field::new("author_tz", DataType::Utf8, true),
        Field::new("committer_name", DataType::Utf8, true),
        Field::new("committer_email", DataType::Utf8, true),
        Field::new("committer_when", ts(), true),
        Field::new("committer_tz", DataType::Utf8, true),
        Field::new("parent_count", DataType::Int32, false),
        Field::new("is_merge", DataType::Boolean, false),
        Field::new("gpg_signed", DataType::Boolean, false),
    ]));
    let cols: Vec<ArrayRef> = vec![
        Arc::new(oid.finish()), Arc::new(rid.finish()), Arc::new(tree.finish()),
        Arc::new(msg.finish()), Arc::new(summ.finish()), Arc::new(an.finish()),
        Arc::new(ae.finish()), Arc::new(aw.finish()), Arc::new(atz.finish()),
        Arc::new(cn.finish()), Arc::new(ce.finish()), Arc::new(cw.finish()),
        Arc::new(ctz.finish()), Arc::new(pc.finish()), Arc::new(merge.finish()),
        Arc::new(gpg.finish()),
    ];
    Ok(RecordBatch::try_new(schema, cols)?)
}

fn parents_batch(rows: &[ParentRow]) -> Result<RecordBatch> {
    let (mut co, mut po, mut idx) = (BinaryBuilder::new(), BinaryBuilder::new(), Int32Builder::new());
    for r in rows {
        co.append_value(r.commit_oid.as_bytes());
        po.append_value(r.parent_oid.as_bytes());
        idx.append_value(r.idx);
    }
    let schema = Arc::new(Schema::new(vec![
        Field::new("commit_oid", DataType::Binary, false),
        Field::new("parent_oid", DataType::Binary, false),
        Field::new("idx", DataType::Int32, false),
    ]));
    let cols: Vec<ArrayRef> = vec![Arc::new(co.finish()), Arc::new(po.finish()), Arc::new(idx.finish())];
    Ok(RecordBatch::try_new(schema, cols)?)
}

fn file_changes_batch(rows: &[ChangeRow]) -> Result<RecordBatch> {
    let (mut co, mut bo, mut obo) = (BinaryBuilder::new(), BinaryBuilder::new(), BinaryBuilder::new());
    let (mut path, mut oldp, mut status) = (StringBuilder::new(), StringBuilder::new(), StringBuilder::new());
    let (mut add, mut del) = (Int32Builder::new(), Int32Builder::new());
    for r in rows {
        co.append_value(r.commit_oid.as_bytes());
        path.append_value(&r.path);
        oldp.append_option(r.old_path.as_deref());
        status.append_value(r.status);
        add.append_option(r.additions);
        del.append_option(r.deletions);
        bo.append_option(r.blob_oid.as_ref().map(|o| o.as_bytes()));
        obo.append_option(r.old_blob_oid.as_ref().map(|o| o.as_bytes()));
    }
    let schema = Arc::new(Schema::new(vec![
        Field::new("commit_oid", DataType::Binary, false),
        Field::new("path", DataType::Utf8, false),
        Field::new("old_path", DataType::Utf8, true),
        Field::new("status", DataType::Utf8, false),
        Field::new("additions", DataType::Int32, true),
        Field::new("deletions", DataType::Int32, true),
        Field::new("blob_oid", DataType::Binary, true),
        Field::new("old_blob_oid", DataType::Binary, true),
    ]));
    let cols: Vec<ArrayRef> = vec![
        Arc::new(co.finish()), Arc::new(path.finish()), Arc::new(oldp.finish()),
        Arc::new(status.finish()), Arc::new(add.finish()), Arc::new(del.finish()),
        Arc::new(bo.finish()), Arc::new(obo.finish()),
    ];
    Ok(RecordBatch::try_new(schema, cols)?)
}

fn refs_batch(rows: &[&RefRecord], repo_id: &str) -> Result<RecordBatch> {
    let (mut rid, mut name, mut kind, mut up) =
        (StringBuilder::new(), StringBuilder::new(), StringBuilder::new(), StringBuilder::new());
    let mut tgt = BinaryBuilder::new();
    let mut sym = BooleanBuilder::new();
    for r in rows {
        rid.append_value(repo_id);
        name.append_value(&r.name);
        kind.append_value(r.kind);
        tgt.append_value(r.target_oid.as_bytes());
        sym.append_value(r.is_symbolic);
        up.append_null(); // upstream — not tracked here yet
    }
    let schema = Arc::new(Schema::new(vec![
        Field::new("repo_id", DataType::Utf8, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("kind", DataType::Utf8, false),
        Field::new("target_oid", DataType::Binary, false),
        Field::new("is_symbolic", DataType::Boolean, false),
        Field::new("upstream", DataType::Utf8, true),
    ]));
    let cols: Vec<ArrayRef> = vec![
        Arc::new(rid.finish()), Arc::new(name.finish()), Arc::new(kind.finish()),
        Arc::new(tgt.finish()), Arc::new(sym.finish()), Arc::new(up.finish()),
    ];
    Ok(RecordBatch::try_new(schema, cols)?)
}

fn notes_batch(rows: &[NoteRecord], repo_id: &str) -> Result<RecordBatch> {
    let (mut rid, mut nref, mut note) =
        (StringBuilder::new(), StringBuilder::new(), StringBuilder::new());
    let mut oid = BinaryBuilder::new();
    for n in rows {
        rid.append_value(repo_id);
        nref.append_value(&n.notes_ref);
        oid.append_value(n.annotated_oid.as_bytes());
        note.append_value(&n.note);
    }
    let schema = Arc::new(Schema::new(vec![
        Field::new("repo_id", DataType::Utf8, false),
        Field::new("notes_ref", DataType::Utf8, false),
        Field::new("annotated_oid", DataType::Binary, false),
        Field::new("note", DataType::Utf8, false),
    ]));
    let cols: Vec<ArrayRef> = vec![
        Arc::new(rid.finish()), Arc::new(nref.finish()), Arc::new(oid.finish()),
        Arc::new(note.finish()),
    ];
    Ok(RecordBatch::try_new(schema, cols)?)
}

/// Build + emit the accumulated commit/parent/file-change batches, then clear.
/// Returns `false` if the consumer hung up (caller should stop).
fn flush_git_stream(
    sink: &ChangeSink,
    repo_id: &str,
    cbuf: &mut Vec<CommitRow>,
    pbuf: &mut Vec<ParentRow>,
    fbuf: &mut Vec<ChangeRow>,
) -> Result<bool> {
    let mut live = true;
    if !cbuf.is_empty() {
        live &= sink.emit(ChangeBatch::new("commits", ChangeOp::Insert, commits_batch(cbuf, repo_id)?));
    }
    if !pbuf.is_empty() {
        live &= sink.emit(ChangeBatch::new("commit_parents", ChangeOp::Insert, parents_batch(pbuf)?));
    }
    if !fbuf.is_empty() {
        live &= sink.emit(ChangeBatch::new("file_changes", ChangeOp::Insert, file_changes_batch(fbuf)?));
    }
    cbuf.clear();
    pbuf.clear();
    fbuf.clear();
    Ok(live)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{change_channel, Db, Poll};
    use std::collections::HashMap;
    use std::process::Command;
    use std::time::Duration;

    fn git(dir: &std::path::Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@e")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@e")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?} failed");
    }

    /// End-to-end: ingest a real (tiny) repo and drain its changes off `poll`.
    #[test]
    fn ingest_streams_change_batches_to_poll() {
        let dir = std::env::temp_dir().join(format!("entl-stream-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]);
        std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-qm", "first"]);
        std::fs::write(dir.join("a.txt"), "hello\nworld\n").unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-qm", "second"]);

        let db = Db::open(":memory:").unwrap();
        db.migrate().unwrap();
        let (sink, stream) = change_channel(1024);
        let path = dir.to_string_lossy().to_string();

        // Producer on its own thread; dropping `sink` at the end closes the stream.
        let producer = std::thread::spawn(move || {
            let counter = AtomicU64::new(0);
            ingest_git_streamed(&db, &path, &counter, Some(&sink)).unwrap()
        });

        let mut rows: HashMap<String, usize> = HashMap::new();
        loop {
            match stream.poll(Duration::from_secs(10)) {
                Poll::Batch(b) => *rows.entry(b.table.clone()).or_default() += b.len(),
                Poll::Closed => break,
                Poll::Idle => panic!("producer stalled — no batch within timeout"),
            }
        }
        let ingest = producer.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(ingest.new_commits, 2, "ingested two commits");
        assert_eq!(rows.get("commits").copied().unwrap_or(0), 2, "two commit rows streamed");
        assert!(rows.get("file_changes").copied().unwrap_or(0) >= 2, "file_changes streamed");
        assert!(rows.contains_key("refs"), "refs streamed");
    }

    /// Git notes (refs/notes/*) land in `git_notes` — annotated oid recovered
    /// from the notes tree, note text intact — and re-sync replaces cleanly.
    #[test]
    fn ingest_captures_git_notes() {
        let dir = std::env::temp_dir().join(format!("entl-notes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]);
        std::fs::write(dir.join("a.txt"), "hello\n").unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-qm", "first"]);
        git(&dir, &["notes", "add", "-m", "reviewed: looks good"]);
        git(&dir, &["notes", "--ref", "refs/notes/pm", "add", "-m", "pm-state: shipped"]);

        let db = Db::open(":memory:").unwrap();
        db.migrate().unwrap();
        let path = dir.to_string_lossy().to_string();
        let counter = AtomicU64::new(0);
        ingest_git(&db, &path, &counter).unwrap();

        let (n, note): (i64, String) = db
            .conn
            .query_row(
                "SELECT count(*) OVER (), note FROM git_notes \
                 WHERE notes_ref = 'refs/notes/commits' \
                 AND annotated_oid = (SELECT oid FROM commits LIMIT 1)",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(note, "reviewed: looks good\n");
        let total: i64 =
            db.conn.query_row("SELECT count(*) FROM git_notes", [], |r| r.get(0)).unwrap();
        assert_eq!(total, 2, "both notes refs captured (got {n} in the commits ref)");

        // Re-sync replaces, not duplicates.
        ingest_git(&db, &path, &counter).unwrap();
        let total: i64 =
            db.conn.query_row("SELECT count(*) FROM git_notes", [], |r| r.get(0)).unwrap();
        assert_eq!(total, 2, "bulk-replace on re-sync");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

