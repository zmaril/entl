//! Rebuild — reconstruct a real git repo from a store (`entl rebuild`; see notes/design/testing.md).
//!
//! The inverse of [`ingest`](crate::ingest) + [`objects`](crate::objects): read the canonical
//! git tables (commits/parents/refs + trees/tree_entries/blobs) back into fast-import commits and
//! replay them. Because git OIDs are content hashes, a faithful store yields byte-identical OIDs —
//! this is both the round-trip test's P2 and a real "rehydrate a repo from Postgres" capability.
//! Requires object ingest (`--objects`) to have run, so the full tree/blob content is present.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;

use crate::extract::{Snapshot, GIT_FULL_TABLES};
use crate::gitwrite::{import, SnapCommit, SnapRef};
use crate::pull::SinkTarget;

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .filter_map(|i| s.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()))
        .collect()
}

fn s<'a>(row: &'a BTreeMap<String, Value>, k: &str) -> &'a str {
    row.get(k).and_then(Value::as_str).unwrap_or("")
}

/// Reconstruct a repo from a canonical [`Snapshot`] into `repo`; returns each commit's OID by the
/// order it was written (topological). The store must include the object tables.
pub fn rebuild_from_snapshot(snap: &Snapshot, repo: &Path) -> Result<Vec<String>> {
    let empty = Vec::new();
    let get = |t: &str| snap.get(t).unwrap_or(&empty);

    // blob oid(hex) → content bytes.
    let blobs: HashMap<String, Vec<u8>> = get("blobs")
        .iter()
        .map(|r| (s(r, "oid").to_string(), unhex(s(r, "content"))))
        .collect();

    // tree oid(hex) → ordered entries.
    let mut tree_entries: HashMap<String, Vec<(String, String, String, String)>> = HashMap::new();
    for r in get("tree_entries") {
        tree_entries.entry(s(r, "tree_oid").to_string()).or_default().push((
            s(r, "name").to_string(),
            s(r, "mode").to_string(),
            s(r, "entry_type").to_string(),
            s(r, "child_oid").to_string(),
        ));
    }

    // parents by commit oid, in idx order.
    let mut parents: HashMap<String, Vec<(i64, String)>> = HashMap::new();
    for r in get("commit_parents") {
        let idx = r.get("idx").and_then(Value::as_i64).unwrap_or(0);
        parents
            .entry(s(r, "commit_oid").to_string())
            .or_default()
            .push((idx, s(r, "parent_oid").to_string()));
    }
    for v in parents.values_mut() {
        v.sort_by_key(|(i, _)| *i);
    }

    // Commit metadata by oid.
    struct C {
        oid: String,
        tree_oid: String,
        author: (String, String, i64, String),
        committer: (String, String, i64, String),
        message: String,
        parents: Vec<String>,
    }
    let mut commits: Vec<C> = Vec::new();
    for r in get("commits") {
        let oid = s(r, "oid").to_string();
        let par = parents.get(&oid).map(|v| v.iter().map(|(_, p)| p.clone()).collect()).unwrap_or_default();
        commits.push(C {
            tree_oid: s(r, "tree_oid").to_string(),
            author: sig(r, "author"),
            committer: sig(r, "committer"),
            message: s(r, "message").to_string(),
            parents: par,
            oid,
        });
    }
    if commits.is_empty() {
        bail!("store has no commits to rebuild");
    }

    // Topological order (Kahn): a commit is ready once all its parents are emitted.
    let idx_by_oid: HashMap<&str, usize> = commits.iter().enumerate().map(|(i, c)| (c.oid.as_str(), i)).collect();
    let mut indeg: Vec<usize> = commits.iter().map(|c| c.parents.iter().filter(|p| idx_by_oid.contains_key(p.as_str())).count()).collect();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); commits.len()];
    for (i, c) in commits.iter().enumerate() {
        for p in &c.parents {
            if let Some(&pi) = idx_by_oid.get(p.as_str()) {
                children[pi].push(i);
            }
        }
    }
    let mut queue: VecDeque<usize> = (0..commits.len()).filter(|&i| indeg[i] == 0).collect();
    let mut order = Vec::with_capacity(commits.len());
    while let Some(i) = queue.pop_front() {
        order.push(i);
        for &ch in &children[i] {
            indeg[ch] -= 1;
            if indeg[ch] == 0 {
                queue.push_back(ch);
            }
        }
    }
    if order.len() != commits.len() {
        bail!("commit graph has a cycle or missing parents");
    }
    // position in the fast-import stream (index) for each commit oid.
    let pos: HashMap<&str, usize> = order.iter().enumerate().map(|(pos, &i)| (commits[i].oid.as_str(), pos)).collect();

    // Build the fast-import commits in topo order.
    let mut snap_commits = Vec::with_capacity(order.len());
    for &i in &order {
        let c = &commits[i];
        let mut tree = Vec::new();
        expand_tree(&c.tree_oid, "", &tree_entries, &blobs, &mut tree)
            .with_context(|| format!("expand tree for commit {}", c.oid))?;
        let parents = c.parents.iter().filter_map(|p| pos.get(p.as_str()).copied()).collect();
        snap_commits.push(SnapCommit {
            parents,
            tree,
            author: c.author.clone(),
            committer: c.committer.clone(),
            message: c.message.clone(),
        });
    }

    // Refs → full names, targeting the commit's stream position.
    let mut refs = Vec::new();
    for r in get("refs") {
        let name = s(r, "name");
        let full = match s(r, "kind") {
            "branch" => format!("refs/heads/{name}"),
            "tag" => format!("refs/tags/{name}"),
            "remote" => format!("refs/remotes/{name}"),
            _ => continue, // HEAD / symbolic — set by import
        };
        if let Some(&target) = pos.get(s(r, "target_oid")) {
            refs.push(SnapRef { name: full, target });
        }
    }

    import(repo, &snap_commits, &refs)
}

