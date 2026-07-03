//! Reassemble the fake forge from the stored `gh_*` tables and compare it to the generated
//! [`ForgeWorld`] — the forge analogue of rebuilding a git repo (P3). Both sides are lowered to a
//! canonical JSON keyed by natural keys (PR/issue number, review/comment id, event id) with
//! references resolved (author → id, commit → oid), so the comparison is order- and
//! representation-agnostic. Timestamps are omitted (representation differs `Z` vs `+00:00`, and the
//! store round-trip P3 already checks them); everything else — including the PR sub-resources — is
//! compared.

use std::collections::BTreeMap;

use entl_core::extract::Snapshot;
use serde_json::{json, Value};

use crate::forge::ForgeWorld;

type Row = BTreeMap<String, Value>;

fn sorted(mut v: Vec<Value>) -> Value {
    v.sort_by_cached_key(|x| x.to_string());
    Value::Array(v)
}

fn sorted_strs(mut v: Vec<String>) -> Value {
    v.sort();
    v.dedup();
    Value::Array(v.into_iter().map(Value::String).collect())
}

// ---- generated side ----

/// Canonical form of the *generated* forge (resolve indices → ids/oids).
pub fn canonical_forge(w: &ForgeWorld, oids: &[String]) -> Value {
    let oid = |i: usize| -> String {
        if oids.is_empty() { String::new() } else { oids[i % oids.len()].clone() }
    };
    let uid = |idx: Option<usize>| -> Value {
        idx.and_then(|i| w.users.get(i)).map(|u| Value::from(u.id)).unwrap_or(Value::Null)
    };
    let oid_opt = |idx: Option<usize>| idx.map(|i| Value::String(oid(i))).unwrap_or(Value::Null);
    let lname = |i: usize| w.labels.get(i).map(|l| l.name.clone()).unwrap_or_default();

    let pulls = sorted(w.pulls.iter().map(|p| json!({
        "number": p.number, "state": p.state, "is_draft": p.is_draft, "author_id": uid(p.author),
        "title": p.title, "body": p.body, "mergeable": p.mergeable, "checks": p.rollup,
        "additions": p.additions, "deletions": p.deletions, "changed_files": p.changed_files,
        "head_ref": p.head_ref, "base_ref": p.base_ref,
        "head_oid": oid_opt(p.head_commit), "base_oid": oid_opt(p.base_commit),
        "merge_commit_oid": oid_opt(p.merge_commit),
        "reviews": sorted(p.reviews.iter().map(|r| json!({
            "id": r.id, "reviewer_id": uid(r.author), "state": r.state, "body": r.body })).collect()),
        "comments": sorted(p.comments.iter().map(|c| json!({
            "id": c.id, "author_id": uid(c.author), "body": c.body })).collect()),
        "review_comments": sorted(p.review_comments.iter().map(|rc| json!({
            "id": rc.id, "author_id": uid(rc.author), "path": rc.path, "line": rc.line,
            "side": rc.side, "commit_oid": oid_opt(rc.commit), "body": rc.body })).collect()),
        "pr_commits": sorted_strs(p.commits.iter().map(|&i| oid(i)).collect()),
        "requested_reviewers": sorted(p.requested_reviewers.iter().map(|&i| uid(Some(i))).collect()),
        "labels": sorted_strs(p.labels.iter().map(|&i| lname(i)).collect()),
    })).collect());

    let issues = sorted(w.issues.iter().map(|i| json!({
        "number": i.number, "state": i.state, "author_id": uid(i.author), "title": i.title, "body": i.body,
        "comments": sorted(i.comments.iter().map(|c| json!({
            "id": c.id, "author_id": uid(c.author), "body": c.body })).collect()),
        "labels": sorted_strs(i.labels.iter().map(|&x| lname(x)).collect()),
    })).collect());

    let events = sorted(w.events.iter().map(|e| json!({
        "id": e.id, "type": e.typ, "actor_id": uid(e.actor) })).collect());

    // The users/labels *pools* may contain entries no PR/issue references; the ingest only stores
    // referenced ones (covered by the store round-trip), so they're excluded here.
    json!({ "pulls": pulls, "issues": issues, "events": events })
}

