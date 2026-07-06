//! Actions / CI via REST (octocrab): workflows, workflow runs, and each run's
//! jobs + steps. Runs are fetched most-recent-first and capped (full history is
//! an incremental-sync concern); each run's head sha is collected so check-runs
//! and commit-statuses can be fetched for the relevant commits.

use std::collections::HashSet;

use anyhow::{Context, Result};
use duckdb::params;
use duckdb::types::Value;
use futures::stream::{self, StreamExt};
use octocrab::models::workflows::Run;
use octocrab::Octocrab;

use super::{enum_str as s, oid, ts, GithubIngest, CONCURRENCY};
use crate::db::Db;

const PER_PAGE: u8 = 100;
const RUN_PAGE_CAP: u32 = 5; // up to 5 × 100 = 500 most-recent runs (v1 bound)

pub async fn sync_actions(
    db: &Db,
    client: &Octocrab,
    owner: &str,
    name: &str,
    repo_id: &str,
    stats: &mut GithubIngest,
) -> Result<()> {
    // Actions is a bounded full-refetch: clear this repo's rows first. jobs/steps
    // have no repo_id, so cascade via run_id / job_id.
    db.conn.execute(
        "DELETE FROM gh_steps WHERE job_id IN (SELECT j.id FROM gh_jobs j \
         JOIN gh_workflow_runs r ON r.id = j.run_id WHERE r.repo_id = ?)",
        params![repo_id],
    )?;
    db.conn.execute(
        "DELETE FROM gh_jobs WHERE run_id IN (SELECT id FROM gh_workflow_runs WHERE repo_id = ?)",
        params![repo_id],
    )?;
    for t in [
        "gh_workflow_runs",
        "gh_workflows",
        "gh_check_runs",
        "gh_commit_statuses",
    ] {
        db.conn.execute(
            &format!("DELETE FROM {t} WHERE repo_id = ?"),
            params![repo_id],
        )?;
    }

    // --- workflows ---
    let wfs = client
        .workflows(owner, name)
        .list()
        .per_page(PER_PAGE)
        .send()
        .await
        .context("list workflows")?;
    {
        let mut app = db.conn.appender("gh_workflows")?;
        for w in &wfs.items {
            app.append_row(params![w.id.0 as i64, repo_id, w.name, w.path, w.state])?;
        }
        app.flush()?;
    }

    // --- workflow runs (+ jobs + steps), most-recent-first, capped ---
    let mut head_shas: HashSet<String> = HashSet::new();
    let mut page = client
        .workflows(owner, name)
        .list_all_runs()
        .per_page(PER_PAGE)
        .send()
        .await
        .context("list workflow runs")?;
    let mut run_ids = Vec::new();
    let mut pages_done = 0u32;
    loop {
        let mut run_app = db.conn.appender("gh_workflow_runs")?;
        for r in &page.items {
            head_shas.insert(r.head_sha.clone());
            run_ids.push(r.id);
            run_app.append_row(params![
                r.id.0 as i64,
                repo_id,
                r.workflow_id.0 as i64,
                oid(&r.head_sha).as_ref().map(|o| o.as_bytes()),
                r.head_branch,
                r.event,
                r.status,
                r.conclusion,
                r.run_number,
                Value::Null, // run_attempt (not on the run model)
                ts(Some(r.created_at)),
                ts(Some(r.updated_at)),
                Value::Null, // run_started_at
            ])?;
            stats.workflow_runs += 1;
        }
        run_app.flush()?;

        pages_done += 1;
        if pages_done >= RUN_PAGE_CAP {
            break;
        }
        match client.get_page::<Run>(&page.next).await {
            Ok(Some(next)) => page = next,
            _ => break,
        }
    }

    // Jobs (+ steps) per run: fetch concurrently, write sequentially.
    let job_pages: Vec<_> = stream::iter(run_ids)
        .map(|id| async move {
            client
                .workflows(owner, name)
                .list_jobs(id)
                .per_page(PER_PAGE)
                .send()
                .await
        })
        .buffer_unordered(CONCURRENCY)
        .collect()
        .await;
    {
        let mut job_app = db.conn.appender("gh_jobs")?;
        let mut step_app = db.conn.appender("gh_steps")?;
        for jobs in job_pages.iter().flatten() {
            for j in &jobs.items {
                job_app.append_row(params![
                    j.id.0 as i64,
                    j.run_id.0 as i64,
                    j.name,
                    s(&j.status),
                    j.conclusion.as_ref().and_then(s),
                    ts(Some(j.started_at)),
                    ts(j.completed_at),
                    j.runner_name,
                ])?;
                for st in &j.steps {
                    step_app.append_row(params![
                        j.id.0 as i64,
                        st.number,
                        st.name,
                        s(&st.status),
                        st.conclusion.as_ref().and_then(s),
                        ts(st.started_at),
                        ts(st.completed_at),
                    ])?;
                }
            }
        }
        job_app.flush()?;
        step_app.flush()?;
    }

    super::checks::sync_checks(db, client, owner, name, repo_id, &head_shas, stats).await?;
    Ok(())
}
