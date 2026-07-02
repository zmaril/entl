use std::collections::BTreeMap;

use entl_testkit::{git, materialize, GenBlob, GenCommit, GenRef, GenSig, GitWorld, Mode};

fn blob(s: &str) -> GenBlob {
    GenBlob { content: s.as_bytes().to_vec(), mode: Mode::Normal }
}
fn sig(t: i64) -> GenSig {
    GenSig { name: "Test User".into(), email: "t@example.com".into(), time_secs: t, tz: "+0000".into() }
}

fn sample() -> GitWorld {
    let mut t0 = BTreeMap::new();
    t0.insert("a.txt".to_string(), blob("hello\n"));
    let mut t1 = t0.clone();
    t1.insert("dir/b.txt".to_string(), blob("world\n"));
    t1.insert("run.sh".to_string(), GenBlob { content: b"#!/bin/sh\n".to_vec(), mode: Mode::Exec });
    GitWorld {
        commits: vec![
            GenCommit { parents: vec![], tree: t0, author: sig(1_000_000_000), committer: sig(1_000_000_000), message: "first\n".into() },
            GenCommit { parents: vec![0], tree: t1, author: sig(1_000_000_100), committer: sig(1_000_000_100), message: "second\n".into() },
        ],
        refs: vec![
            GenRef { name: "refs/heads/main".into(), target: 1 },
            GenRef { name: "refs/tags/v1".into(), target: 0 },
        ],
    }
}

#[test]
fn materializes_valid_deterministic_repo() {
    let w = sample();
    let d1 = tempfile::tempdir().unwrap();
    let oids = materialize(&w, d1.path()).unwrap();
    assert_eq!(oids.len(), 2);
    assert!(oids.iter().all(|o| o.len() == 40), "oids: {oids:?}");

    // git agrees: main → commit 1, tag → commit 0, 2 commits reachable, fsck clean.
    assert_eq!(git(d1.path(), &["rev-parse", "refs/heads/main"]).unwrap().trim(), oids[1]);
    assert_eq!(git(d1.path(), &["rev-parse", "refs/tags/v1"]).unwrap().trim(), oids[0]);
    assert_eq!(git(d1.path(), &["rev-list", "--count", "refs/heads/main"]).unwrap().trim(), "2");
    git(d1.path(), &["fsck", "--strict"]).unwrap();
    // exec bit survived
    let ls = git(d1.path(), &["ls-tree", "refs/heads/main", "run.sh"]).unwrap();
    assert!(ls.starts_with("100755"), "expected exec mode, got: {ls}");

    // Determinism: a second identical materialize yields identical OIDs.
    let d2 = tempfile::tempdir().unwrap();
    let oids2 = materialize(&w, d2.path()).unwrap();
    assert_eq!(oids, oids2);
}
