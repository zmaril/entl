//! GitHub event-feed ingest (`/repos/{o}/{r}/events`). This is entl's top-level
//! change signal: a `304` on the feed means the repo is idle (skip all syncs). On
//! a change we store every new event (the activity log) and let the caller run the
//! per-resource gated syncs. The feed is capped (~300 events / 90 days), so the
//! `events` table is complete *going forward*, not a historical backfill.

use anyhow::Result;
use chrono::{DateTime, Utc};
use duckdb::params;
use octocrab::Octocrab;
use serde::Deserialize;

use super::{etag_gate, read_etag, read_watermark, ts, write_etag, write_watermark, GithubIngest};
use crate::db::Db;

const MAX_PAGES: u32 = 3; // the feed caps at ~300 events (3 × 100)

#[derive(Deserialize)]
struct GhEvent {
    id: String,
    #[serde(rename = "type")]
    event_type: Option<String>,
    actor: Option<EventActor>,
    created_at: Option<DateTime<Utc>>,
    payload: Option<serde_json::Value>,
}
#[derive(Deserialize)]
struct EventActor {
    id: Option<i64>,
    login: Option<String>,
}

/// Poll the event feed. Returns `true` if the feed changed (caller should run the
/// per-resource syncs); `false` on a `304` (repo idle — skip everything).
pub async fn sync_events(
    db: &Db,
    client: &Octocrab,
    base: &str,
    owner: &str,
    name: &str,
    repo_id: &str,
    stats: &mut GithubIngest,
) -> Result<bool> {
    let resource = format!("gh:events:{repo_id}");
    let gate_url = format!("{base}/repos/{owner}/{name}/events?per_page=1");
    let (changed, etag) = etag_gate(client, &gate_url, read_etag(db, &resource)?.as_deref()).await;
    if !changed {
        eprintln!("github: events unchanged (304)");
        return Ok(false);
    }

    let watermark = read_watermark(db, &resource)?;
    let mut new_wm: Option<DateTime<Utc>> = None;
    let mut stmt = db.conn.prepare(
        "INSERT INTO gh_events (repo_id, id, type, actor_id, actor_login, created_at, payload)
         VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT (repo_id, id) DO NOTHING",
    )?;

    'pages: for page in 1..=MAX_PAGES {
        let route = format!("/repos/{owner}/{name}/events?per_page=100&page={page}");
        let events: Vec<GhEvent> = match client.get(&route, None::<&()>).await {
            Ok(e) => e,
            Err(_) => break,
        };
        if events.is_empty() {
            break;
        }
        for e in &events {
            // Feed is newest-first; stop once we reach already-stored events.
            if let Some(c) = e.created_at {
                if new_wm.map_or(true, |m| c > m) {
                    new_wm = Some(c);
                }
                if matches!(watermark, Some(w) if c <= w) {
                    break 'pages;
                }
            }
            let payload = e.payload.as_ref().map(|p| p.to_string());
            let n = stmt.execute(params![
                repo_id,
                e.id,
                e.event_type,
                e.actor.as_ref().and_then(|a| a.id),
                e.actor.as_ref().and_then(|a| a.login.clone()),
                ts(e.created_at),
                payload,
            ])?;
            stats.events += n; // ON CONFLICT DO NOTHING → counts only new rows
        }
    }

    if let Some(e) = etag {
        write_etag(db, &resource, &e)?;
    }
    if let Some(m) = new_wm {
        write_watermark(db, &resource, m)?;
    }
    Ok(true)
}
