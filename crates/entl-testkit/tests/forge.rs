//! Forge round-trip: drive the *real* `ingest_github` against the mock GitHub server.

use std::collections::BTreeMap;
use std::sync::Mutex;

use entl_core::extract::{
    bool_columns, diff, extract_duckdb, extract_jsonl, extract_sqlite, FORGE_TABLES, GIT_TABLES,
};
use entl_core::{build_sink, pull_into, Db, PullOpts, SinkSelect, SinkTarget};
use entl_testkit::forge::*;
use entl_testkit::{
    arb_forge_world, arb_git_world, git, materialize, GenBlob, GenCommit, GenRef, GenSig, GitWorld,
    MockForge, Mode,
};
use proptest::prelude::*;
use serde_json::json;

/// `ENTL_GITHUB_API` is process-global, so serialize forge ingests.
static ENV: Mutex<()> = Mutex::new(());

fn one_commit_repo(dir: &std::path::Path) -> Vec<String> {
    let mut tree = BTreeMap::new();
    tree.insert("a.txt".to_string(), GenBlob { content: b"hi\n".to_vec(), mode: Mode::Normal });
    let sig = GenSig { name: "T".into(), email: "t@e.com".into(), time_secs: 1_600_000_000, tz: "+0000".into() };
    let w = GitWorld {
        commits: vec![GenCommit { parents: vec![], tree, author: sig.clone(), committer: sig, message: "c0\n".into() }],
        refs: vec![GenRef { name: "refs/heads/main".into(), target: 0 }],
    };
    let oids = materialize(&w, dir).unwrap();
    git(dir, &["remote", "add", "origin", "https://github.com/acme/widget.git"]).unwrap();
    oids
}

fn count(db: &Db, table: &str) -> i64 {
    db.conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0)).unwrap()
}

#[test]
fn mock_forge_ingests_through_real_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    let oids = one_commit_repo(dir.path());

    let world = ForgeWorld {
        owner: "acme".into(),
        name: "widget".into(),
        users: vec![
            GhUser { id: 1, login: "alice".into(), typ: "User".into() },
            GhUser { id: 2, login: "bob".into(), typ: "User".into() },
        ],
        labels: vec![GhLabel { name: "bug".into(), color: Some("f00".into()), description: Some("a bug".into()) }],
        pulls: vec![GhPull {
            number: 1,
            title: Some("Add feature".into()),
            body: Some("the body".into()),
            state: "OPEN".into(),
            is_draft: false,
            mergeable: Some("MERGEABLE".into()),
            created_at: "2020-01-01T00:00:00Z".into(),
            updated_at: "2020-01-02T00:00:00Z".into(),
            closed_at: None,
            merged_at: None,
            additions: Some(10),
            deletions: Some(2),
            changed_files: Some(1),
            head_ref: Some("feat".into()),
            base_ref: Some("main".into()),
            head_commit: Some(0),
            base_commit: Some(0),
            merge_commit: None,
            author: Some(0),
            rollup: Some("SUCCESS".into()),
            labels: vec![0],
            commits: vec![0],
            reviews: vec![GhReview { id: 100, state: Some("APPROVED".into()), submitted_at: Some("2020-01-01T12:00:00Z".into()), body: Some("lgtm".into()), author: Some(1) }],
            requested_reviewers: vec![1],
            comments: vec![GhComment { id: 200, body: Some("nice".into()), created_at: Some("2020-01-01T13:00:00Z".into()), author: Some(1) }],
            review_comments: vec![GhReviewComment { id: 300, path: Some("a.txt".into()), line: Some(1), side: Some("RIGHT".into()), commit: Some(0), body: Some("fix".into()), created_at: Some("2020-01-01T14:00:00Z".into()), reply_to: None, author: Some(1) }],
        }],
        issues: vec![GhIssue {
            number: 1,
            title: Some("Bug".into()),
            body: Some("broken".into()),
            state: "OPEN".into(),
            created_at: "2020-01-01T00:00:00Z".into(),
            updated_at: "2020-01-02T00:00:00Z".into(),
            closed_at: None,
            author: Some(0),
            labels: vec![0],
            comments: vec![GhComment { id: 400, body: Some("me too".into()), created_at: Some("2020-01-01T15:00:00Z".into()), author: Some(1) }],
        }],
        events: vec![GhEvent { id: "1000".into(), typ: Some("PushEvent".into()), actor: Some(0), created_at: Some("2020-01-03T00:00:00Z".into()), payload: json!({"size": 1}) }],
        workflows: vec![GhWorkflow { id: 500, name: "CI".into(), path: ".github/workflows/ci.yml".into(), state: "active".into() }],
        runs: vec![GhRun {
            id: 600, workflow_id: 500, head_commit: 0, head_branch: "main".into(), event: "push".into(),
            status: "completed".into(), conclusion: Some("success".into()), run_number: 1,
            jobs: vec![GhJob { id: 700, name: "build".into(), status: "completed".into(), conclusion: Some("success".into()), runner_name: Some("ubuntu".into()),
                steps: vec![GhStep { number: 1, name: "checkout".into(), status: "completed".into(), conclusion: Some("success".into()) }] }],
        }],
        checks: vec![GhCheck { id: 800, commit: 0, name: "lint".into(), conclusion: Some("success".into()) }],
        statuses: vec![GhStatus { id: 900, commit: 0, context: Some("ci/build".into()), state: "success".into(), description: Some("ok".into()), target_url: Some("https://x.example/s".into()) }],
    };

    let mock = MockForge::start();
    mock.serve(world, oids);

    let db = Db::open(":memory:").unwrap();
    db.migrate().unwrap();

    {
        let _g = ENV.lock().unwrap();
        std::env::set_var("ENTL_GITHUB_API", &mock.base_url);
        std::env::set_var("GH_TOKEN", "mock-token");
        entl_core::ingest_github(&db, dir.path().to_str().unwrap()).unwrap();
        std::env::remove_var("ENTL_GITHUB_API");
    }

    assert_eq!(count(&db, "gh_pull_requests"), 1, "prs");
    assert_eq!(count(&db, "gh_pr_reviews"), 1, "reviews");
    assert_eq!(count(&db, "gh_pr_commits"), 1, "pr commits");
    assert_eq!(count(&db, "gh_requested_reviewers"), 1, "requested reviewers");
    assert_eq!(count(&db, "gh_review_comments"), 1, "review comments");
    assert_eq!(count(&db, "gh_comments"), 2, "comments (pr + issue)");
    assert_eq!(count(&db, "gh_labeled"), 2, "labeled (pr + issue)");
    assert_eq!(count(&db, "gh_labels"), 1, "labels");
    assert_eq!(count(&db, "gh_issues"), 1, "issues");
    assert_eq!(count(&db, "gh_users"), 2, "users");
    assert_eq!(count(&db, "gh_events"), 1, "events");
    // Actions/Checks flow through the real ingest too.
    assert_eq!(count(&db, "gh_workflows"), 1, "workflows");
    assert_eq!(count(&db, "gh_workflow_runs"), 1, "workflow runs");
    assert_eq!(count(&db, "gh_jobs"), 1, "jobs");
    assert_eq!(count(&db, "gh_steps"), 1, "steps");
    assert_eq!(count(&db, "gh_check_runs"), 1, "check runs");
    assert_eq!(count(&db, "gh_commit_statuses"), 1, "commit statuses");

    // Schema guard: every gh_ table the ingest can write must be covered by the mock, so adding a
    // new forge table without extending the mock is caught here. (gh_assignees has no writer.)
    let gh_tables: Vec<String> = db
        .conn
        .prepare("SELECT table_name FROM information_schema.tables WHERE table_schema='main' AND table_name LIKE 'gh_%'")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for t in gh_tables {
        if t == "gh_assignees" {
            continue; // documented: DDL exists, no writer
        }
        assert!(count(&db, &t) > 0, "mock does not cover {t} (ingest writes it but it's empty)");
    }
}

