//! Admin catalog editing: any series meta/volumes, reorder, move, merge, split.

use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::{
        ImportResponse, ManualSeriesRequest, ManualVolumeInput, MergeSeriesRequest,
        MoveVolumesRequest, SplitVolumesRequest, VolumeOrderRequest,
    },
    repositories,
    services::manual,
    state::AppState,
};

pub async fn update_series(
    state: &AppState,
    series_id: Uuid,
    body: ManualSeriesRequest,
) -> AppResult<ImportResponse> {
    let existing = repositories::find_series_by_id(&state.pool, series_id)
        .await?
        .ok_or_else(|| AppError::NotFound("series not found".into()))?;

    let title = body.title.trim();
    if title.is_empty() {
        return Err(AppError::BadRequest("title is required".into()));
    }

    manual::ensure_title_available_pub(&state.pool, title, Some(series_id), body.force).await?;

    let series = repositories::update_series_meta(
        &state.pool,
        series_id,
        title,
        body.author.as_deref().map(str::trim).filter(|s| !s.is_empty()),
        body.publisher
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
        body.cover_url.as_deref(),
    )
    .await?;

    let _ = existing;

    if body.volumes.is_empty() && body.volume_count.is_none() {
        // Meta-only update: leave volumes unchanged
        repositories::refresh_series_publish_dates(&state.pool, series_id).await?;
        let volume_count = repositories::count_volumes(&state.pool, series_id).await?;
        return Ok(ImportResponse {
            series_id: series.id,
            title: series.title,
            volume_count,
        });
    }

    let mut numbered = manual::normalize_volumes_pub(title, body.volumes, body.volume_count)?;
    manual::apply_series_cover_pub(&mut numbered, body.cover_url.as_deref());

    let current = repositories::list_volumes(&state.pool, series_id, "asc").await?;
    let current_ids: HashSet<Uuid> = current.iter().map(|v| v.id).collect();
    let mut by_number: HashMap<i64, Uuid> = current
        .iter()
        .map(|v| (v.volume_number, v.id))
        .collect();

    let mut keep: HashSet<Uuid> = HashSet::new();
    let mut planned: Vec<(Uuid, i64, ManualVolumeInput, bool)> = Vec::new();

    for (num, vol) in numbered {
        let resolved = if let Some(id) = vol.id {
            if !current_ids.contains(&id) {
                return Err(AppError::BadRequest(format!(
                    "volume id {id} does not belong to this series"
                )));
            }
            by_number.retain(|_, vid| *vid != id);
            Some(id)
        } else {
            by_number.remove(&num)
        };

        if let Some(id) = resolved {
            keep.insert(id);
            planned.push((id, num, vol, false));
        } else {
            let id = Uuid::new_v4();
            planned.push((id, num, vol, true));
        }
    }

    const STAGE_BASE: i64 = -2_000_000;
    for (idx, vol) in current.iter().enumerate() {
        repositories::set_volume_number(&state.pool, vol.id, STAGE_BASE - idx as i64).await?;
    }

    for vol in &current {
        if !keep.contains(&vol.id) {
            repositories::delete_volume(&state.pool, vol.id, series_id).await?;
        }
    }

    for (id, num, vol, is_new) in planned {
        let vol_title = manual::volume_title_pub(title, num, &vol);
        let cover = vol
            .cover_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let published = manual::parse_date_pub(vol.published_at.as_deref());
        let label = vol
            .label
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if is_new {
            repositories::insert_manual_volume(
                &state.pool,
                id,
                series_id,
                num,
                &vol_title,
                cover,
                published,
                vol.is_unreleased,
                label,
            )
            .await?;
        } else {
            repositories::update_manual_volume(
                &state.pool,
                id,
                series_id,
                num,
                &vol_title,
                cover,
                published,
                vol.is_unreleased,
                label,
            )
            .await?;
        }
    }

    repositories::refresh_series_publish_dates(&state.pool, series_id).await?;
    let volume_count = repositories::count_volumes(&state.pool, series_id).await?;

    Ok(ImportResponse {
        series_id: series.id,
        title: series.title,
        volume_count,
    })
}

pub async fn reorder_volumes(
    state: &AppState,
    series_id: Uuid,
    body: VolumeOrderRequest,
) -> AppResult<ImportResponse> {
    let series = repositories::find_series_by_id(&state.pool, series_id)
        .await?
        .ok_or_else(|| AppError::NotFound("series not found".into()))?;

    if body.volume_ids.is_empty() {
        return Err(AppError::BadRequest("volume_ids required".into()));
    }

    repositories::renumber_volumes_in_order(&state.pool, series_id, &body.volume_ids).await?;
    let volume_count = repositories::count_volumes(&state.pool, series_id).await?;

    Ok(ImportResponse {
        series_id: series.id,
        title: series.title,
        volume_count,
    })
}

