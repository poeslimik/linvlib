//! New-release driven catalog refresh.
//!
//! Instead of re-searching every series, we pull Aladin's ItemNewAll lists,
//! refresh catalog matches, and queue unmatched LN titles as admin suggestions.

use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use tokio::time::sleep;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::{NewReleaseRefreshResult, RefreshStartResponse},
    repositories,
    services::aladin::{self, AladinItem},
    services::quota,
    state::AppState,
};

const REFRESH_RUNNING_KEY: &str = "refresh_running";

/// Start a background refresh. Returns immediately.
///
/// If a job is already running, `started` is false and `already_running` is true.
pub async fn start_background(state: AppState) -> AppResult<RefreshStartResponse> {
    if !state.refresh_gate.try_begin() {
        return Ok(RefreshStartResponse {
            started: false,
            already_running: true,
            message: "신간 갱신이 이미 진행 중입니다.".into(),
        });
    }

    let started_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let _ = repositories::set_app_meta(&state.pool, REFRESH_RUNNING_KEY, "1").await;
    let _ = repositories::set_app_meta(
        &state.pool,
        "last_refresh_note",
        &format!("running: started_at={started_at}"),
    )
    .await;

    tokio::spawn(async move {
        let result = refresh(&state).await;
        match &result {
            Ok(res) => {
                tracing::info!(
                    scanned = res.scanned_items,
                    matched = res.matched_series,
                    refreshed = res.refreshed,
                    failed = res.failed,
                    suggested = res.suggested,
                    "background new-release refresh finished"
                );
            }
            Err(err) => {
                let note = format!("newlist failed: {err}");
                let _ = repositories::set_app_meta(&state.pool, "last_refresh_note", &note).await;
                let _ = repositories::set_app_meta(
                    &state.pool,
                    "last_refresh_at",
                    &Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                )
                .await;
                tracing::error!(error = %err, "background new-release refresh failed");
            }
        }
        let _ = repositories::set_app_meta(&state.pool, REFRESH_RUNNING_KEY, "0").await;
        state.refresh_gate.end();
    });

    Ok(RefreshStartResponse {
        started: true,
        already_running: false,
        message: "신간 갱신을 백그라운드에서 시작했습니다. 완료까지 수 분 걸릴 수 있습니다.".into(),
    })
}

/// Prefer this for admin status: gate (same process) or persisted flag.
pub async fn is_running_persisted(state: &AppState) -> bool {
    if state.refresh_gate.is_running() {
        return true;
    }
    matches!(
        repositories::get_app_meta(&state.pool, REFRESH_RUNNING_KEY).await,
        Ok(Some(v)) if v == "1"
    )
}

/// Run a full new-release refresh (manual / scheduled).
pub async fn refresh(state: &AppState) -> AppResult<NewReleaseRefreshResult> {
    let items = aladin::fetch_new_release_items(state).await?;
    let scanned_items = items.len() as i64;

    // series_id -> (title, aladin_series_id, seed_item_id)
    let mut matched_jobs: HashMap<Uuid, (String, Option<String>, Option<String>)> = HashMap::new();
    let mut suggestion_items: HashMap<String, AladinItem> = HashMap::new();

    for item in items {
        match resolve_catalog_match(state, &item).await? {
            Some((series_id, title, catalog_key)) => {
                let (list_aladin, list_seed) = import_keys_for_item(&item);
                let entry = matched_jobs.entry(series_id).or_insert_with(|| {
                    let aladin = if is_synthetic_series_key(&catalog_key) {
                        list_aladin
                    } else {
                        Some(catalog_key)
                    };
                    let seed = if aladin.is_some() {
                        None
                    } else {
                        list_seed.or_else(|| Some(item.item_id.to_string()))
                    };
                    (title, aladin, seed)
                });
                if entry.1.is_none() {
                    if let Some(sid) = item.series_id() {
                        entry.1 = Some(sid.to_string());
                        entry.2 = None;
                    }
                }
            }
            None => {
                let (key, _) = suggestion_key_for_item(&item);
                suggestion_items.entry(key).or_insert(item);
            }
        }
    }

    let suggested = upsert_suggestions(state, &suggestion_items).await?;
    let matched_series = matched_jobs.len() as i64;
    let (refreshed, failed, note) =
        refresh_matched(state, matched_jobs, scanned_items, suggested).await?;

    let _ = repositories::set_app_meta(&state.pool, "last_refresh_note", &note).await;
    let _ = repositories::set_app_meta(
        &state.pool,
        "last_refresh_at",
        &Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    )
    .await;

    Ok(NewReleaseRefreshResult {
        scanned_items,
        matched_series,
        refreshed,
        failed,
        suggested,
    })
}

