//! Reassemble the fake forge from the stored `gh_*` tables and compare it to the generated
//! [`ForgeWorld`] — the forge analogue of rebuilding a git repo (P3). Both sides are lowered to a
//! canonical JSON keyed by natural keys (user id, PR/issue number, event id) with references
//! resolved (author → id, commit → oid), so the comparison is order- and representation-agnostic.
//! Top-level scalar fields are compared; the sub-resources (reviews/comments/…) are covered by the
//! store round-trip (P3) and would add index-vs-oid dedup subtleties here.

use entl_core::extract::Snapshot;
use serde_json::{json, Value};

use crate::forge::ForgeWorld;

fn sorted(mut v: Vec<Value>) -> Value {
    v.sort_by_cached_key(|x| x.to_string());
    Value::Array(v)
}

/// Canonical form of the *generated* forge (resolve indices → ids/oids).
pub fn canonical_forge(w: &ForgeWorld, oids: &[String]) -> Value {
    let oid = |i: usize| -> String {
        if oids.is_empty() { String::new() } else { oids[i % oids.len()].clone() }
    };
    let uid = |idx: Option<usize>| -> Value {
        idx.and_then(|i| w.users.get(i)).map(|u| Value::from(u.id)).unwrap_or(Value::Null)
    };
    let oid_opt = |idx: Option<usize>| -> Value {
        idx.map(|i| Value::String(oid(i))).unwrap_or(Value::Null)
    };

    let pulls = sorted(w.pulls.iter().map(|p| json!({
        "number": p.number, "state": p.state, "is_draft": p.is_draft, "author_id": uid(p.author),
        "title": p.title, "body": p.body, "mergeable": p.mergeable, "checks": p.rollup,
        "additions": p.additions, "deletions": p.deletions, "changed_files": p.changed_files,
        "head_ref": p.head_ref, "base_ref": p.base_ref,
        "head_oid": oid_opt(p.head_commit), "base_oid": oid_opt(p.base_commit),
        "merge_commit_oid": oid_opt(p.merge_commit),
    })).collect());
    let issues = sorted(w.issues.iter().map(|i| json!({
        "number": i.number, "state": i.state, "author_id": uid(i.author), "title": i.title, "body": i.body,
    })).collect());
    let events = sorted(w.events.iter().map(|e| json!({
        "id": e.id, "type": e.typ, "actor_id": uid(e.actor),
    })).collect());

    // The `users`/`labels` pools may contain entries no PR/issue references; the ingest only
    // stores referenced ones (covered by the store round-trip), so they're excluded here.
    json!({ "pulls": pulls, "issues": issues, "events": events })
}

/// Canonical form reassembled from the stored `gh_*` tables.
pub fn canonical_store(snap: &Snapshot) -> Value {
    let empty = Vec::new();
    let rows = |t: &str| snap.get(t).unwrap_or(&empty);
    let g = |r: &std::collections::BTreeMap<String, Value>, k: &str| r.get(k).cloned().unwrap_or(Value::Null);

    let pulls = sorted(rows("gh_pull_requests").iter().map(|r| json!({
        "number": g(r, "number"), "state": g(r, "state"), "is_draft": g(r, "is_draft"), "author_id": g(r, "author_id"),
        "title": g(r, "title"), "body": g(r, "body"), "mergeable": g(r, "mergeable"), "checks": g(r, "checks"),
        "additions": g(r, "additions"), "deletions": g(r, "deletions"), "changed_files": g(r, "changed_files"),
        "head_ref": g(r, "head_ref"), "base_ref": g(r, "base_ref"),
        "head_oid": g(r, "head_oid"), "base_oid": g(r, "base_oid"), "merge_commit_oid": g(r, "merge_commit_oid"),
    })).collect());
    let issues = sorted(rows("gh_issues").iter().map(|r| json!({
        "number": g(r, "number"), "state": g(r, "state"), "author_id": g(r, "author_id"), "title": g(r, "title"), "body": g(r, "body"),
    })).collect());
    let events = sorted(rows("gh_events").iter().map(|r| json!({
        "id": g(r, "id"), "type": g(r, "type"), "actor_id": g(r, "actor_id"),
    })).collect());

    json!({ "pulls": pulls, "issues": issues, "events": events })
}