pub async fn move_volumes(state: &AppState, body: MoveVolumesRequest) -> AppResult<ImportResponse> {
    if body.volume_ids.is_empty() {
        return Err(AppError::BadRequest("volume_ids required".into()));
    }

    let target = repositories::find_series_by_id(&state.pool, body.target_series_id)
        .await?
        .ok_or_else(|| AppError::NotFound("target series not found".into()))?;

    let mut volumes = Vec::new();
    let mut source_ids: HashSet<Uuid> = HashSet::new();
    for id in &body.volume_ids {
        let vol = repositories::find_volume_by_id(&state.pool, *id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("volume {id} not found")))?;
        if vol.series_id == body.target_series_id {
            return Err(AppError::BadRequest(
                "volume already belongs to the target series".into(),
            ));
        }
        source_ids.insert(vol.series_id);
        volumes.push(vol);
    }

    // Stage target numbers then append
    let target_vols = repositories::list_volumes(&state.pool, body.target_series_id, "asc").await?;
    const STAGE_BASE: i64 = -2_000_000;
    for (idx, vol) in target_vols.iter().enumerate() {
        repositories::set_volume_number(&state.pool, vol.id, STAGE_BASE - idx as i64).await?;
    }
    for (idx, vol) in target_vols.iter().enumerate() {
        repositories::set_volume_number(&state.pool, vol.id, (idx + 1) as i64).await?;
    }

    // Stage source volumes being moved
    for (idx, vol) in volumes.iter().enumerate() {
        repositories::set_volume_number(&state.pool, vol.id, STAGE_BASE - 100_000 - idx as i64)
            .await?;
    }

    let mut next = repositories::max_volume_number(&state.pool, body.target_series_id).await? + 1;
    for vol in &volumes {
        repositories::move_volume_to_series(
            &state.pool,
            vol.id,
            vol.series_id,
            body.target_series_id,
            next,
        )
        .await?;
        next += 1;
    }

    for source_id in source_ids {
        // Compact remaining source volumes
        let remaining = repositories::list_volumes(&state.pool, source_id, "asc").await?;
        let ids: Vec<Uuid> = remaining.iter().map(|v| v.id).collect();
        if !ids.is_empty() {
            repositories::renumber_volumes_in_order(&state.pool, source_id, &ids).await?;
        }
        repositories::refresh_series_publish_dates(&state.pool, source_id).await?;
    }
    repositories::refresh_series_publish_dates(&state.pool, body.target_series_id).await?;

    let volume_count = repositories::count_volumes(&state.pool, body.target_series_id).await?;
    Ok(ImportResponse {
        series_id: target.id,
        title: target.title,
        volume_count,
    })
}

pub async fn merge_series(state: &AppState, body: MergeSeriesRequest) -> AppResult<ImportResponse> {
    if body.source_series_id == body.target_series_id {
        return Err(AppError::BadRequest(
            "source and target must differ".into(),
        ));
    }

    let source = repositories::find_series_by_id(&state.pool, body.source_series_id)
        .await?
        .ok_or_else(|| AppError::NotFound("source series not found".into()))?;
    let target = repositories::find_series_by_id(&state.pool, body.target_series_id)
        .await?
        .ok_or_else(|| AppError::NotFound("target series not found".into()))?;

    let source_vols =
        repositories::list_volumes(&state.pool, body.source_series_id, "asc").await?;
    let volume_ids: Vec<Uuid> = source_vols.iter().map(|v| v.id).collect();

    if !volume_ids.is_empty() {
        move_volumes(
            state,
            MoveVolumesRequest {
                volume_ids,
                target_series_id: body.target_series_id,
            },
        )
        .await?;
    }

    repositories::merge_series_ratings(
        &state.pool,
        body.source_series_id,
        body.target_series_id,
    )
    .await?;
    repositories::merge_series_tierlist_entries(
        &state.pool,
        body.source_series_id,
        body.target_series_id,
    )
    .await?;
    repositories::merge_series_tierlist_gatekeepers(
        &state.pool,
        body.source_series_id,
        body.target_series_id,
    )
    .await?;

    repositories::absorb_series_aladin_identities(
        &state.pool,
        body.source_series_id,
        body.target_series_id,
    )
    .await?;

    repositories::delete_series(&state.pool, body.source_series_id).await?;
    repositories::refresh_series_publish_dates(&state.pool, body.target_series_id).await?;

    let volume_count = repositories::count_volumes(&state.pool, body.target_series_id).await?;
    let _ = source;
    Ok(ImportResponse {
        series_id: target.id,
        title: target.title,
        volume_count,
    })
}

pub async fn split_volumes(
    state: &AppState,
    body: SplitVolumesRequest,
) -> AppResult<ImportResponse> {
    if body.volume_ids.is_empty() {
        return Err(AppError::BadRequest("volume_ids required".into()));
    }

    let title = body.title.trim();
    if title.is_empty() {
        return Err(AppError::BadRequest("title is required".into()));
    }

    manual::ensure_title_available_pub(&state.pool, title, None, body.force).await?;

    if let Some(first_id) = body.volume_ids.first() {
        if let Some(volume) = repositories::find_volume_by_id(&state.pool, *first_id).await? {
            let rules = state.title_rules.as_ref();
            let mut drop_keys = vec![format!(
                "title:{}",
                rules.normalize_series_title(title)
            )];
            for volume_id in &body.volume_ids {
                if let Some(vol) = repositories::find_volume_by_id(&state.pool, *volume_id).await? {
                    let key = format!("title:{}", rules.normalize_series_title(&vol.title));
                    if key != "title:" {
                        drop_keys.push(key);
                    }
                }
            }
            repositories::remove_series_aladin_aliases(&state.pool, volume.series_id, &drop_keys)
                .await?;
        }
    }

    let series_id = Uuid::new_v4();
    let aladin_series_id = format!("manual:{series_id}");
    let series = repositories::upsert_series(
        &state.pool,
        series_id,
        title,
        body.author.as_deref().map(str::trim).filter(|s| !s.is_empty()),
        body.publisher
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
        &aladin_series_id,
        None,
        None,
    )
    .await?;

    move_volumes(
        state,
        MoveVolumesRequest {
            volume_ids: body.volume_ids,
            target_series_id: series.id,
        },
    )
    .await
}
