//! Materialize a [`GitWorld`] into a real on-disk git repo, reusing the core `fast-import`
//! primitive ([`entl_core::gitwrite`]) shared with the `rebuild` feature.

use std::path::Path;

use anyhow::Result;
use entl_core::gitwrite::{import, SnapCommit, SnapRef};

use crate::world::{GenSig, GitWorld};

/// Lower a `GitWorld` to fast-import commits/refs and import it into `repo`; returns each commit's
/// OID (hex) by index.
pub fn materialize(world: &GitWorld, repo: &Path) -> Result<Vec<String>> {
    let commits: Vec<SnapCommit> = world
        .commits
        .iter()
        .map(|c| SnapCommit {
            parents: c.parents.clone(),
            tree: c
                .tree
                .iter()
                .map(|(p, b)| (p.clone(), b.content.clone(), b.mode.octal().to_string()))
                .collect(),
            author: sig(&c.author),
            committer: sig(&c.committer),
            message: c.message.clone(),
        })
        .collect();
    let refs: Vec<SnapRef> = world
        .refs
        .iter()
        .map(|r| SnapRef { name: r.name.clone(), target: r.target })
        .collect();
    import(repo, &commits, &refs)
}

fn sig(s: &GenSig) -> (String, String, i64, String) {
    (s.name.clone(), s.email.clone(), s.time_secs, s.tz.clone())
}