fn sig(row: &BTreeMap<String, Value>, who: &str) -> (String, String, i64, String) {
    let when = row
        .get(&format!("{who}_when"))
        .and_then(Value::as_str)
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .map(|dt| dt.timestamp())
        .unwrap_or(0);
    (
        s(row, &format!("{who}_name")).to_string(),
        s(row, &format!("{who}_email")).to_string(),
        when,
        s(row, &format!("{who}_tz")).to_string(),
    )
}

/// Recursively flatten a tree into `(full_path, octal_mode, content)` leaves.
fn expand_tree(
    tree_oid: &str,
    prefix: &str,
    entries: &HashMap<String, Vec<(String, String, String, String)>>,
    blobs: &HashMap<String, Vec<u8>>,
    out: &mut Vec<(String, Vec<u8>, String)>,
) -> Result<()> {
    let Some(children) = entries.get(tree_oid) else {
        return Ok(()); // empty tree (or absent)
    };
    for (name, mode, etype, child) in children {
        let path = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        match etype.as_str() {
            "tree" => expand_tree(child, &path, entries, blobs, out)?,
            "commit" => {} // gitlink/submodule — no content to write
            _ => {
                let content = blobs
                    .get(child)
                    .ok_or_else(|| anyhow!("missing blob {child} for {path}"))?
                    .clone();
                out.push((path, content, mode.clone()));
            }
        }
    }
    Ok(())
}

/// Extract the git tables from a sink store and rebuild a repo at `out`. `dest` is the store
/// location (SQLite file, JSONL dir, or Postgres URL); `schema` is the Postgres schema.
pub fn rebuild_from_store(
    target: SinkTarget,
    dest: &str,
    schema: Option<&str>,
    out: &Path,
) -> Result<Vec<String>> {
    let snap = match target {
        SinkTarget::Sqlite => {
            crate::extract::extract_sqlite(dest, GIT_FULL_TABLES, &Default::default())?
        }
        SinkTarget::Jsonl => crate::extract::extract_jsonl(dest, GIT_FULL_TABLES)?,
        SinkTarget::Postgres => {
            crate::extract::extract_postgres(dest, schema.unwrap_or("entl"), GIT_FULL_TABLES)?
        }
    };
    rebuild_from_snapshot(&snap, out)
}

/// Rebuild a repo from any store named by string: `duckdb` | `sqlite` | `jsonl` | `postgres`.
/// The shared entry for the CLI and the language bindings.
pub fn rebuild_store(from: &str, dest: &str, schema: Option<&str>, out: &Path) -> Result<Vec<String>> {
    if from == "duckdb" {
        let d = crate::db::Db::open(dest)?;
        let snap = crate::extract::extract_duckdb(&d.conn, GIT_FULL_TABLES)?;
        rebuild_from_snapshot(&snap, out)
    } else {
        rebuild_from_store(from.parse()?, dest, schema, out)
    }
}