// ---- store side ----

fn gv(r: &Row, k: &str) -> Value {
    r.get(k).cloned().unwrap_or(Value::Null)
}
fn ni(r: &Row, k: &str) -> Option<i64> {
    r.get(k).and_then(Value::as_i64)
}
fn is_subject(x: &Row, st: &str, n: Option<i64>) -> bool {
    gv(x, "subject_type") == json!(st) && ni(x, "subject_number") == n
}

/// Canonical form reassembled from the stored `gh_*` tables.
pub fn canonical_store(snap: &Snapshot) -> Value {
    let empty = Vec::new();
    let rows = |t: &str| snap.get(t).unwrap_or(&empty).as_slice();
    let strs = |it: Vec<&Row>, col: &str| -> Value {
        sorted_strs(it.iter().filter_map(|r| r.get(col).and_then(Value::as_str).map(str::to_string)).collect())
    };

    let pulls = sorted(rows("gh_pull_requests").iter().map(|r| {
        let n = ni(r, "number");
        json!({
            "number": gv(r, "number"), "state": gv(r, "state"), "is_draft": gv(r, "is_draft"), "author_id": gv(r, "author_id"),
            "title": gv(r, "title"), "body": gv(r, "body"), "mergeable": gv(r, "mergeable"), "checks": gv(r, "checks"),
            "additions": gv(r, "additions"), "deletions": gv(r, "deletions"), "changed_files": gv(r, "changed_files"),
            "head_ref": gv(r, "head_ref"), "base_ref": gv(r, "base_ref"),
            "head_oid": gv(r, "head_oid"), "base_oid": gv(r, "base_oid"), "merge_commit_oid": gv(r, "merge_commit_oid"),
            "reviews": sorted(rows("gh_pr_reviews").iter().filter(|x| ni(x, "pr_number") == n).map(|x| json!({
                "id": gv(x, "id"), "reviewer_id": gv(x, "reviewer_id"), "state": gv(x, "state"), "body": gv(x, "body") })).collect()),
            "comments": sorted(rows("gh_comments").iter().filter(|x| is_subject(x, "pr", n)).map(|x| json!({
                "id": gv(x, "id"), "author_id": gv(x, "author_id"), "body": gv(x, "body") })).collect()),
            "review_comments": sorted(rows("gh_review_comments").iter().filter(|x| ni(x, "pr_number") == n).map(|x| json!({
                "id": gv(x, "id"), "author_id": gv(x, "author_id"), "path": gv(x, "path"), "line": gv(x, "line"),
                "side": gv(x, "side"), "commit_oid": gv(x, "commit_oid"), "body": gv(x, "body") })).collect()),
            "pr_commits": strs(rows("gh_pr_commits").iter().filter(|x| ni(x, "pr_number") == n).collect(), "commit_oid"),
            "requested_reviewers": sorted(rows("gh_requested_reviewers").iter().filter(|x| ni(x, "pr_number") == n).map(|x| gv(x, "user_id")).collect()),
            "labels": strs(rows("gh_labeled").iter().filter(|x| is_subject(x, "pr", n)).collect(), "label_name"),
        })
    }).collect());

    let issues = sorted(rows("gh_issues").iter().map(|r| {
        let n = ni(r, "number");
        json!({
            "number": gv(r, "number"), "state": gv(r, "state"), "author_id": gv(r, "author_id"), "title": gv(r, "title"), "body": gv(r, "body"),
            "comments": sorted(rows("gh_comments").iter().filter(|x| is_subject(x, "issue", n)).map(|x| json!({
                "id": gv(x, "id"), "author_id": gv(x, "author_id"), "body": gv(x, "body") })).collect()),
            "labels": strs(rows("gh_labeled").iter().filter(|x| is_subject(x, "issue", n)).collect(), "label_name"),
        })
    }).collect());

    let events = sorted(rows("gh_events").iter().map(|r| json!({
        "id": gv(r, "id"), "type": gv(r, "type"), "actor_id": gv(r, "actor_id") })).collect());

    json!({ "pulls": pulls, "issues": issues, "events": events })
}
