//! GitHub → DuckDB ingest (notes/design/engine.md).
//!
//! Transport: octocrab. GraphQL batches the PR graph + issues (each PR node
//! carries reviews/commits/comments inline); REST drives Actions/CI (no GraphQL
//! API). Repo identity (`repo_id`) is the same path-hash the git side uses, so
//! GitHub rows join straight to `commits`.

use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use duckdb::params;
use duckdb::types::{TimeUnit, Value};

use crate::db::Db;
use crate::ingest::compute_repo_id;
use crate::stream::{ChangeBatch, ChangeOp, ChangeSink};
use duckdb::arrow::record_batch::RecordBatch;

mod actions;
mod checks;
mod events;
mod graphql;
use graphql::{Actor, IssueNode, PrNode};

/// Concurrent in-flight REST requests (jobs per run, checks per commit). Kept
/// modest to stay clear of GitHub's secondary rate limits.
const CONCURRENCY: usize = 8;

/// Serialize an octocrab string-enum (Status/Conclusion/StatusState) to its wire string.
fn enum_str<T: serde::Serialize>(v: &T) -> Option<String> {
    serde_json::to_value(v)
        .ok()
        .and_then(|j| j.as_str().map(|s| s.to_string()))
}

#[derive(Debug, Default)]
pub struct GithubIngest {
    pub pull_requests: usize,
    pub reviews: usize,
    pub review_comments: usize,
    pub issues: usize,
    pub comments: usize,
    pub workflow_runs: usize,
    pub check_runs: usize,
    pub users: usize,
    pub events: usize,
    pub truncated: usize,
}

/// Accumulated GitHub users (id -> login, type), deduped across all references
/// and upserted into `gh_users` at the end.
type Users = HashMap<i64, (String, Option<String>)>;
/// Accumulated label defs (name -> color, description), upserted at the end.
type Labels = HashMap<String, (Option<String>, Option<String>)>;

/// Record an actor and return its `gh_users.id` (None if no numeric id).
fn record(users: &mut Users, actor: &Option<Actor>) -> Option<i64> {
    let a = actor.as_ref()?;
    let id = a.database_id?;
    users
        .entry(id)
        .or_insert_with(|| (a.login.clone().unwrap_or_default(), a.typename.clone()));
    Some(id)
}

/// Record label defs + write `labeled` edges for one subject ('pr' | 'issue').
fn write_labeled(
    app: &mut duckdb::Appender,
    labels: &mut Labels,
    repo_id: &str,
    subject_type: &str,
    number: i64,
    nodes: &[graphql::LabelNode],
) -> Result<()> {
    let mut seen = HashSet::new();
    for l in nodes {
        labels
            .entry(l.name.clone())
            .or_insert_with(|| (l.color.clone(), l.description.clone()));
        if seen.insert(l.name.as_str()) {
            app.append_row(params![repo_id, subject_type, number, l.name])?;
        }
    }
    Ok(())
}

/// chrono timestamp -> DuckDB microsecond TIMESTAMP (UTC), or NULL.
fn ts(d: Option<DateTime<Utc>>) -> Value {
    match d {
        Some(d) => Value::Timestamp(TimeUnit::Microsecond, d.timestamp_micros()),
        None => Value::Null,
    }
}

/// Parse a hex sha into a git oid (for BLOB commit_oid columns).
fn oid(s: &str) -> Option<gix::ObjectId> {
    gix::ObjectId::from_hex(s.as_bytes()).ok()
}