fn is_synthetic_series_key(key: &str) -> bool {
    key.starts_with("item:") || key.starts_with("title:") || key.starts_with("manual:")
}

fn suggestion_key_for_item(item: &AladinItem) -> (String, Option<String>) {
    if let Some(sid) = item.series_id() {
        let key = sid.to_string();
        return (key.clone(), Some(key));
    }
    (format!("item:{}", item.item_id), None)
}

fn import_keys_for_item(item: &AladinItem) -> (Option<String>, Option<String>) {
    if let Some(sid) = item.series_id() {
        return (Some(sid.to_string()), None);
    }
    (None, Some(item.item_id.to_string()))
}

fn optional_trim(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

async fn resolve_catalog_match(
    state: &AppState,
    item: &AladinItem,
) -> AppResult<Option<(Uuid, String, String)>> {
    if let Some(sid) = item.series_id() {
        let key = sid.to_string();
        if let Some(series) = repositories::find_series_by_aladin_id(&state.pool, &key).await? {
            return Ok(Some((series.id, series.title, key)));
        }
    }

    let item_id = item.item_id.to_string();
    if let Some(series_id) =
        repositories::series_id_for_aladin_item_id(&state.pool, &item_id).await?
    {
        if let Some(series) = repositories::find_series_by_id(&state.pool, series_id).await? {
            return Ok(Some((
                series.id,
                series.title.clone(),
                series.aladin_series_id,
            )));
        }
    }
    Ok(None)
}

async fn upsert_suggestions(
    state: &AppState,
    suggestion_items: &HashMap<String, AladinItem>,
) -> AppResult<i64> {
    let mut suggested = 0i64;
    for (key, item) in suggestion_items {
        let (_, aladin_series_id) = suggestion_key_for_item(item);
        let mut title = aladin::normalize_series_title(&item.title);
        if title.is_empty() {
            title = item.title.clone();
        }
        repositories::upsert_new_release_suggestion(
            &state.pool,
            key,
            aladin_series_id.as_deref(),
            &item.item_id.to_string(),
            &title,
            optional_trim(&item.author),
            optional_trim(&item.publisher),
            optional_trim(&item.cover),
            optional_trim(&item.pub_date),
        )
        .await?;
        suggested += 1;
    }
    Ok(suggested)
}

async fn refresh_matched(
    state: &AppState,
    matched_jobs: HashMap<Uuid, (String, Option<String>, Option<String>)>,
    scanned_items: i64,
    suggested: i64,
) -> AppResult<(i64, i64, String)> {
    let matched_series = matched_jobs.len() as i64;
    let mut refreshed = 0i64;
    let mut failed = 0i64;
    let mut stopped_for_quota = false;
    let mut deferred = 0i64;

    for (idx, (series_id, (title, aladin_series_id, seed_item_id))) in
        matched_jobs.into_iter().enumerate()
    {
        if quota::remaining_soft_quota(state).await? == 0 {
            stopped_for_quota = true;
            deferred = matched_series - (refreshed + failed);
            tracing::info!(
                refreshed,
                failed,
                deferred,
                "stopping new-release refresh: daily soft quota reached"
            );
            break;
        }
        if idx > 0 {
            sleep(Duration::from_millis(250)).await;
        }

        match aladin::import_series(
            state,
            aladin_series_id.clone(),
            seed_item_id.into_iter().collect(),
            Some(title.clone()),
        )
        .await
        {
            Ok(_) => {
                refreshed += 1;
            }
            Err(AppError::QuotaExceeded(_)) => {
                failed += 1;
                stopped_for_quota = true;
                deferred = matched_series - (refreshed + failed);
                break;
            }
            Err(err) => {
                failed += 1;
                tracing::warn!(%series_id, %title, error = %err, "new-release refresh failed");
            }
        }
    }

    let note = if stopped_for_quota {
        format!(
            "newlist quota pause: scanned={scanned_items} matched={matched_series} refreshed={refreshed} failed={failed} deferred={deferred} suggested={suggested}"
        )
    } else {
        format!(
            "newlist complete: scanned={scanned_items} matched={matched_series} refreshed={refreshed} failed={failed} suggested={suggested}"
        )
    };

    Ok((refreshed, failed, note))
}
