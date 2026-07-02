//! proptest generators for [`GitWorld`]. We generate a simple, always-valid *recipe* (a list of
//! commit specs + extra refs) and fold it into a `GitWorld` deterministically — indices are taken
//! modulo their valid range so every generated value builds, which keeps shrinking clean.
//!
//! Strings (names/messages) are drawn from a constrained charset so they stay in the OID-exact
//! envelope (valid UTF-8, no control bytes) needed by the git-reassembly property later.

use std::collections::BTreeMap;

use proptest::prelude::*;

use crate::world::{GenBlob, GenCommit, GenRef, GenSig, GitWorld, Mode};

const PATHS: &[&str] = &[
    "a.txt", "b.txt", "c.md", "dir/x.txt", "dir/y.bin", "src/main.rs", "README", "d/e/f.txt",
];
const PEOPLE: &[(&str, &str)] = &[
    ("Alice", "alice@example.com"),
    ("Bob", "bob@example.com"),
    ("Carol", "carol@example.com"),
];

#[derive(Debug, Clone)]
enum RawOp {
    Put(usize, Vec<u8>, u8),
    Del(usize),
    Rename(usize, usize),
}

fn arb_op() -> impl Strategy<Value = RawOp> {
    prop_oneof![
        3 => (0..PATHS.len(), prop::collection::vec(any::<u8>(), 0..48), 0u8..3)
            .prop_map(|(p, c, m)| RawOp::Put(p, c, m)),
        1 => (0..PATHS.len()).prop_map(RawOp::Del),
        1 => (0..PATHS.len(), 0..PATHS.len()).prop_map(|(a, b)| RawOp::Rename(a, b)),
    ]
}

#[derive(Debug, Clone)]
struct RawCommit {
    ops: Vec<RawOp>,
    second_parent: Option<usize>,
    person: usize,
    dt: u32,
    msg: String,
}

fn arb_commit() -> impl Strategy<Value = RawCommit> {
    (
        prop::collection::vec(arb_op(), 0..4),
        prop::option::of(0usize..64),
        0usize..PEOPLE.len(),
        0u32..100_000,
        "[a-zA-Z0-9 ._-]{1,40}",
    )
        .prop_map(|(ops, second_parent, person, dt, msg)| RawCommit {
            ops,
            second_parent,
            person,
            dt,
            msg,
        })
}

/// A valid random git history: 1–7 commits (linear backbone + occasional merges), evolving trees
/// (adds/modifies/deletes/renames, regular/exec/symlink modes), and a handful of refs.
pub fn arb_git_world() -> impl Strategy<Value = GitWorld> {
    (
        prop::collection::vec(arb_commit(), 1..8),
        prop::collection::vec((0usize..3, 0usize..64), 0..3),
    )
        .prop_map(|(raws, extra_refs)| build(raws, extra_refs))
}

fn build(raws: Vec<RawCommit>, extra_refs: Vec<(usize, usize)>) -> GitWorld {
    let n = raws.len();
    let mut commits = Vec::with_capacity(n);
    // Backbone tree: parent0 is always i-1, so this evolves along the linear chain.
    let mut tree: BTreeMap<String, GenBlob> = BTreeMap::new();
    let mut time: i64 = 1_600_000_000;

    for (i, rc) in raws.into_iter().enumerate() {
        let mut parents = Vec::new();
        if i >= 1 {
            parents.push(i - 1);
            if i >= 2 {
                if let Some(sp) = rc.second_parent {
                    parents.push(sp % (i - 1)); // an earlier commit, distinct from i-1
                }
            }
        }
        for op in rc.ops {
            match op {
                RawOp::Put(p, content, m) => {
                    let path = PATHS[p % PATHS.len()];
                    let mode = match m % 3 {
                        0 => Mode::Normal,
                        1 => Mode::Exec,
                        _ => Mode::Symlink,
                    };
                    // A symlink's content is its target path (a valid link).
                    let content = if mode == Mode::Symlink {
                        path.as_bytes().to_vec()
                    } else {
                        content
                    };
                    tree.insert(path.to_string(), GenBlob { content, mode });
                }
                RawOp::Del(p) => {
                    tree.remove(PATHS[p % PATHS.len()]);
                }
                RawOp::Rename(a, b) => {
                    let (pa, pb) = (PATHS[a % PATHS.len()], PATHS[b % PATHS.len()]);
                    if pa != pb {
                        if let Some(bl) = tree.remove(pa) {
                            tree.insert(pb.to_string(), bl);
                        }
                    }
                }
            }
        }
        time += (rc.dt as i64) + 1;
        let (name, email) = PEOPLE[rc.person];
        let sig = GenSig {
            name: name.to_string(),
            email: email.to_string(),
            time_secs: time,
            tz: "+0000".to_string(),
        };
        commits.push(GenCommit {
            parents,
            tree: tree.clone(),
            author: sig.clone(),
            committer: sig,
            message: format!("{}\n", rc.msg),
        });
    }

    let mut refs = vec![GenRef { name: "refs/heads/main".to_string(), target: n - 1 }];
    for (k, (kind, tgt)) in extra_refs.into_iter().enumerate() {
        let target = tgt % n;
        let name = match kind {
            0 => format!("refs/heads/b{k}"),
            1 => format!("refs/tags/t{k}"),
            _ => format!("refs/heads/feat{k}"),
        };
        refs.push(GenRef { name, target });
    }
    GitWorld { commits, refs }
}