/// Incremental watermark (the most-recent `updated_at` synced for a resource).
fn read_watermark(db: &Db, resource: &str) -> Result<Option<DateTime<Utc>>> {
    let r: duckdb::Result<i64> = db.conn.query_row(
        "SELECT epoch_us(watermark) FROM sync_state WHERE resource = ? AND watermark IS NOT NULL",
        params![resource],
        |row| row.get(0),
    );
    match r {
        Ok(us) => Ok(DateTime::from_timestamp_micros(us)),
        Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn write_watermark(db: &Db, resource: &str, wm: DateTime<Utc>) -> Result<()> {
    db.conn.execute(
        "INSERT INTO sync_state (resource, watermark, last_synced_at)
         VALUES (?, ?, now()::TIMESTAMP)
         ON CONFLICT (resource) DO UPDATE SET
           watermark = excluded.watermark, last_synced_at = excluded.last_synced_at",
        params![resource, Value::Timestamp(TimeUnit::Microsecond, wm.timestamp_micros())],
    )?;
    Ok(())
}

fn read_etag(db: &Db, resource: &str) -> Result<Option<String>> {
    let r: duckdb::Result<String> = db.conn.query_row(
        "SELECT etag FROM sync_state WHERE resource = ? AND etag IS NOT NULL",
        params![resource],
        |row| row.get(0),
    );
    match r {
        Ok(s) => Ok(Some(s)),
        Err(duckdb::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn write_etag(db: &Db, resource: &str, etag: &str) -> Result<()> {
    db.conn.execute(
        "INSERT INTO sync_state (resource, etag, last_synced_at)
         VALUES (?, ?, now()::TIMESTAMP)
         ON CONFLICT (resource) DO UPDATE SET
           etag = excluded.etag, last_synced_at = excluded.last_synced_at",
        params![resource, etag],
    )?;
    Ok(())
}

/// Conditional-request gate: a cheap REST probe (sorted `per_page=1`) whose ETag
/// changes iff the resource changed. A `304 Not Modified` costs no rate budget and
/// means "skip the expensive sync". Returns `(changed, new_etag)` — on 304,
/// `changed=false`; on 200, `changed=true` + the fresh ETag to store. Any failure
/// is treated as "changed" so we never skip a sync we should have run.
async fn etag_gate(
    client: &octocrab::Octocrab,
    url: &str,
    prev_etag: Option<&str>,
) -> (bool, Option<String>) {
    use http::header::{HeaderMap, HeaderValue, ETAG, IF_NONE_MATCH};
    let mut headers = HeaderMap::new();
    if let Some(e) = prev_etag {
        if let Ok(v) = HeaderValue::from_str(e) {
            headers.insert(IF_NONE_MATCH, v);
        }
    }
    match client._get_with_headers(url, Some(headers)).await {
        Ok(resp) => {
            if resp.status() == http::StatusCode::NOT_MODIFIED {
                (false, None)
            } else {
                let etag = resp
                    .headers()
                    .get(ETAG)
                    .and_then(|v| v.to_str().ok())
                    .map(String::from);
                (true, etag)
            }
        }
        Err(_) => (true, None), // network/parse trouble → don't skip
    }
}

/// Remove all rows for one PR (incremental replace before re-insert).
fn delete_pr_rows(db: &Db, repo_id: &str, n: i64) -> Result<()> {
    db.conn
        .execute("DELETE FROM gh_pull_requests WHERE repo_id = ? AND number = ?", params![repo_id, n])?;
    for t in ["gh_pr_reviews", "gh_pr_commits", "gh_requested_reviewers", "gh_review_comments"] {
        db.conn.execute(
            &format!("DELETE FROM {t} WHERE repo_id = ? AND pr_number = ?"),
            params![repo_id, n],
        )?;
    }
    for t in ["gh_comments", "gh_labeled"] {
        db.conn.execute(
            &format!("DELETE FROM {t} WHERE repo_id = ? AND subject_type = 'pr' AND subject_number = ?"),
            params![repo_id, n],
        )?;
    }
    Ok(())
}

/// Remove all rows for one issue.
fn delete_issue_rows(db: &Db, repo_id: &str, n: i64) -> Result<()> {
    db.conn
        .execute("DELETE FROM gh_issues WHERE repo_id = ? AND number = ?", params![repo_id, n])?;
    for t in ["gh_comments", "gh_labeled"] {
        db.conn.execute(
            &format!("DELETE FROM {t} WHERE repo_id = ? AND subject_type = 'issue' AND subject_number = ?"),
            params![repo_id, n],
        )?;
    }
    Ok(())
}

/// (owner, repo) from a github remote URL, or None if it isn't a github remote.
fn parse_owner_repo(url: &str) -> Option<(String, String)> {
    let s = url.trim();
    let rest = s
        .strip_prefix("git@github.com:")
        .or_else(|| s.strip_prefix("https://github.com/"))
        .or_else(|| s.strip_prefix("ssh://git@github.com/"))
        .or_else(|| s.strip_prefix("git://github.com/"))?;
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let mut it = rest.splitn(2, '/');
    let owner = it.next()?.to_string();
    let repo = it.next()?.trim_end_matches('/').to_string();
    (!owner.is_empty() && !repo.is_empty()).then_some((owner, repo))
}

/// Resolve owner/repo + the path-derived `repo_id` (shared with the git side).
/// `None` if the repo has no github.com remote (a non-GitHub repo → skip, not error).
fn resolve_repo(path: &str) -> Result<Option<(String, String, String)>> {
    let repo = gix::discover(path).context("discover git repo")?;
    let (repo_id, _canon) = compute_repo_id(&repo);
    let Some(url) = repo
        .config_snapshot()
        .string("remote.origin.url")
        .map(|s| s.to_string())
    else {
        return Ok(None);
    };
    Ok(parse_owner_repo(&url).map(|(owner, name)| (owner, name, repo_id)))
}

/// The GitHub API base URL. `ENTL_GITHUB_API` overrides it (e.g. a localhost mock in the
/// round-trip tests; see notes/design/testing.md); defaults to the real API.
fn api_base() -> String {
    std::env::var("ENTL_GITHUB_API").unwrap_or_else(|_| "https://api.github.com".to_string())
}

/// Token from `gh auth token`, then `GH_TOKEN` / `GITHUB_TOKEN`.
fn resolve_token() -> Result<String> {
    if let Ok(out) = std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
    {
        if out.status.success() {
            let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !t.is_empty() {
                return Ok(t);
            }
        }
    }
    for var in ["GH_TOKEN", "GITHUB_TOKEN"] {
        if let Ok(t) = std::env::var(var) {
            if !t.is_empty() {
                return Ok(t);
            }
        }
    }
    Err(anyhow!("no GitHub token: run `gh auth login` or set GH_TOKEN"))
}

/// Ingest GitHub data for the repo at `path` into `db`.
pub fn ingest_github(db: &Db, path: &str) -> Result<GithubIngest> {
    let Some((owner, name, repo_id)) = resolve_repo(path)? else {
        eprintln!("github: no github.com remote — skipping");
        return Ok(GithubIngest::default());
    };
    let token = resolve_token()?;
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    rt.block_on(ingest_async(db, &owner, &name, &repo_id, token))
}

/// Ingest GitHub data, teeing the rows that changed this cycle into `sink` as
/// Arrow change batches (notes/design/engine.md, "The change stream").
///
/// Emitted post-hoc from the store (no threading through the async sync): the
/// top-level resources — `gh_pull_requests`/`gh_issues` (upsert, by `updated_at`
/// watermark), `gh_events` (by `created_at`; the `id` is text, not orderable),
/// `gh_workflow_runs`/`gh_check_runs` (bounded, cleared+refetched → replace) —
/// plus the sub-resources of the PRs/issues that changed (reviews, commits,
/// comments, labeled, review-comments, requested-reviewers) and the
/// `gh_users`/`gh_labels` dims. Follow-ups: the Actions children
/// (`gh_jobs`/`gh_steps`/…) and per-parent *delete* of removed sub-rows.
pub fn ingest_github_streamed(
    db: &Db,
    path: &str,
    sink: Option<&ChangeSink>,
) -> Result<GithubIngest> {
    let Some(sink) = sink else {
        return ingest_github(db, path);
    };
    let Some((_, _, repo_id)) = resolve_repo(path)? else {
        return ingest_github(db, path); // no github remote → normal (no-op) path
    };

    // Snapshot the deltas' lower bounds *before* the sync mutates the store.
    let pr_wm = read_watermark(db, &format!("gh:prs:{repo_id}"))?;
    let is_wm = read_watermark(db, &format!("gh:issues:{repo_id}"))?;
    let ev_wm: Option<DateTime<Utc>> = {
        let us: Option<i64> = db.conn.query_row(
            "SELECT epoch_us(max(created_at)) FROM gh_events WHERE repo_id = ?",
            params![repo_id],
            |r| r.get(0),
        )?;
        us.and_then(DateTime::from_timestamp_micros)
    };

    let stats = ingest_github(db, path)?;

    // Top-level resources.
    emit_delta(db, sink, "gh_pull_requests", ChangeOp::Upsert, &repo_id, "updated_at", pr_wm)?;
    emit_delta(db, sink, "gh_issues", ChangeOp::Upsert, &repo_id, "updated_at", is_wm)?;
    emit_delta(db, sink, "gh_events", ChangeOp::Insert, &repo_id, "created_at", ev_wm)?;
    emit_all(db, sink, "gh_workflow_runs", ChangeOp::Replace, &repo_id)?;
    emit_all(db, sink, "gh_check_runs", ChangeOp::Replace, &repo_id)?;

    // Sub-resources of the PRs/issues that changed this cycle. Upserted by natural
    // key; per-parent *replace* (dropping sub-rows a PR no longer has, e.g. a
    // deleted review) is a follow-up — the ingest deletes+reinserts, but this
    // delta doesn't yet emit the deletions.
    let changed_prs = changed_numbers(db, "gh_pull_requests", &repo_id, pr_wm)?;
    let changed_issues = changed_numbers(db, "gh_issues", &repo_id, is_wm)?;
    for table in ["gh_pr_reviews", "gh_pr_commits", "gh_requested_reviewers", "gh_review_comments"] {
        emit_keys(db, sink, table, "pr_number", ChangeOp::Upsert, &repo_id, &changed_prs)?;
    }
    for table in ["gh_comments", "gh_labeled"] {
        emit_subject(db, sink, table, ChangeOp::Upsert, &repo_id, "pr", &changed_prs)?;
        emit_subject(db, sink, table, ChangeOp::Upsert, &repo_id, "issue", &changed_issues)?;
    }
    // Dim tables the above reference. `gh_users` is global (no repo_id); both are
    // sent whole (not yet delta-optimized) so a sink always has the referents.
    emit_all_global(db, sink, "gh_users", ChangeOp::Upsert)?;
    emit_all(db, sink, "gh_labels", ChangeOp::Upsert, &repo_id)?;

    Ok(stats)
}

/// Send each non-empty batch; stop early if the consumer has hung up.
fn emit_batches(sink: &ChangeSink, table: &str, op: ChangeOp, batches: Vec<RecordBatch>) {
    for b in batches {
        if b.num_rows() > 0 && !sink.emit(ChangeBatch::new(table, op, b)) {
            break;
        }
    }
}

/// Emit rows whose `col` advanced past the pre-sync watermark (all rows if none).
fn emit_delta(
    db: &Db,
    sink: &ChangeSink,
    table: &str,
    op: ChangeOp,
    repo_id: &str,
    col: &str,
    wm: Option<DateTime<Utc>>,
) -> Result<()> {
    let batches: Vec<RecordBatch> = if let Some(w) = wm {
        let sql = format!("SELECT * FROM {table} WHERE repo_id = ? AND {col} > ?");
        let mut stmt = db.conn.prepare(&sql)?;
        stmt.query_arrow(params![
            repo_id,
            Value::Timestamp(TimeUnit::Microsecond, w.timestamp_micros())
        ])?
        .collect()
    } else {
        let sql = format!("SELECT * FROM {table} WHERE repo_id = ?");
        let mut stmt = db.conn.prepare(&sql)?;
        stmt.query_arrow(params![repo_id])?.collect()
    };
    emit_batches(sink, table, op, batches);
    Ok(())
}

/// Emit the whole current set of a (bounded) table for this repo.
fn emit_all(db: &Db, sink: &ChangeSink, table: &str, op: ChangeOp, repo_id: &str) -> Result<()> {
    let sql = format!("SELECT * FROM {table} WHERE repo_id = ?");
    let mut stmt = db.conn.prepare(&sql)?;
    let batches: Vec<RecordBatch> = stmt.query_arrow(params![repo_id])?.collect();
    emit_batches(sink, table, op, batches);
    Ok(())
}

/// Emit a whole table that has no `repo_id` (global dim tables like `gh_users`).
fn emit_all_global(db: &Db, sink: &ChangeSink, table: &str, op: ChangeOp) -> Result<()> {
    let mut stmt = db.conn.prepare(&format!("SELECT * FROM {table}"))?;
    let batches: Vec<RecordBatch> = stmt.query_arrow([])?.collect();
    emit_batches(sink, table, op, batches);
    Ok(())
}

/// The PR/issue `number`s whose watermark advanced (all of them if there's no
/// prior watermark) — the parents whose sub-rows we (re)emit this cycle.
fn changed_numbers(
    db: &Db,
    table: &str,
    repo_id: &str,
    wm: Option<DateTime<Utc>>,
) -> Result<Vec<i32>> {
    let out = if let Some(w) = wm {
        let mut stmt = db.conn.prepare(&format!(
            "SELECT number FROM {table} WHERE repo_id = ? AND updated_at > ?"
        ))?;
        stmt.query_map(
            params![repo_id, Value::Timestamp(TimeUnit::Microsecond, w.timestamp_micros())],
            |r| r.get::<_, i32>(0),
        )?
        .collect::<duckdb::Result<Vec<i32>>>()?
    } else {
        let mut stmt = db
            .conn
            .prepare(&format!("SELECT number FROM {table} WHERE repo_id = ?"))?;
        stmt.query_map(params![repo_id], |r| r.get::<_, i32>(0))?
            .collect::<duckdb::Result<Vec<i32>>>()?
    };
    Ok(out)
}

/// Emit a sub-table's rows for `key_col IN numbers` (chunked to bound the query).
fn emit_keys(
    db: &Db,
    sink: &ChangeSink,
    table: &str,
    key_col: &str,
    op: ChangeOp,
    repo_id: &str,
    numbers: &[i32],
) -> Result<()> {
    for chunk in numbers.chunks(500) {
        let ph = std::iter::repeat("?").take(chunk.len()).collect::<Vec<_>>().join(", ");
        let sql = format!("SELECT * FROM {table} WHERE repo_id = ? AND {key_col} IN ({ph})");
        let mut stmt = db.conn.prepare(&sql)?;
        let mut p: Vec<&dyn duckdb::ToSql> = Vec::with_capacity(chunk.len() + 1);
        p.push(&repo_id);
        for n in chunk {
            p.push(n);
        }
        let batches: Vec<RecordBatch> = stmt.query_arrow(p.as_slice())?.collect();
        emit_batches(sink, table, op, batches);
    }
    Ok(())
}

/// Emit a subject-keyed table (`gh_comments` / `gh_labeled`) for one subject type.
fn emit_subject(
    db: &Db,
    sink: &ChangeSink,
    table: &str,
    op: ChangeOp,
    repo_id: &str,
    subject_type: &str,
    numbers: &[i32],
) -> Result<()> {
    for chunk in numbers.chunks(500) {
        let ph = std::iter::repeat("?").take(chunk.len()).collect::<Vec<_>>().join(", ");
        let sql = format!(
            "SELECT * FROM {table} WHERE repo_id = ? AND subject_type = ? AND subject_number IN ({ph})"
        );
        let mut stmt = db.conn.prepare(&sql)?;
        let mut p: Vec<&dyn duckdb::ToSql> = Vec::with_capacity(chunk.len() + 2);
        p.push(&repo_id);
        p.push(&subject_type);
        for n in chunk {
            p.push(n);
        }
        let batches: Vec<RecordBatch> = stmt.query_arrow(p.as_slice())?.collect();
        emit_batches(sink, table, op, batches);
    }
    Ok(())
}

async fn ingest_async(
    db: &Db,
    owner: &str,
    name: &str,
    repo_id: &str,
    token: String,
) -> Result<GithubIngest> {
    let base = api_base();
    let client = octocrab::Octocrab::builder()
        .personal_token(token)
        .base_uri(base.clone())
        .context("set github api base")?
        .build()
        .context("build octocrab client")?;

    let mut stats = GithubIngest::default();
    let mut users: Users = HashMap::new();
    let mut labels: Labels = HashMap::new();
    let api = format!("{base}/repos/{owner}/{name}");

    // Top-level signal: poll the event feed (also stored as an activity log). A 304
    // means the repo is idle → skip all per-resource syncs for free.
    let active = events::sync_events(db, &client, &base, owner, name, repo_id, &mut stats).await?;
    if !active {
        return Ok(stats);
    }

    // Each resource is gated by a cheap conditional REST probe (free on 304), then
    // synced incrementally (PRs/issues replace only what changed since their
    // watermark; Actions clears + refetches its bounded tables).
    let pr_res = format!("gh:prs:{repo_id}");
    let (pr_changed, pr_etag) = etag_gate(
        &client,
        &format!("{api}/pulls?state=all&sort=updated&direction=desc&per_page=1"),
        read_etag(db, &pr_res)?.as_deref(),
    )
    .await;
    if pr_changed {
        sync_pr_graph(db, &client, owner, name, repo_id, &mut users, &mut labels, &mut stats).await?;
        if let Some(e) = pr_etag {
            write_etag(db, &pr_res, &e)?;
        }
    } else {
        eprintln!("github: pulls unchanged (304)");
    }

    let is_res = format!("gh:issues:{repo_id}");
    let (is_changed, is_etag) = etag_gate(
        &client,
        &format!("{api}/issues?state=all&sort=updated&direction=desc&per_page=1"),
        read_etag(db, &is_res)?.as_deref(),
    )
    .await;
    if is_changed {
        sync_issues(db, &client, owner, name, repo_id, &mut users, &mut labels, &mut stats).await?;
        if let Some(e) = is_etag {
            write_etag(db, &is_res, &e)?;
        }
    } else {
        eprintln!("github: issues unchanged (304)");
    }

    let run_res = format!("gh:runs:{repo_id}");
    let (run_changed, run_etag) =
        etag_gate(&client, &format!("{api}/actions/runs?per_page=1"), read_etag(db, &run_res)?.as_deref()).await;
    if run_changed {
        actions::sync_actions(db, &client, owner, name, repo_id, &mut stats).await?;
        if let Some(e) = run_etag {
            write_etag(db, &run_res, &e)?;
        }
    } else {
        eprintln!("github: actions unchanged (304)");
    }

    write_users(db, &users)?;
    write_labels(db, repo_id, &labels)?;
    stats.users = users.len();
    Ok(stats)
}

/// Paginate issues (GraphQL) and write each page.
async fn sync_issues(
    db: &Db,
    client: &octocrab::Octocrab,
    owner: &str,
    name: &str,
    repo_id: &str,
    users: &mut Users,
    labels: &mut Labels,
    stats: &mut GithubIngest,
) -> Result<()> {
    let resource = format!("gh:issues:{repo_id}");
    let watermark = read_watermark(db, &resource)?;
    let mut new_wm: Option<DateTime<Utc>> = None;
    let mut cursor: Option<String> = None;
    let mut seen: HashSet<i64> = HashSet::new();
    loop {
        let vars = serde_json::json!({ "owner": owner, "name": name, "cursor": cursor });
        let data: graphql::IssueData = client
            .graphql(&serde_json::json!({ "query": graphql::ISSUE_QUERY, "variables": vars }))
            .await
            .context("issues query")?;
        let conn = data.repository.issues;

        let mut stop_idx = conn.nodes.len();
        let mut stop = false;
        for (i, is) in conn.nodes.iter().enumerate() {
            if let Some(u) = is.updated_at {
                if new_wm.map_or(true, |m| u > m) {
                    new_wm = Some(u);
                }
                if matches!(watermark, Some(w) if u <= w) {
                    stop_idx = i;
                    stop = true;
                    break;
                }
            }
        }
        write_issue_page(db, repo_id, &conn.nodes[..stop_idx], &mut seen, users, labels, stats)?;

        if stop || !conn.page_info.has_next_page || conn.page_info.end_cursor.is_none() {
            break;
        }
        cursor = conn.page_info.end_cursor;
    }
    if let Some(m) = new_wm {
        write_watermark(db, &resource, m)?;
    }
    Ok(())
}

fn write_issue_page(
    db: &Db,
    repo_id: &str,
    nodes: &[IssueNode],
    seen: &mut HashSet<i64>,
    users: &mut Users,
    labels: &mut Labels,
    stats: &mut GithubIngest,
) -> Result<()> {
    let mut to_write: Vec<&IssueNode> = Vec::new();
    for is in nodes {
        if seen.insert(is.number) {
            delete_issue_rows(db, repo_id, is.number)?;
            to_write.push(is);
        }
    }

    let mut is_app = db.conn.appender("gh_issues")?;
    let mut cm_app = db.conn.appender("gh_comments")?;
    let mut lb_app = db.conn.appender("gh_labeled")?;
    for is in to_write {
        let author_id = record(users, &is.author);
        is_app.append_row(params![
            repo_id, is.number, is.title, is.body, is.state, author_id,
            ts(is.created_at), ts(is.updated_at), ts(is.closed_at),
        ])?;
        stats.issues += 1;
        write_labeled(&mut lb_app, labels, repo_id, "issue", is.number, &is.labels.nodes)?;
        for c in &is.comments.nodes {
            let Some(cid) = c.database_id else { continue };
            let aid = record(users, &c.author);
            cm_app.append_row(params![cid, repo_id, "issue", is.number, aid, c.body, ts(c.created_at)])?;
            stats.comments += 1;
        }
    }
    is_app.flush()?;
    cm_app.flush()?;
    lb_app.flush()?;
    Ok(())
}

/// Upsert label defs into `labels`.
fn write_labels(db: &Db, repo_id: &str, labels: &Labels) -> Result<()> {
    let mut stmt = db.conn.prepare(
        "INSERT INTO gh_labels (repo_id, name, color, description) VALUES (?, ?, ?, ?)
         ON CONFLICT (repo_id, name) DO UPDATE SET color = excluded.color, description = excluded.description",
    )?;
    for (name, (color, desc)) in labels {
        stmt.execute(params![repo_id, name, color, desc])?;
    }
    Ok(())
}

/// Paginate the PR graph (GraphQL) and write each page as it arrives.
async fn sync_pr_graph(
    db: &Db,
    client: &octocrab::Octocrab,
    owner: &str,
    name: &str,
    repo_id: &str,
    users: &mut Users,
    labels: &mut Labels,
    stats: &mut GithubIngest,
) -> Result<()> {
    let resource = format!("gh:prs:{repo_id}");
    let watermark = read_watermark(db, &resource)?;
    let mut new_wm: Option<DateTime<Utc>> = None;
    let mut cursor: Option<String> = None;
    let mut seen: HashSet<i64> = HashSet::new();
    loop {
        let vars = serde_json::json!({
            "owner": owner, "name": name, "cursor": cursor,
        });
        let data: graphql::PrData = client
            .graphql(&serde_json::json!({ "query": graphql::PR_QUERY, "variables": vars }))
            .await
            .context("PR graph query")?;
        let conn = data.repository.pull_requests;

        // PRs come newest-first; stop at the first one we've already synced.
        let mut stop_idx = conn.nodes.len();
        let mut stop = false;
        for (i, pr) in conn.nodes.iter().enumerate() {
            if let Some(u) = pr.updated_at {
                if new_wm.map_or(true, |m| u > m) {
                    new_wm = Some(u);
                }
                if matches!(watermark, Some(w) if u <= w) {
                    stop_idx = i;
                    stop = true;
                    break;
                }
            }
        }
        write_pr_page(db, repo_id, &conn.nodes[..stop_idx], &mut seen, users, labels, stats)?;

        if stop || !conn.page_info.has_next_page || conn.page_info.end_cursor.is_none() {
            break;
        }
        cursor = conn.page_info.end_cursor;
    }
    if let Some(m) = new_wm {
        write_watermark(db, &resource, m)?;
    }
    Ok(())
}

/// Write one page of PRs + their sub-resources via the Appender.
fn write_pr_page(
    db: &Db,
    repo_id: &str,
    nodes: &[PrNode],
    seen: &mut HashSet<i64>,
    users: &mut Users,
    labels: &mut Labels,
    stats: &mut GithubIngest,
) -> Result<()> {
    // Pass 1: dedup (updated_at pagination can re-surface a PR) + delete the
    // existing rows of each PR we're about to (re)write — before opening appenders.
    let mut to_write: Vec<&PrNode> = Vec::new();
    for pr in nodes {
        if seen.insert(pr.number) {
            delete_pr_rows(db, repo_id, pr.number)?;
            to_write.push(pr);
        }
    }

    // Pass 2: append.
    let mut pr_app = db.conn.appender("gh_pull_requests")?;
    let mut rv_app = db.conn.appender("gh_pr_reviews")?;
    let mut pc_app = db.conn.appender("gh_pr_commits")?;
    let mut rr_app = db.conn.appender("gh_requested_reviewers")?;
    let mut cm_app = db.conn.appender("gh_comments")?;
    let mut rc_app = db.conn.appender("gh_review_comments")?;
    let mut lb_app = db.conn.appender("gh_labeled")?;

    for pr in to_write {
        let author_id = record(users, &pr.author);
        let merge_oid = pr.merge_commit.as_ref().and_then(|o| oid(&o.oid));
        let head_oid = pr.head_oid.as_deref().and_then(oid);
        let base_oid = pr.base_oid.as_deref().and_then(oid);
        pr_app.append_row(params![
            repo_id, pr.number, pr.title, pr.body, pr.state, author_id,
            ts(pr.created_at), ts(pr.updated_at), ts(pr.closed_at), ts(pr.merged_at),
            merge_oid.as_ref().map(|o| o.as_bytes()),
            pr.head_ref, pr.base_ref, pr.additions, pr.deletions, pr.changed_files, pr.is_draft,
            pr.mergeable, pr.checks(),
            head_oid.as_ref().map(|o| o.as_bytes()), base_oid.as_ref().map(|o| o.as_bytes()),
        ])?;
        stats.pull_requests += 1;
        write_labeled(&mut lb_app, labels, repo_id, "pr", pr.number, &pr.labels.nodes)?;

        let mut seen_commits = HashSet::new();
        for c in &pr.commits.nodes {
            if let Some(o) = oid(&c.commit.oid) {
                if seen_commits.insert(o) {
                    pc_app.append_row(params![repo_id, pr.number, o.as_bytes()])?;
                }
            }
        }
        if pr.commits.total_count as usize > pr.commits.nodes.len() {
            stats.truncated += 1;
        }

        for r in &pr.reviews.nodes {
            let Some(rid) = r.database_id else { continue };
            let reviewer = record(users, &r.author);
            rv_app.append_row(params![
                rid, repo_id, pr.number, reviewer, r.state, ts(r.submitted_at), r.body
            ])?;
            stats.reviews += 1;
        }
        if pr.reviews.total_count as usize > pr.reviews.nodes.len() {
            stats.truncated += 1;
        }

        let mut seen_rr = HashSet::new();
        for rr in &pr.review_requests.nodes {
            if let Some(uid) = rr.requested_reviewer.as_ref().and_then(|a| a.database_id) {
                record(users, &rr.requested_reviewer);
                if seen_rr.insert(uid) {
                    rr_app.append_row(params![repo_id, pr.number, uid])?;
                }
            }
        }

        for c in &pr.comments.nodes {
            let Some(cid) = c.database_id else { continue };
            let aid = record(users, &c.author);
            cm_app.append_row(params![cid, repo_id, "pr", pr.number, aid, c.body, ts(c.created_at)])?;
            stats.comments += 1;
        }

        for thread in &pr.review_threads.nodes {
            for rc in &thread.comments.nodes {
                let Some(rcid) = rc.database_id else { continue };
                let aid = record(users, &rc.author);
                let coid = rc.commit.as_ref().and_then(|o| oid(&o.oid));
                let line = rc.line.or(rc.original_line);
                rc_app.append_row(params![
                    rcid, repo_id, pr.number, rc.path, line, thread.side.as_deref(),
                    coid.as_ref().map(|o| o.as_bytes()), aid, rc.body, ts(rc.created_at),
                    rc.reply_to.as_ref().and_then(|r| r.database_id),
                ])?;
                stats.review_comments += 1;
            }
        }
    }

    pr_app.flush()?;
    rv_app.flush()?;
    pc_app.flush()?;
    rr_app.flush()?;
    cm_app.flush()?;
    rc_app.flush()?;
    lb_app.flush()?;
    Ok(())
}

/// Upsert the deduped users into `gh_users` (global; survives multi-repo DBs).
fn write_users(db: &Db, users: &Users) -> Result<()> {
    let mut stmt = db.conn.prepare(
        "INSERT INTO gh_users (id, login, type, name) VALUES (?, ?, ?, NULL)
         ON CONFLICT (id) DO UPDATE SET login = excluded.login, type = excluded.type",
    )?;
    for (id, (login, ty)) in users {
        stmt.execute(params![id, login, ty])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_owner_repo;

    #[test]
    fn parses_remote_urls() {
        let cases = [
            "git@github.com:confluentinc/ksql.git",
            "https://github.com/confluentinc/ksql.git",
            "https://github.com/confluentinc/ksql",
            "ssh://git@github.com/confluentinc/ksql.git",
        ];
        for c in cases {
            assert_eq!(
                parse_owner_repo(c),
                Some(("confluentinc".into(), "ksql".into())),
                "{c}"
            );
        }
        assert_eq!(parse_owner_repo("git@gitlab.com:foo/bar.git"), None);
    }
}
