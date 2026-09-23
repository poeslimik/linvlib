//! New-release driven catalog refresh.
//!
//! Pull Yes24 new titles for LN categories/imprints, refresh catalog matches,
//! and queue unmatched titles as admin suggestions.

use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use tokio::time::sleep;
use uuid::Uuid;

use crate::{
    error::AppResult,
    models::{NewReleaseRefreshResult, RefreshStartResponse},
    repositories,
    services::catalog::{self, group, group::CatalogVolume, yes24},
    state::AppState,
};

const REFRESH_RUNNING_KEY: &str = "refresh_running";

/// Who started the refresh. Only the daily schedule posts a Discord status report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshOrigin {
    Scheduled,
    Manual,
}

/// Start a background refresh. Returns immediately.
///
/// If a job is already running, `started` is false and `already_running` is true.
/// A [`RefreshOrigin::Scheduled`] run sends the Discord status report after it finishes.
pub async fn start_background(
    state: AppState,
    origin: RefreshOrigin,
) -> AppResult<RefreshStartResponse> {
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
                let note = format!("refresh failed: {err}");
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
        if origin == RefreshOrigin::Scheduled {
            crate::services::status_report::send_after_scheduled_refresh(&state).await;
        }
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
    let items = yes24::fetch_new_release_items(state).await?;
    let scanned_items = items.len() as i64;

    // series_id -> (title, series_key)
    let mut matched_jobs: HashMap<Uuid, (String, String)> = HashMap::new();
    let mut suggestion_items: HashMap<String, CatalogVolume> = HashMap::new();

    for item in items {
        match resolve_catalog_match(state, &item).await? {
            Some((series_id, title, catalog_key)) => {
                matched_jobs
                    .entry(series_id)
                    .or_insert_with(|| (title, catalog_key));
            }
            None => {
                let key = suggestion_key_for_item(&item);
                suggestion_items.entry(key).or_insert(item);
            }
        }
    }

    let suggested = upsert_suggestions(state, &suggestion_items).await?;
    let matched_series = matched_jobs.len() as i64;
    let (refreshed, failed, note) =
        refresh_matched(state, matched_jobs, scanned_items, suggested).await?;

    if let Ok(n) = repositories::dismiss_suggestions_already_in_catalog(&state.pool).await {
        if n > 0 {
            tracing::info!(dismissed = n, "dismissed duplicate new-release suggestions");
        }
    }

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

fn suggestion_key_for_item(item: &CatalogVolume) -> String {
    if let Some(stored) = item
        .stored_item_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return stored.to_string();
    }
    if !item.isbn13.trim().is_empty() {
        return format!("isbn:{}", item.isbn13.trim());
    }
    let title = group::normalize_series_title(&item.title);
    if !title.is_empty() {
        format!("title:{title}")
    } else {
        format!("id:{}", item.item_id)
    }
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
    item: &CatalogVolume,
) -> AppResult<Option<(Uuid, String, String)>> {
    let external = group::volume_external_id(item);
    if let Some(series_id) =
        repositories::series_id_for_aladin_item_id(&state.pool, &external).await?
    {
        if let Some(series) = repositories::find_series_by_id(&state.pool, series_id).await? {
            return Ok(Some((
                series.id,
                series.title.clone(),
                series.aladin_series_id,
            )));
        }
    }

    if !item.isbn13.trim().is_empty() {
        let isbn_key = format!("isbn:{}", item.isbn13.trim());
        if isbn_key != external {
            if let Some(series_id) =
                repositories::series_id_for_aladin_item_id(&state.pool, &isbn_key).await?
            {
                if let Some(series) = repositories::find_series_by_id(&state.pool, series_id).await?
                {
                    return Ok(Some((
                        series.id,
                        series.title.clone(),
                        series.aladin_series_id,
                    )));
                }
            }
        }
    }

    let normalized = group::normalize_series_title(&item.title);
    if !normalized.is_empty() {
        if let Some(series) =
            repositories::find_series_by_normalized_title(&state.pool, &normalized).await?
        {
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
    suggestion_items: &HashMap<String, CatalogVolume>,
) -> AppResult<i64> {
    let mut suggested = 0i64;
    for (key, item) in suggestion_items {
        let external = group::volume_external_id(item);
        if repositories::series_id_for_aladin_item_id(&state.pool, &external)
            .await?
            .is_some()
        {
            continue;
        }
        let normalized = group::normalize_series_title(&item.title);
        if !normalized.is_empty()
            && repositories::find_series_by_normalized_title(&state.pool, &normalized)
                .await?
                .is_some()
        {
            continue;
        }

        let mut title = normalized.clone();
        if title.is_empty() {
            title = item.title.clone();
        }
        let series_key = if !normalized.is_empty() {
            Some(format!("title:{normalized}"))
        } else {
            None
        };
        repositories::upsert_new_release_suggestion(
            &state.pool,
            key,
            series_key.as_deref(),
            &external,
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
    matched_jobs: HashMap<Uuid, (String, String)>,
    scanned_items: i64,
    suggested: i64,
) -> AppResult<(i64, i64, String)> {
    let matched_series = matched_jobs.len() as i64;
    let mut refreshed = 0i64;
    let mut failed = 0i64;

    for (idx, (series_id, (title, series_key))) in matched_jobs.into_iter().enumerate() {
        if idx > 0 {
            sleep(Duration::from_millis(250)).await;
        }

        let import_key = if series_key.starts_with("title:") {
            series_key.clone()
        } else {
            format!("title:{}", group::normalize_series_title(&title))
        };

        match catalog::import_series(state, Some(import_key), Vec::new(), Some(title.clone()))
            .await
        {
            Ok(_) => {
                refreshed += 1;
            }
            Err(err) => {
                failed += 1;
                tracing::warn!(%series_id, %title, error = %err, "new-release refresh failed");
            }
        }
    }

    let note = format!(
        "refresh complete: scanned={scanned_items} matched={matched_series} refreshed={refreshed} failed={failed} suggested={suggested}"
    );

    Ok((refreshed, failed, note))
}
