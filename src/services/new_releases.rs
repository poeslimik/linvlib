//! New-release driven catalog refresh.
//!
//! Instead of re-searching every series, we pull Aladin's ItemNewAll lists,
//! refresh catalog matches, and queue unmatched LN titles as admin suggestions.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{NaiveDate, Utc};
use tokio::time::sleep;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::{BulkRefreshItem, BulkRefreshResponse},
    repositories,
    services::aladin::{self, AladinItem},
    services::quota,
    state::AppState,
};

/// Run a full new-release refresh (manual / admin button).
pub async fn refresh(state: &AppState) -> AppResult<BulkRefreshResponse> {
    refresh_bounded(state, None).await
}

/// Same as [`refresh`], but stop importing matched series after `stay_on_date`
/// ends (used by the 23:30 KST scheduler so tomorrow's quota is not spent).
pub async fn refresh_bounded(
    state: &AppState,
    stay_on_date: Option<NaiveDate>,
) -> AppResult<BulkRefreshResponse> {
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
    let (refreshed, failed, results, note) =
        refresh_matched(state, matched_jobs, stay_on_date, scanned_items, suggested).await?;

    let _ = repositories::set_app_meta(&state.pool, "last_refresh_note", &note).await;
    let _ = repositories::set_app_meta(
        &state.pool,
        "last_refresh_at",
        &Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    )
    .await;

    Ok(BulkRefreshResponse {
        scanned_items,
        matched_series,
        refreshed,
        failed,
        suggested,
        total: matched_series,
        items: results,
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
    stay_on_date: Option<NaiveDate>,
    scanned_items: i64,
    suggested: i64,
) -> AppResult<(i64, i64, Vec<BulkRefreshItem>, String)> {
    let matched_series = matched_jobs.len() as i64;
    let mut refreshed = 0i64;
    let mut failed = 0i64;
    let mut results = Vec::with_capacity(matched_jobs.len());
    let mut stopped_for_quota = false;
    let mut stopped_for_midnight = false;
    let mut deferred = 0i64;

    for (idx, (series_id, (title, aladin_series_id, seed_item_id))) in
        matched_jobs.into_iter().enumerate()
    {
        if let Some(day) = stay_on_date {
            if quota::seoul_today_date() != day {
                stopped_for_midnight = true;
                deferred = matched_series - (refreshed + failed);
                tracing::info!(
                    refreshed,
                    failed,
                    deferred,
                    stay_on_date = %day,
                    now = %quota::seoul_now_display(),
                    "stopping new-release refresh: KST day rolled over"
                );
                break;
            }
        }
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
            seed_item_id.clone(),
            Some(title.clone()),
        )
        .await
        {
            Ok(res) => {
                refreshed += 1;
                results.push(BulkRefreshItem {
                    series_id: res.series_id,
                    title: res.title,
                    ok: true,
                    volume_count: Some(res.volume_count),
                    error: None,
                });
            }
            Err(AppError::QuotaExceeded(msg)) => {
                failed += 1;
                stopped_for_quota = true;
                results.push(BulkRefreshItem {
                    series_id,
                    title,
                    ok: false,
                    volume_count: None,
                    error: Some(msg),
                });
                deferred = matched_series - (refreshed + failed);
                break;
            }
            Err(err) => {
                failed += 1;
                tracing::warn!(%series_id, %title, error = %err, "new-release refresh failed");
                results.push(BulkRefreshItem {
                    series_id,
                    title,
                    ok: false,
                    volume_count: None,
                    error: Some(err.to_string()),
                });
            }
        }
    }

    let note = if stopped_for_midnight {
        format!(
            "newlist midnight stop: scanned={scanned_items} matched={matched_series} refreshed={refreshed} failed={failed} deferred={deferred} suggested={suggested}"
        )
    } else if stopped_for_quota {
        format!(
            "newlist quota pause: scanned={scanned_items} matched={matched_series} refreshed={refreshed} failed={failed} deferred={deferred} suggested={suggested}"
        )
    } else {
        format!(
            "newlist complete: scanned={scanned_items} matched={matched_series} refreshed={refreshed} failed={failed} suggested={suggested}"
        )
    };

    Ok((refreshed, failed, results, note))
}
