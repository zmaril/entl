//! A localhost mock GitHub server — serves GraphQL + REST shaped from a [`ForgeWorld`], the exact
//! inverse of the ingest's parse (`entl-core/src/github/`). Point the ingest at it with
//! `ENTL_GITHUB_API=<base_url>` and the *real* `ingest_github` runs end-to-end against it.
//!
//! Only the endpoints the ingest actually hits are implemented: `POST /graphql` (PR + issue
//! queries), the events feed, and trivial gate/actions responses (empty workflows/runs, so no
//! jobs/checks are fetched). See notes/design/testing.md.

use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::{json, Value};
use tiny_http::{Header, Method, Response, Server};

use crate::forge::ForgeWorld;

/// The forge state currently being served, with git commit indices resolved to real OIDs.
#[derive(Default)]
struct Served {
    world: ForgeWorld,
    oids: Vec<String>,
}

/// A running mock GitHub API. Drop to stop it.
pub struct MockForge {
    pub base_url: String,
    state: Arc<Mutex<Served>>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl MockForge {
    /// Bind a random localhost port and start serving (empty until [`serve`](Self::serve)).
    pub fn start() -> Self {
        let server = Arc::new(Server::http("127.0.0.1:0").expect("bind mock server"));
        let port = server.server_addr().to_ip().unwrap().port();
        let state = Arc::new(Mutex::new(Served::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (srv, st, stp) = (server.clone(), state.clone(), stop.clone());
        let handle = std::thread::spawn(move || {
            while !stp.load(Ordering::Relaxed) {
                match srv.recv_timeout(Duration::from_millis(100)) {
                    Ok(Some(req)) => handle_req(req, &st),
                    Ok(None) => {}
                    Err(_) => break,
                }
            }
        });
        MockForge {
            base_url: format!("http://127.0.0.1:{port}"),
            state,
            stop,
            handle: Some(handle),
        }
    }

    /// Serve `world`, resolving its commit indices against `oids` (from materialize).
    pub fn serve(&self, world: ForgeWorld, oids: Vec<String>) {
        *self.state.lock().unwrap() = Served { world, oids };
    }
}

impl Drop for MockForge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn handle_req(mut req: tiny_http::Request, state: &Arc<Mutex<Served>>) {
    let method = req.method().clone();
    let url = req.url().to_string();
    let mut body = String::new();
    let _ = req.as_reader().read_to_string(&mut body);
    let served = state.lock().unwrap();
    let payload = route(&method, &url, &body, &served);
    let resp = Response::from_string(payload.to_string())
        .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap())
        .with_header(Header::from_bytes(&b"ETag"[..], &b"\"entl-mock\""[..]).unwrap());
    let _ = req.respond(resp);
}

fn route(method: &Method, url: &str, body: &str, s: &Served) -> Value {
    let path = url.split('?').next().unwrap_or(url);
    match method {
        Method::Post if path.contains("graphql") => {
            if body.contains("pullRequests") {
                json!({ "data": pr_data(s) })
            } else {
                json!({ "data": issue_data(s) })
            }
        }
        Method::Get if path.ends_with("/events") => {
            // The gate (per_page=1) ignores the body; the feed reads page 1 then stops on empty.
            if page(url) >= 2 {
                json!([])
            } else {
                events(s)
            }
        }
        Method::Get if path.ends_with("/actions/workflows") => json!({ "total_count": 0, "workflows": [] }),
        Method::Get if path.ends_with("/actions/runs") => json!({ "total_count": 0, "workflow_runs": [] }),
        // pulls/issues REST are only used as etag gates (body ignored).
        _ => json!([]),
    }
}

fn page(url: &str) -> u32 {
    url.split(['?', '&'])
        .find_map(|kv| kv.strip_prefix("page="))
        .and_then(|p| p.parse().ok())
        .unwrap_or(1)
}

// ---- GraphQL serialization (matches the DTOs in entl-core/src/github/graphql.rs) ----

fn pr_data(s: &Served) -> Value {
    json!({ "repository": { "pullRequests": {
        "pageInfo": { "hasNextPage": false, "endCursor": Value::Null },
        "nodes": s.world.pulls.iter().map(|p| pr_node(p, s)).collect::<Vec<_>>(),
    }}})
}

fn pr_node(p: &crate::forge::GhPull, s: &Served) -> Value {
    let rollup_nodes = match &p.rollup {
        Some(state) => vec![json!({ "commit": { "statusCheckRollup": { "state": state } } })],
        None => vec![],
    };
    json!({
        "number": p.number, "title": p.title, "body": p.body, "state": p.state,
        "isDraft": p.is_draft, "mergeable": p.mergeable,
        "createdAt": p.created_at, "updatedAt": p.updated_at, "closedAt": p.closed_at, "mergedAt": p.merged_at,
        "additions": p.additions, "deletions": p.deletions, "changedFiles": p.changed_files,
        "headRefName": p.head_ref, "baseRefName": p.base_ref,
        "headRefOid": oid_opt(p.head_commit, s), "baseRefOid": oid_opt(p.base_commit, s),
        "author": actor(p.author, s),
        "mergeCommit": p.merge_commit.map(|i| json!({ "oid": oid(i, s) })),
        "rollup": { "nodes": rollup_nodes },
        "labels": { "nodes": p.labels.iter().map(|&i| label(i, s)).collect::<Vec<_>>() },
        "commits": { "totalCount": p.commits.len(),
                     "nodes": p.commits.iter().map(|&i| json!({ "commit": { "oid": oid(i, s) } })).collect::<Vec<_>>() },
        "reviews": { "totalCount": p.reviews.len(),
                     "nodes": p.reviews.iter().map(|r| json!({
                         "databaseId": r.id, "state": r.state, "submittedAt": r.submitted_at,
                         "body": r.body, "author": actor(r.author, s) })).collect::<Vec<_>>() },
        "reviewRequests": { "nodes": p.requested_reviewers.iter()
            .map(|&i| json!({ "requestedReviewer": actor(Some(i), s) })).collect::<Vec<_>>() },
        "comments": { "totalCount": p.comments.len(),
                      "nodes": p.comments.iter().map(|c| comment(c, s)).collect::<Vec<_>>() },
        "reviewThreads": { "nodes": p.review_comments.iter().map(|rc| json!({
            "diffSide": rc.side,
            "comments": { "nodes": [json!({
                "databaseId": rc.id, "path": rc.path, "line": rc.line, "originalLine": Value::Null,
                "commit": rc.commit.map(|i| json!({ "oid": oid(i, s) })),
                "body": rc.body, "createdAt": rc.created_at,
                "replyTo": rc.reply_to.map(|id| json!({ "databaseId": id })),
                "author": actor(rc.author, s),
            })] },
        })).collect::<Vec<_>>() },
    })
}

fn issue_data(s: &Served) -> Value {
    json!({ "repository": { "issues": {
        "pageInfo": { "hasNextPage": false, "endCursor": Value::Null },
        "nodes": s.world.issues.iter().map(|is| json!({
            "number": is.number, "title": is.title, "body": is.body, "state": is.state,
            "createdAt": is.created_at, "updatedAt": is.updated_at, "closedAt": is.closed_at,
            "author": actor(is.author, s),
            "labels": { "nodes": is.labels.iter().map(|&i| label(i, s)).collect::<Vec<_>>() },
            "comments": { "totalCount": is.comments.len(),
                          "nodes": is.comments.iter().map(|c| comment(c, s)).collect::<Vec<_>>() },
        })).collect::<Vec<_>>(),
    }}})
}

fn events(s: &Served) -> Value {
    json!(s.world.events.iter().map(|e| json!({
        "id": e.id, "type": e.typ,
        "actor": e.actor.and_then(|i| s.world.users.get(i)).map(|u| json!({ "id": u.id, "login": u.login })),
        "created_at": e.created_at, "payload": e.payload,
    })).collect::<Vec<_>>())
}

fn comment(c: &crate::forge::GhComment, s: &Served) -> Value {
    json!({ "databaseId": c.id, "body": c.body, "createdAt": c.created_at, "author": actor(c.author, s) })
}

fn label(i: usize, s: &Served) -> Value {
    match s.world.labels.get(i) {
        Some(l) => json!({ "name": l.name, "color": l.color, "description": l.description }),
        None => json!({ "name": format!("l{i}"), "color": Value::Null, "description": Value::Null }),
    }
}

/// An Actor object, or null.
fn actor(idx: Option<usize>, s: &Served) -> Value {
    match idx.and_then(|i| s.world.users.get(i)) {
        Some(u) => json!({ "login": u.login, "__typename": u.typ, "databaseId": u.id }),
        None => Value::Null,
    }
}

fn oid(i: usize, s: &Served) -> String {
    if s.oids.is_empty() {
        String::new()
    } else {
        s.oids[i % s.oids.len()].clone()
    }
}

fn oid_opt(idx: Option<usize>, s: &Served) -> Value {
    match idx {
        Some(i) => Value::String(oid(i, s)),
        None => Value::Null,
    }
}
