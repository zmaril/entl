//! Writing git repos with `git fast-import` — the shared primitive behind both `rebuild` (a store
//! → repo, [`crate::rebuild`]) and the round-trip test's materialize (a generated world → repo).
//!
//! fast-import is deterministic: identical input → identical commit OIDs. We build the command
//! stream, feed it on stdin, and read back the mark→OID map it exports.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

/// A commit ready for fast-import: full tree (path, bytes, octal mode) + parents (by prior index)
/// + author/committer `(name, email, unix_secs, tz)` + message.
pub struct SnapCommit {
    pub parents: Vec<usize>,
    pub tree: Vec<(String, Vec<u8>, String)>,
    pub author: (String, String, i64, String),
    pub committer: (String, String, i64, String),
    pub message: String,
}

/// A ref to set after import: full name → commit index.
pub struct SnapRef {
    pub name: String,
    pub target: usize,
}

/// Build the fast-import command stream. Commit `i` gets mark `i+1`.
pub fn fast_import_stream(commits: &[SnapCommit], refs: &[SnapRef]) -> Vec<u8> {
    let mut s: Vec<u8> = Vec::new();
    for (i, c) in commits.iter().enumerate() {
        let mark = i + 1;
        // Build on a scratch branch; real refs are reset afterwards.
        s.extend_from_slice(b"commit refs/heads/__entl_build\n");
        s.extend_from_slice(format!("mark :{mark}\n").as_bytes());
        person(&mut s, "author", &c.author);
        person(&mut s, "committer", &c.committer);
        s.extend_from_slice(format!("data {}\n", c.message.len()).as_bytes());
        s.extend_from_slice(c.message.as_bytes());
        s.extend_from_slice(b"\n");
        if let Some(p0) = c.parents.first() {
            s.extend_from_slice(format!("from :{}\n", p0 + 1).as_bytes());
            for p in &c.parents[1..] {
                s.extend_from_slice(format!("merge :{}\n", p + 1).as_bytes());
            }
        }
        // Re-materialize the whole tree each commit so it's exactly this set of files.
        s.extend_from_slice(b"deleteall\n");
        for (path, content, mode) in &c.tree {
            s.extend_from_slice(format!("M {mode} inline {path}\n").as_bytes());
            s.extend_from_slice(format!("data {}\n", content.len()).as_bytes());
            s.extend_from_slice(content);
            s.extend_from_slice(b"\n");
        }
        s.extend_from_slice(b"\n");
    }
    for r in refs {
        s.extend_from_slice(format!("reset {}\n", r.name).as_bytes());
        s.extend_from_slice(format!("from :{}\n\n", r.target + 1).as_bytes());
    }
    s
}

fn person(w: &mut Vec<u8>, kind: &str, (name, email, ts, tz): &(String, String, i64, String)) {
    w.extend_from_slice(format!("{kind} {name} <{email}> {ts} {tz}\n").as_bytes());
}

/// `git init` + `fast-import` the commits/refs into `repo`, returning each commit's OID (hex) by
/// index. HEAD is pointed at the first `refs/heads/*` ref so the repo is checkout-shaped.
pub fn import(repo: &Path, commits: &[SnapCommit], refs: &[SnapRef]) -> Result<Vec<String>> {
    std::fs::create_dir_all(repo)?;
    git(repo, &["init", "-q"])?;
    git(repo, &["config", "core.autocrlf", "false"])?;

    let marks = repo.join("entl-marks");
    let mut child = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["fast-import", "--quiet", "--done"])
        .arg(format!("--export-marks={}", marks.display()))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .context("spawn git fast-import")?;
    let mut stream = fast_import_stream(commits, refs);
    stream.extend_from_slice(b"done\n"); // `--done` requires a trailing `done` command
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&stream)
        .context("write fast-import stream")?;
    if !child.wait()?.success() {
        bail!("git fast-import failed");
    }

    let marks_txt = std::fs::read_to_string(&marks).context("read export-marks")?;
    let mut oids = vec![String::new(); commits.len()];
    for line in marks_txt.lines() {
        if let Some((m, oid)) = line.split_once(' ') {
            if let Ok(mark) = m.trim_start_matches(':').parse::<usize>() {
                if mark >= 1 && mark <= commits.len() {
                    oids[mark - 1] = oid.trim().to_string();
                }
            }
        }
    }
    let _ = std::fs::remove_file(&marks);
    git(repo, &["update-ref", "-d", "refs/heads/__entl_build"]).ok();
    if let Some(r) = refs.iter().find(|r| r.name.starts_with("refs/heads/")) {
        git(repo, &["symbolic-ref", "HEAD", &r.name]).ok();
    }
    Ok(oids)
}

/// Run a git command in `repo`, erroring on non-zero exit; returns stdout.
pub fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .with_context(|| format!("git {args:?}"))?;
    if !out.status.success() {
        bail!("git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
