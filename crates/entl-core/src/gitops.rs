//! Live git reads (gix, straight off on-disk refs/objects) backing the operational
//! helpers powdermonkey used to shell `git` for: branch existence, current branch,
//! reachable commit messages, remote branch listing. Unlike the DB tables (a periodic
//! snapshot), these reflect the current on-disk state on every call — the node binding
//! `git fetch`es first when remote freshness matters.

use anyhow::{Context, Result};

/// True if `name` resolves to a commit, following git's normal ref precedence
/// (mirrors `git rev-parse --verify <name>`). The caller asks for `origin/<name>`
/// explicitly when it wants the remote-tracking ref — so this stays exact, keeping
/// worktree-add's local-vs-remote branching correct.
pub fn branch_exists(repo_path: &str, name: &str) -> Result<bool> {
    let repo = gix::discover(repo_path)?;
    Ok(repo.rev_parse_single(name).is_ok())
}

/// The checked-out branch (HEAD's short name), or "HEAD" when detached.
pub fn current_branch(repo_path: &str) -> Result<String> {
    let repo = gix::discover(repo_path)?;
    let head = repo.head().context("read HEAD")?;
    Ok(head
        .referent_name()
        .map(|n| n.shorten().to_string())
        .unwrap_or_else(|| "HEAD".to_string()))
}

/// Commit message bodies for every commit reachable from `branch` (mirrors
/// `git log <branch> --format=%B%x00`), NUL-separated. Seeds the walk from *both*
/// the local branch and `origin/<branch>`, so a just-fetched merge on `origin/main`
/// is scanned even when local `main` hasn't been pulled — the freshness reconcile's
/// trailer scan needs. "" when neither resolves.
pub fn commit_bodies(repo_path: &str, branch: &str) -> Result<String> {
    let repo = gix::discover(repo_path)?;
    let mut tips = Vec::new();
    if let Ok(t) = repo.rev_parse_single(branch) {
        tips.push(t.detach());
    }
    if let Ok(t) = repo.rev_parse_single(format!("origin/{branch}").as_str()) {
        tips.push(t.detach());
    }
    if tips.is_empty() {
        return Ok(String::new());
    }
    let mut out = String::new();
    for info in repo.rev_walk(tips).all()? {
        let Ok(info) = info else { continue };
        let Ok(commit) = repo.find_commit(info.id) else { continue };
        if let Ok(msg) = commit.message_raw() {
            out.push_str(&msg.to_string());
            out.push('\0');
        }
    }
    Ok(out)
}

/// Remote branch names (without the `refs/heads/` prefix) matching `pattern` — a
/// trailing-`*` glob like `refs/heads/pm/task-12*` or `pm/task-12*`. Reads the
/// `origin/*` remote-tracking refs; mirrors `git ls-remote --heads origin <pattern>`.
/// Fetch first for freshness.
pub fn ls_remote_heads(repo_path: &str, pattern: &str) -> Result<Vec<String>> {
    let repo = gix::discover(repo_path)?;
    let glob = pattern.strip_prefix("refs/heads/").unwrap_or(pattern);
    let platform = repo.references()?;
    let mut out = Vec::new();
    for r in platform.all()?.filter_map(std::result::Result::ok) {
        if r.name().category() != Some(gix::reference::Category::RemoteBranch) {
            continue;
        }
        // shorten() → "origin/pm/task-12-foo"; drop the remote name to get the branch.
        let short = r.name().shorten().to_string();
        let branch = short.splitn(2, '/').nth(1).unwrap_or(&short);
        if branch != "HEAD" && glob_match(glob, branch) {
            out.push(branch.to_string());
        }
    }
    Ok(out)
}

/// Minimal trailing-`*` glob — the only form powdermonkey uses (`pm/task-12*`).
fn glob_match(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == pattern,
    }
}
