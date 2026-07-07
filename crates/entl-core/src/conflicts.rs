//! Merge-conflict hot zones (the north star).
//!
//! Replays every historical 2-parent merge with gix's in-process 3-way tree
//! merge and records the paths that conflicted. Writes the `conflicts` table;
//! "hot zones" are then a `GROUP BY path`. Object writes from the merge are kept
//! in memory (`with_object_memory`) so we never pollute the source repo.

use std::collections::HashMap;

use anyhow::{Context, Result};
use duckdb::params;
use gix::merge::tree::TreatAsUnresolved;

use crate::db::Db;
use crate::ingest::compute_repo_id;

#[derive(Debug, Default)]
pub struct ConflictStats {
    pub merges_analyzed: usize,
    pub octopus_skipped: usize,
    pub no_base_skipped: usize,
    pub conflict_paths: usize,
    pub unresolved_paths: usize,
}

const CHUNK: usize = 2000;

struct Row {
    merge_oid: gix::ObjectId,
    path: String,
    unresolved: bool,
}

pub fn analyze_conflicts(
    db: &Db,
    path: &str,
    mut on_progress: impl FnMut(u64),
) -> Result<ConflictStats> {
    let repo = gix::discover(path)
        .context("discover git repo")?
        .with_object_memory();
    let (repo_id, _canon) = compute_repo_id(&repo);

    // Full recompute for this repo (re-runnable).
    db.conn
        .execute("DELETE FROM conflicts WHERE repo_id = ?", params![repo_id])?;

    // Merge commits come from the already-ingested `commits` table (oid is BLOB).
    let merges: Vec<gix::ObjectId> = {
        let mut stmt = db
            .conn
            .prepare("SELECT oid FROM commits WHERE is_merge AND repo_id = ?")?;
        let rows = stmt.query_map([&repo_id], |r| r.get::<_, Vec<u8>>(0))?;
        rows.filter_map(|r| r.ok())
            .filter_map(|b| gix::ObjectId::try_from(b.as_slice()).ok())
            .collect()
    };

    let mut stats = ConflictStats::default();
    let mut buf: Vec<Row> = Vec::new();

    for &oid in &merges {
        let Ok(commit) = repo.find_commit(oid) else {
            continue;
        };
        let parents: Vec<gix::ObjectId> = commit.parent_ids().map(|i| i.detach()).collect();
        if parents.len() != 2 {
            stats.octopus_skipped += 1; // octopus / root — handle pairwise later
            continue;
        }

        match conflicts_for_merge(&repo, oid, parents[0], parents[1]) {
            Ok(rows) => {
                stats.merges_analyzed += 1;
                for r in rows {
                    stats.conflict_paths += 1;
                    if r.unresolved {
                        stats.unresolved_paths += 1;
                    }
                    buf.push(r);
                }
            }
            Err(MergeSkip::NoBase) => stats.no_base_skipped += 1,
            Err(MergeSkip::Other) => {}
        }

        if buf.len() >= CHUNK {
            flush(db, &repo_id, &mut buf)?;
            on_progress(stats.merges_analyzed as u64);
        }
    }
    flush(db, &repo_id, &mut buf)?;
    Ok(stats)
}

enum MergeSkip {
    NoBase,
    Other,
}

fn conflicts_for_merge(
    repo: &gix::Repository,
    merge_oid: gix::ObjectId,
    p1: gix::ObjectId,
    p2: gix::ObjectId,
) -> Result<Vec<Row>, MergeSkip> {
    let base = repo
        .merge_base(p1, p2)
        .map_err(|_| MergeSkip::NoBase)?
        .detach();

    let tree_of = |oid: gix::ObjectId| -> Result<gix::ObjectId, MergeSkip> {
        Ok(repo
            .find_commit(oid)
            .map_err(|_| MergeSkip::Other)?
            .tree_id()
            .map_err(|_| MergeSkip::Other)?
            .detach())
    };
    let base_tree = tree_of(base)?;
    let our_tree = tree_of(p1)?;
    let their_tree = tree_of(p2)?;

    let options = repo.tree_merge_options().map_err(|_| MergeSkip::Other)?;
    let outcome = repo
        .merge_trees(base_tree, our_tree, their_tree, Default::default(), options)
        .map_err(|_| MergeSkip::Other)?;

    // Dedup paths within a merge (rename conflicts can emit two entries);
    // keep unresolved = OR across entries for the same path.
    let mut by_path: HashMap<String, bool> = HashMap::new();
    for c in &outcome.conflicts {
        let p = c.ours.location().to_string();
        let unresolved = c.is_unresolved(TreatAsUnresolved::git());
        let e = by_path.entry(p).or_insert(false);
        *e = *e || unresolved;
    }
    Ok(by_path
        .into_iter()
        .map(|(path, unresolved)| Row {
            merge_oid,
            path,
            unresolved,
        })
        .collect())
}

fn flush(db: &Db, repo_id: &str, buf: &mut Vec<Row>) -> Result<()> {
    if buf.is_empty() {
        return Ok(());
    }
    let mut app = db.conn.appender("conflicts")?;
    for r in buf.iter() {
        app.append_row(params![
            repo_id,
            r.merge_oid.as_bytes(),
            r.path,
            r.unresolved
        ])?;
    }
    app.flush()?;
    buf.clear();
    Ok(())
}
