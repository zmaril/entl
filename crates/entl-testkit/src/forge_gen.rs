//! proptest generator for [`ForgeWorld`]. Produces valid, varied forge state with unique natural
//!
//! straitjacket-allow-file:duplication — proptest generator arms are parallel by shape.
//! keys (user ids, PR/issue numbers, review/comment ids) assigned by counters so the ingest's
//! primary keys never collide. Commit references are indices (the mock clamps them to real OIDs).

use proptest::prelude::*;
use serde_json::json;

use crate::forge::*;

const BASE_TS: i64 = 1_600_000_000;

fn rfc3339(secs: i64) -> String {
    chrono::DateTime::from_timestamp(secs, 0)
        .unwrap()
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string()
}

#[derive(Debug, Clone)]
struct RawPr {
    author: usize,
    state: u8,
    draft: bool,
    dt: u32,
    labels: Vec<usize>,
    commits: Vec<usize>,
    reviews: Vec<(usize, u8)>,
    reviewers: Vec<usize>,
    comments: Vec<usize>,
    rcomments: Vec<(usize, usize)>,
    rollup: bool,
}

#[derive(Debug, Clone)]
struct RawIssue {
    author: usize,
    open: bool,
    dt: u32,
    labels: Vec<usize>,
    comments: Vec<usize>,
}

fn arb_pr() -> impl Strategy<Value = RawPr> {
    (
        0usize..8,
        0u8..3,
        any::<bool>(),
        0u32..100_000,
        prop::collection::vec(0usize..8, 0..3),
        prop::collection::vec(0usize..8, 0..3),
        prop::collection::vec((0usize..8, 0u8..3), 0..3),
        prop::collection::vec(0usize..8, 0..2),
        prop::collection::vec(0usize..8, 0..3),
        prop::collection::vec((0usize..8, 0usize..8), 0..2),
        any::<bool>(),
    )
        .prop_map(
            |(
                author,
                state,
                draft,
                dt,
                labels,
                commits,
                reviews,
                reviewers,
                comments,
                rcomments,
                rollup,
            )| {
                RawPr {
                    author,
                    state,
                    draft,
                    dt,
                    labels,
                    commits,
                    reviews,
                    reviewers,
                    comments,
                    rcomments,
                    rollup,
                }
            },
        )
}

#[derive(Debug, Clone)]
struct RawRun {
    head: usize,
    jobs: Vec<usize>, // one entry per job = its step count
    check: bool,
    status: bool,
}

fn arb_run() -> impl Strategy<Value = RawRun> {
    (
        0usize..8,
        prop::collection::vec(0usize..4, 0..3),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(head, jobs, check, status)| RawRun {
            head,
            jobs,
            check,
            status,
        })
}

fn arb_issue() -> impl Strategy<Value = RawIssue> {
    (
        0usize..8,
        any::<bool>(),
        0u32..100_000,
        prop::collection::vec(0usize..8, 0..3),
        prop::collection::vec(0usize..8, 0..3),
    )
        .prop_map(|(author, open, dt, labels, comments)| RawIssue {
            author,
            open,
            dt,
            labels,
            comments,
        })
}

/// A valid random forge state: 1–4 users, up to 3 labels, up to 4 PRs / 4 issues (each with
/// reviews/comments/labels/commits), and up to 4 events.
pub fn arb_forge_world() -> impl Strategy<Value = ForgeWorld> {
    (
        1usize..5,
        0usize..4,
        prop::collection::vec(arb_pr(), 0..5),
        prop::collection::vec(arb_issue(), 0..5),
        0usize..5,
        0usize..3,
        prop::collection::vec(arb_run(), 0..3),
    )
        .prop_map(|(nu, nl, prs, issues, ne, nw, runs)| build(nu, nl, prs, issues, ne, nw, runs))
}

fn dedup(mut v: Vec<usize>, modulo: usize) -> Vec<usize> {
    if modulo == 0 {
        return Vec::new();
    }
    v.iter_mut().for_each(|x| *x %= modulo);
    v.sort_unstable();
    v.dedup();
    v
}

