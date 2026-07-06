//! Diff any two commits → per-file changes + a full unified patch, computed
//! locally from git objects (gix tree-diff + imara unified-diff). No GitHub
//! round-trip — this is the offline read path for PR review / "what changed
//! between X and Y".

use std::ops::ControlFlow;

use anyhow::{Context, Result};
use gix::object::tree::diff::Change;
use serde::Serialize;

#[derive(Serialize)]
pub struct FileDiff {
    pub path: String,
    pub old_path: Option<String>,
    /// A | M | D | R
    pub status: &'static str,
    pub additions: i64,
    pub deletions: i64,
    /// Full single-file unified patch (`diff --git` header + hunks). Empty for
    /// binary files (either side non-UTF-8), matching what GitHub omits.
    pub patch: String,
}

/// Per-file diff between two commits. With `three_dot`, the base is shifted to the
/// merge-base of (base, head) — GitHub's `base...head` PR diff.
pub fn diff_commits(
    repo_path: &str,
    base_hex: &str,
    head_hex: &str,
    three_dot: bool,
) -> Result<Vec<FileDiff>> {
    let repo = gix::discover(repo_path).context("discover git repo")?;
    let head = gix::ObjectId::from_hex(head_hex.as_bytes()).context("bad head oid")?;
    let mut base = gix::ObjectId::from_hex(base_hex.as_bytes()).context("bad base oid")?;
    // Three-dot (GitHub's base...head) uses the merge-base; if it can't be computed
    // (e.g. histories not both local), fall back to a plain base..head diff.
    if three_dot {
        if let Ok(mb) = repo.merge_base(base, head) {
            base = mb.detach();
        }
    }
    let base_tree = repo
        .find_commit(base)
        .context("base commit not present locally (fetch the repo)")?
        .tree()?;
    let head_tree = repo
        .find_commit(head)
        .context("head commit not present locally (fetch the repo)")?
        .tree()?;
    let mut rcache = repo
        .diff_resource_cache_for_tree_diff()
        .context("resource cache")?;

    let mut out = Vec::new();
    base_tree
        .changes()?
        .for_each_to_obtain_tree(&head_tree, |change| {
            if let Some(fd) = map_file_diff(&repo, change, &mut rcache) {
                out.push(fd);
            }
            rcache.clear_resource_cache_keep_allocation();
            Ok::<_, std::convert::Infallible>(ControlFlow::Continue(()))
        })?;
    Ok(out)
}

/// Full UTF-8 text of `path` at `commit`, or `None` if absent/binary. For PR-review
/// context expansion (old/new side).
pub fn file_at(repo_path: &str, commit_hex: &str, path: &str) -> Result<Option<String>> {
    let repo = gix::discover(repo_path).context("discover git repo")?;
    let oid = gix::ObjectId::from_hex(commit_hex.as_bytes()).context("bad commit oid")?;
    let tree = repo.find_commit(oid)?.tree()?;
    match tree.lookup_entry_by_path(std::path::Path::new(path))? {
        Some(entry) => {
            let data = repo.find_object(entry.oid().to_owned())?.data.clone();
            Ok(String::from_utf8(data).ok())
        }
        None => Ok(None),
    }
}

fn blob(repo: &gix::Repository, oid: Option<gix::ObjectId>) -> Vec<u8> {
    oid.and_then(|o| repo.find_object(o).ok())
        .map(|o| o.data.clone())
        .unwrap_or_default()
}

fn map_file_diff(
    repo: &gix::Repository,
    change: Change<'_, '_, '_>,
    rcache: &mut gix::diff::blob::Platform,
) -> Option<FileDiff> {
    let (additions, deletions) = match change
        .diff(rcache)
        .ok()
        .and_then(|mut p| p.line_counts().ok())
        .flatten()
    {
        Some(c) => (c.insertions as i64, c.removals as i64),
        None => (0, 0),
    };
    let (path, old_path, status, old_oid, new_oid) = match change {
        Change::Addition {
            location,
            entry_mode,
            id,
            ..
        } => {
            entry_mode.is_blob().then_some(())?;
            (location.to_string(), None, "A", None, Some(id.detach()))
        }
        Change::Deletion {
            location,
            entry_mode,
            id,
            ..
        } => {
            entry_mode.is_blob().then_some(())?;
            (location.to_string(), None, "D", Some(id.detach()), None)
        }
        Change::Modification {
            location,
            entry_mode,
            id,
            previous_id,
            ..
        } => {
            entry_mode.is_blob().then_some(())?;
            (
                location.to_string(),
                None,
                "M",
                Some(previous_id.detach()),
                Some(id.detach()),
            )
        }
        Change::Rewrite {
            location,
            source_location,
            id,
            source_id,
            ..
        } => (
            location.to_string(),
            Some(source_location.to_string()),
            "R",
            Some(source_id.detach()),
            Some(id.detach()),
        ),
    };
    let old = blob(repo, old_oid);
    let new = blob(repo, new_oid);
    let patch = unified_patch(
        &old,
        &new,
        old_path.as_deref().unwrap_or(&path),
        &path,
        status,
    );
    Some(FileDiff {
        path,
        old_path,
        status,
        additions,
        deletions,
        patch,
    })
}

fn unified_patch(old: &[u8], new: &[u8], old_name: &str, new_name: &str, status: &str) -> String {
    let (Ok(old_s), Ok(new_s)) = (std::str::from_utf8(old), std::str::from_utf8(new)) else {
        return String::new(); // binary → no patch (like GitHub)
    };
    use gix::diff::blob::{
        sources::lines, Algorithm, BasicLineDiffPrinter, Diff, InternedInput, UnifiedDiffConfig,
    };
    let input = InternedInput::new(lines(old_s), lines(new_s));
    let diff = Diff::compute(Algorithm::Histogram, &input);
    let hunks = diff
        .unified_diff(
            &BasicLineDiffPrinter(&input.interner),
            UnifiedDiffConfig::default(),
            &input,
        )
        .to_string();
    if hunks.is_empty() {
        return String::new();
    }
    let old_path = if status == "A" {
        "/dev/null".into()
    } else {
        format!("a/{old_name}")
    };
    let new_path = if status == "D" {
        "/dev/null".into()
    } else {
        format!("b/{new_name}")
    };
    format!("diff --git a/{old_name} b/{new_name}\n--- {old_path}\n+++ {new_path}\n{hunks}")
}
