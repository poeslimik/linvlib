//! Manual series registration (non-Aladin).
//! Uses synthetic IDs: `manual:{uuid}` / `manual-vol:{uuid}`.

use chrono::{NaiveDate, TimeZone, Utc};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::{ImportResponse, ManualSeriesRequest, ManualVolumeInput},
    repositories,
    state::AppState,
};

pub fn is_manual_series_id(aladin_series_id: &str) -> bool {
    aladin_series_id.starts_with("manual:")
}

pub(crate) async fn ensure_title_available_pub(
    pool: &sqlx::SqlitePool,
    title: &str,
    exclude_id: Option<Uuid>,
    force: bool,
) -> AppResult<()> {
    ensure_title_available(pool, title, exclude_id, force).await
}

pub(crate) fn normalize_volumes_pub(
    title: &str,
    volumes: Vec<ManualVolumeInput>,
    volume_count: Option<i64>,
) -> AppResult<Vec<(i64, ManualVolumeInput)>> {
    normalize_volumes(title, volumes, volume_count)
}

pub(crate) fn apply_series_cover_pub(
    numbered: &mut [(i64, ManualVolumeInput)],
    cover_url: Option<&str>,
) {
    apply_series_cover(numbered, cover_url);
}

pub(crate) fn volume_title_pub(series_title: &str, num: i64, vol: &ManualVolumeInput) -> String {
    volume_title(series_title, num, vol)
}

pub(crate) fn parse_date_pub(value: Option<&str>) -> Option<NaiveDate> {
    parse_date(value)
}

pub async fn create_manual_series(
    state: &AppState,
    body: ManualSeriesRequest,
) -> AppResult<ImportResponse> {
    let title = body.title.trim();
    if title.is_empty() {
        return Err(AppError::BadRequest("title is required".into()));
    }

    ensure_title_available(&state.pool, title, None, body.force).await?;

    let mut numbered = normalize_volumes(title, body.volumes, body.volume_count)?;
    apply_series_cover(&mut numbered, body.cover_url.as_deref());

    let series_id = Uuid::new_v4();
    let aladin_series_id = format!("manual:{series_id}");

    let dates: Vec<NaiveDate> = numbered
        .iter()
        .filter_map(|(_, v)| parse_date(v.published_at.as_deref()))
        .collect();
    let first = dates.iter().min().copied();
    let latest = dates.iter().max().copied();

    let series = repositories::upsert_series_with_cover(
        &state.pool,
        series_id,
        title,
        body.author.as_deref().map(str::trim).filter(|s| !s.is_empty()),
        body.publisher
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
        &aladin_series_id,
        first.map(|d| Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap())),
        latest.map(|d| Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap())),
        body.cover_url.as_deref(),
    )
    .await?;

    for (num, vol) in &numbered {
        let vol_id = Uuid::new_v4();
        let vol_title = volume_title(title, *num, vol);
        repositories::insert_manual_volume(
            &state.pool,
            vol_id,
            series.id,
            *num,
            &vol_title,
            vol.cover_url.as_deref().map(str::trim).filter(|s| !s.is_empty()),
            parse_date(vol.published_at.as_deref()),
        )
        .await?;
    }

    repositories::refresh_series_publish_dates(&state.pool, series.id).await?;
    let volume_count = repositories::count_volumes(&state.pool, series.id).await?;

    Ok(ImportResponse {
        series_id: series.id,
        title: series.title,
        volume_count,
    })
}

pub async fn update_manual_series(
    state: &AppState,
    series_id: Uuid,
    body: ManualSeriesRequest,
) -> AppResult<ImportResponse> {
    crate::services::catalog_edit::update_series(state, series_id, body).await
}

pub async fn delete_manual_series(state: &AppState, series_id: Uuid) -> AppResult<()> {
    repositories::find_series_by_id(&state.pool, series_id)
        .await?
        .ok_or_else(|| AppError::NotFound("series not found".into()))?;

    repositories::delete_series(&state.pool, series_id).await
}

async fn ensure_title_available(
    pool: &sqlx::SqlitePool,
    title: &str,
    exclude_id: Option<Uuid>,
    force: bool,
) -> AppResult<()> {
    if force {
        return Ok(());
    }
    if let Some(existing) = repositories::find_series_by_title_exact(pool, title).await? {
        if exclude_id != Some(existing.id) {
            return Err(AppError::TitleConflict {
                series_id: existing.id,
                title: existing.title,
            });
        }
    }
    Ok(())
}

/// Fill volumes that lack a cover with the series-level cover URL.
fn apply_series_cover(numbered: &mut [(i64, ManualVolumeInput)], cover_url: Option<&str>) {
    let cover = cover_url.map(str::trim).filter(|s| !s.is_empty());
    let Some(cover) = cover else {
        return;
    };
    for (_, vol) in numbered.iter_mut() {
        let has_cover = vol
            .cover_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .is_some();
        if !has_cover {
            vol.cover_url = Some(cover.to_string());
        }
    }
}

fn normalize_volumes(
    title: &str,
    volumes: Vec<ManualVolumeInput>,
    volume_count: Option<i64>,
) -> AppResult<Vec<(i64, ManualVolumeInput)>> {
    let mut volumes = volumes;
    if volumes.is_empty() {
        let count = volume_count.unwrap_or(0);
        if count <= 0 {
            return Err(AppError::BadRequest(
                "volumes or volume_count (>=1) is required".into(),
            ));
        }
        if count > 500 {
            return Err(AppError::BadRequest("volume_count too large".into()));
        }
        volumes = (1..=count)
            .map(|n| ManualVolumeInput {
                id: None,
                volume_number: Some(n),
                title: Some(format!("{title} {n}권")),
                published_at: None,
                cover_url: None,
            })
            .collect();
    }

    let mut numbered: Vec<(i64, ManualVolumeInput)> = Vec::new();
    for (idx, vol) in volumes.into_iter().enumerate() {
        let num = vol.volume_number.unwrap_or((idx + 1) as i64);
        if num <= 0 {
            return Err(AppError::BadRequest("volume_number must be >= 1".into()));
        }
        numbered.push((num, vol));
    }
    numbered.sort_by_key(|(n, _)| *n);
    for w in numbered.windows(2) {
        if w[0].0 == w[1].0 {
            return Err(AppError::BadRequest(format!(
                "duplicate volume_number: {}",
                w[0].0
            )));
        }
    }
    Ok(numbered)
}

fn volume_title(series_title: &str, num: i64, vol: &ManualVolumeInput) -> String {
    vol.title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{series_title} {num}권"))
}

fn parse_date(value: Option<&str>) -> Option<NaiveDate> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(value, "%Y.%m.%d"))
        .or_else(|_| NaiveDate::parse_from_str(value, "%Y/%m/%d"))
        .ok()
}