#[allow(clippy::too_many_arguments)]
fn build(
    nu: usize,
    nl: usize,
    prs: Vec<RawPr>,
    issues: Vec<RawIssue>,
    ne: usize,
    nw: usize,
    raw_runs: Vec<RawRun>,
) -> ForgeWorld {
    let users: Vec<GhUser> = (0..nu)
        .map(|i| GhUser {
            id: (i + 1) as i64,
            login: format!("u{i}"),
            typ: ["User", "Bot", "Organization"][i % 3].to_string(),
        })
        .collect();
    let labels: Vec<GhLabel> = (0..nl)
        .map(|i| GhLabel {
            name: format!("lbl{i}"),
            color: Some(format!("{:06x}", i * 0x111111)),
            description: Some(format!("label {i}")),
        })
        .collect();

    let mut id = 1000i64; // unique review/comment ids
    let mut next = || {
        id += 1;
        id
    };

    let pulls: Vec<GhPull> = prs
        .into_iter()
        .enumerate()
        .map(|(k, p)| {
            let number = (k + 1) as i64;
            let created = BASE_TS + (k as i64) * 1000;
            let updated = created + (p.dt as i64 % 10_000) + 1;
            let (state, closed_at, merged_at, merge_commit) = match p.state {
                0 => ("OPEN".to_string(), None, None, None),
                1 => ("CLOSED".to_string(), Some(rfc3339(updated)), None, None),
                _ => (
                    "MERGED".to_string(),
                    Some(rfc3339(updated)),
                    Some(rfc3339(updated)),
                    Some(0usize),
                ),
            };
            GhPull {
                number,
                title: Some(format!("PR {number}")),
                body: Some(format!("body {number}")),
                state,
                is_draft: p.draft,
                mergeable: Some("MERGEABLE".to_string()),
                created_at: rfc3339(created),
                updated_at: rfc3339(updated),
                closed_at,
                merged_at,
                additions: Some(k as i64 + 1),
                deletions: Some(k as i64),
                changed_files: Some(1),
                head_ref: Some(format!("feat{number}")),
                base_ref: Some("main".to_string()),
                head_commit: Some(0),
                base_commit: Some(1),
                merge_commit,
                author: (nu > 0).then_some(p.author % nu),
                rollup: p.rollup.then(|| "SUCCESS".to_string()),
                labels: dedup(p.labels, nl),
                commits: {
                    let mut c = p.commits.clone();
                    c.sort_unstable();
                    c.dedup();
                    c
                },
                reviews: p
                    .reviews
                    .into_iter()
                    .map(|(a, st)| GhReview {
                        id: next(),
                        state: Some(
                            ["APPROVED", "CHANGES_REQUESTED", "COMMENTED"][st as usize % 3]
                                .to_string(),
                        ),
                        submitted_at: Some(rfc3339(updated)),
                        body: Some("review".to_string()),
                        author: (nu > 0).then_some(a % nu),
                    })
                    .collect(),
                requested_reviewers: dedup(p.reviewers, nu),
                comments: p
                    .comments
                    .into_iter()
                    .map(|a| GhComment {
                        id: next(),
                        body: Some("comment".to_string()),
                        created_at: Some(rfc3339(updated)),
                        author: (nu > 0).then_some(a % nu),
                    })
                    .collect(),
                review_comments: p
                    .rcomments
                    .into_iter()
                    .map(|(a, c)| GhReviewComment {
                        id: next(),
                        path: Some("a.txt".to_string()),
                        line: Some(1),
                        side: Some("RIGHT".to_string()),
                        commit: Some(c),
                        body: Some("rc".to_string()),
                        created_at: Some(rfc3339(updated)),
                        reply_to: None,
                        author: (nu > 0).then_some(a % nu),
                    })
                    .collect(),
            }
        })
        .collect();

    let issues: Vec<GhIssue> = issues
        .into_iter()
        .enumerate()
        .map(|(k, is)| {
            let number = (k + 1) as i64;
            let created = BASE_TS + (k as i64) * 1000;
            let updated = created + (is.dt as i64 % 10_000) + 1;
            GhIssue {
                number,
                title: Some(format!("Issue {number}")),
                body: Some(format!("ibody {number}")),
                state: if is.open {
                    "OPEN".into()
                } else {
                    "CLOSED".into()
                },
                created_at: rfc3339(created),
                updated_at: rfc3339(updated),
                closed_at: (!is.open).then(|| rfc3339(updated)),
                author: (nu > 0).then_some(is.author % nu),
                labels: dedup(is.labels, nl),
                comments: is
                    .comments
                    .into_iter()
                    .map(|a| GhComment {
                        id: next(),
                        body: Some("icomment".to_string()),
                        created_at: Some(rfc3339(updated)),
                        author: (nu > 0).then_some(a % nu),
                    })
                    .collect(),
            }
        })
        .collect();

    let events: Vec<GhEvent> = (0..ne)
        .map(|i| GhEvent {
            id: format!("ev{i}"),
            typ: Some("PushEvent".to_string()),
            actor: (nu > 0).then_some(i % nu),
            created_at: Some(rfc3339(BASE_TS + 5000 + i as i64)),
            payload: json!({ "seq": i }),
        })
        .collect();

    let workflows: Vec<GhWorkflow> = (0..nw)
        .map(|i| GhWorkflow {
            id: next(),
            name: format!("wf{i}"),
            path: format!(".github/workflows/{i}.yml"),
            state: "active".into(),
        })
        .collect();

    let (mut runs, mut checks, mut statuses) = (Vec::new(), Vec::new(), Vec::new());
    for (k, rr) in raw_runs.into_iter().enumerate() {
        let jobs: Vec<GhJob> = rr
            .jobs
            .iter()
            .map(|&nsteps| GhJob {
                id: next(),
                name: "build".into(),
                status: "completed".into(),
                conclusion: Some("success".into()),
                runner_name: Some("ubuntu".into()),
                steps: (0..nsteps)
                    .map(|n| GhStep {
                        number: (n + 1) as i64,
                        name: format!("step{n}"),
                        status: "completed".into(),
                        conclusion: Some("success".into()),
                    })
                    .collect(),
            })
            .collect();
        let workflow_id = if workflows.is_empty() {
            next()
        } else {
            workflows[k % workflows.len()].id
        };
        runs.push(GhRun {
            id: next(),
            workflow_id,
            head_commit: rr.head,
            head_branch: "main".into(),
            event: "push".into(),
            status: "completed".into(),
            conclusion: Some("success".into()),
            run_number: (k + 1) as i64,
            jobs,
        });
        // Checks/statuses live on a run's head commit so the ingest (which only fetches them for
        // run head shas) actually pulls them.
        if rr.check {
            checks.push(GhCheck {
                id: next(),
                commit: rr.head,
                name: "ci".into(),
                conclusion: Some("success".into()),
            });
        }
        if rr.status {
            statuses.push(GhStatus {
                id: next(),
                commit: rr.head,
                context: Some("ci".into()),
                state: "success".into(),
                description: Some("ok".into()),
                target_url: Some("https://x.example/s".into()),
            });
        }
    }

    ForgeWorld {
        owner: "acme".into(),
        name: "widget".into(),
        users,
        labels,
        pulls,
        issues,
        events,
        workflows,
        runs,
        checks,
        statuses,
    }
}
