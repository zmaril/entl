//! Check runs (Checks API) + commit statuses (Status API). Both are ref-scoped
//! (per commit), so we fetch them only for the head commits seen in workflow
//! runs — a bounded, relevant set rather than every commit in history.

use std::collections::HashSet;

use anyhow::Result;
use duckdb::params;
use duckdb::types::Value;
use futures::stream::{self, StreamExt};
use octocrab::params::repos::Commitish;
use octocrab::Octocrab;

use super::{enum_str, oid, ts, GithubIngest, CONCURRENCY};
use crate::db::Db;

const SHA_CAP: usize = 500; // bound the per-commit fan-out for v1

pub async fn sync_checks(
    db: &Db,
    client: &Octocrab,
    owner: &str,
    name: &str,
    repo_id: &str,
    head_shas: &HashSet<String>,
    stats: &mut GithubIngest,
) -> Result<()> {
    let mut shas: Vec<String> = head_shas.iter().cloned().collect();
    shas.truncate(SHA_CAP);
    if head_shas.len() > SHA_CAP {
        eprintln!(
            "github: check-runs/statuses limited to {} of {} head commits",
            SHA_CAP,
            head_shas.len()
        );
    }

    // Fetch check-runs + statuses per commit concurrently, write sequentially.
    let results: Vec<_> = stream::iter(shas)
        .map(|sha| async move {
            let checks = client
                .checks(owner, name)
                .list_check_runs_for_git_ref(Commitish(sha.clone()))
                .send()
                .await;
            let statuses = client.repos(owner, name).list_statuses(sha.clone()).send().await;
            (sha, checks, statuses)
        })
        .buffer_unordered(CONCURRENCY)
        .collect()
        .await;

    let mut cr_app = db.conn.appender("gh_check_runs")?;
    let mut st_app = db.conn.appender("gh_commit_statuses")?;
    for (sha, checks, statuses) in &results {
        let Some(commit_oid) = oid(sha) else { continue };
        let commit_bytes = commit_oid.as_bytes();
        if let Ok(list) = checks {
            for cr in &list.check_runs {
                cr_app.append_row(params![
                    cr.id.0 as i64,
                    repo_id,
                    commit_bytes,
                    cr.name,
                    Value::Null, // octocrab's CheckRun omits `status`
                    cr.conclusion,
                    ts(cr.started_at),
                    ts(cr.completed_at),
                ])?;
                stats.check_runs += 1;
            }
        }
        if let Ok(page) = statuses {
            for st in &page.items {
                let Some(id) = st.id else { continue };
                st_app.append_row(params![
                    id.0 as i64,
                    repo_id,
                    commit_bytes,
                    st.context,
                    enum_str(&st.state),
                    st.description,
                    st.target_url,
                    ts(st.created_at),
                ])?;
            }
        }
    }
    cr_app.flush()?;
    st_app.flush()?;
    Ok(())
}