fn all_tables() -> Vec<&'static str> {
    GIT_TABLES.iter().chain(FORGE_TABLES).copied().collect()
}

/// Ingest git + (mock) github into a fresh DuckDB + `sink`, under the env lock. Returns the DB.
fn pull_forge(repo: &str, base_url: &str, target: SinkTarget, dest: &str) -> Db {
    let db = Db::open(":memory:").unwrap();
    db.migrate().unwrap();
    let sink = build_sink(target, Some(dest), SinkSelect::default()).unwrap();
    let _g = ENV.lock().unwrap();
    std::env::set_var("ENTL_GITHUB_API", base_url);
    std::env::set_var("GH_TOKEN", "mock");
    pull_into(&db, repo, sink, PullOpts { github: true, objects: false }).unwrap();
    std::env::remove_var("ENTL_GITHUB_API");
    db
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 16, max_shrink_iters: 100, ..ProptestConfig::default() })]

    /// P3 — the forge flows through the real ingest (mock GitHub → ingest_github) and round-trips
    /// through every store, exactly as the git tables do.
    #[test]
    fn p3_forge_store_roundtrip(gitw in arb_git_world(), forge in arb_forge_world()) {
        let repo = tempfile::tempdir().unwrap();
        let oids = materialize(&gitw, repo.path()).unwrap();
        git(repo.path(), &["remote", "add", "origin", "https://github.com/acme/widget.git"]).unwrap();
        let repo_str = repo.path().to_str().unwrap();

        let mock = MockForge::start();
        mock.serve(forge.clone(), oids.clone());
        let tables = all_tables();

        // SQLite
        let sdir = tempfile::tempdir().unwrap();
        let spath = sdir.path().join("s.db");
        let spath = spath.to_str().unwrap();
        let db = pull_forge(repo_str, &mock.base_url, SinkTarget::Sqlite, spath);
        let s0 = extract_duckdb(&db.conn, &tables).unwrap();
        let bcols = bool_columns(&db.conn).unwrap();
        let s1 = extract_sqlite(spath, &tables, &bcols).unwrap();
        let d = diff(&s0, &s1);
        prop_assert!(d.is_empty(), "sqlite forge mismatch:\n{}", d);

        // Reassembly: reconstruct the fake forge from the stored gh_* tables and compare it to the
        // generated ForgeWorld (top-level entities, references resolved).
        let want = entl_testkit::reassemble::canonical_forge(&forge, &oids);
        let got = entl_testkit::reassemble::canonical_store(&s0);
        prop_assert_eq!(&want, &got, "forge reassembly mismatch");

        // JSONL
        let jdir = tempfile::tempdir().unwrap();
        let jpath = jdir.path().to_str().unwrap();
        let db2 = pull_forge(repo_str, &mock.base_url, SinkTarget::Jsonl, jpath);
        let s0b = extract_duckdb(&db2.conn, &tables).unwrap();
        let s1b = extract_jsonl(jpath, &tables).unwrap();
        let d2 = diff(&s0b, &s1b);
        prop_assert!(d2.is_empty(), "jsonl forge mismatch:\n{}", d2);
    }
}
