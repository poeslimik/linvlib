use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::{
        Rating, SaveRatingRequest, SaveReadsRequest,         SeriesDetailResponse, SeriesListItem,
        SeriesListResponse, SeriesSearchAlias, SearchBundlePeer, VolumeWithRead,
    },
    repositories,
    state::AppState,
};

pub async fn get_series_detail(
    state: &AppState,
    user_id: Uuid,
    series_id: Uuid,
    order: &str,
) -> AppResult<SeriesDetailResponse> {
    let series = repositories::find_series_by_id(&state.pool, series_id)
        .await?
        .ok_or_else(|| AppError::NotFound("series not found".into()))?;

    let volumes = repositories::list_volumes(&state.pool, series_id, order).await?;
    let read_map = repositories::get_read_map(&state.pool, user_id, series_id).await?;
    let rating = repositories::get_rating(&state.pool, user_id, series_id).await?;

    let volume_items: Vec<VolumeWithRead> = volumes
        .iter()
        .map(|v| VolumeWithRead {
            id: v.id,
            volume_number: v.volume_number,
            title: v.title.clone(),
            cover_url: v.cover_url.clone(),
            published_at: v.published_at,
            is_read: read_map.get(&v.id).copied().unwrap_or(false),
            is_unreleased: v.is_unreleased,
            label: v.label.clone(),
        })
        .collect();

    let total_volumes = volume_items.len() as i64;
    let read_volumes = volume_items.iter().filter(|v| v.is_read).count() as i64;
    let progress_percent = if total_volumes == 0 {
        0
    } else {
        ((read_volumes as f64 / total_volumes as f64) * 100.0).round() as i32
    };

    let latest_cover_url = repositories::latest_cover_for_series(&state.pool, series_id).await?;

    let is_manual = crate::services::manual::is_manual_series_id(&series.aladin_series_id);

    let search_aliases = repositories::list_search_aliases(&state.pool, series_id)
        .await?
        .into_iter()
        .map(|(id, alias, source)| SeriesSearchAlias { id, alias, source })
        .collect();

    let search_bundle_peers = repositories::list_search_bundle_peers(&state.pool, series_id)
        .await?
        .into_iter()
        .map(|(id, title)| SearchBundlePeer { id, title })
        .collect();

    Ok(SeriesDetailResponse {
        id: series.id,
        title: series.title,
        author: series.author,
        publisher: series.publisher,
        aladin_series_id: series.aladin_series_id,
        is_manual,
        publish_status: series.publish_status.clone(),
        first_published_at: series.first_published_at,
        latest_published_at: series.latest_published_at,
        cover_url: series.cover_url.clone(),
        latest_cover_url,
        rating,
        volumes: volume_items,
        total_volumes,
        read_volumes,
        progress_percent,
        search_aliases,
        search_bundle_peers,
    })
}

pub async fn save_reads(
    state: &AppState,
    user_id: Uuid,
    series_id: Uuid,
    body: SaveReadsRequest,
) -> AppResult<()> {
    repositories::find_series_by_id(&state.pool, series_id)
        .await?
        .ok_or_else(|| AppError::NotFound("series not found".into()))?;

    let reads: Vec<(Uuid, bool)> = body
        .reads
        .into_iter()
        .map(|entry| (entry.volume_id, entry.is_read))
        .collect();

    repositories::save_reads(&state.pool, user_id, series_id, &reads).await?;

    // 읽은 권이 없어지면 평가·티어리스트도 함께 제거
    let read_count =
        repositories::count_read_volumes(&state.pool, user_id, series_id).await?;
    if read_count == 0 {
        repositories::upsert_rating(&state.pool, user_id, series_id, &Rating::None).await?;
    }

    Ok(())
}

pub async fn save_rating(
    state: &AppState,
    user_id: Uuid,
    series_id: Uuid,
    body: SaveRatingRequest,
) -> AppResult<()> {
    repositories::find_series_by_id(&state.pool, series_id)
        .await?
        .ok_or_else(|| AppError::NotFound("series not found".into()))?;

    if body.rating != Rating::None {
        let read_count =
            repositories::count_read_volumes(&state.pool, user_id, series_id).await?;
        if read_count == 0 {
            return Err(AppError::BadRequest(
                "한 권 이상 읽은 뒤에 평가할 수 있습니다".into(),
            ));
        }
    }

    repositories::upsert_rating(&state.pool, user_id, series_id, &body.rating).await?;

    if body.rating != Rating::None {
        if let Some(tier) = body.rating.tier_value() {
            let entries = repositories::list_tierlist_entries(&state.pool, user_id).await?;
            let already_exists = entries.iter().any(|(_, id, _, _, _)| *id == series_id);
            if !already_exists {
                let position = entries
                    .iter()
                    .filter(|(t, _, _, _, _)| t == tier)
                    .map(|(_, _, _, _, pos)| *pos)
                    .max()
                    .map(|p| p + 1)
                    .unwrap_or(0);
                repositories::insert_tierlist_entries(
                    &state.pool,
                    user_id,
                    &[(series_id, tier.to_string(), position)],
                )
                .await?;
            }
        }
    }

    Ok(())
}

pub async fn list_series(
    state: &AppState,
    user_id: Uuid,
    sort: &str,
    order: &str,
    page: i64,
    limit: i64,
    q: &str,
    status: &str,
    read_f: &str,
    rated_f: &str,
    ps_in: &str,
    ps_ex: &str,
) -> AppResult<SeriesListResponse> {
    let page = page.max(1);
    let limit = limit.clamp(1, 100);
    let (rows, total) = repositories::list_series(
        &state.pool,
        user_id,
        sort,
        order,
        page,
        limit,
        q,
        status,
        read_f,
        rated_f,
        ps_in,
        ps_ex,
    )
    .await?;

    let items = rows
        .into_iter()
        .enumerate()
        .map(|(idx, row)| {
            let progress_percent = if row.total_volumes == 0 {
                0
            } else {
                ((row.read_volumes as f64 / row.total_volumes as f64) * 100.0).round() as i32
            };

            SeriesListItem {
                rank: (page - 1) * limit + idx as i64 + 1,
                id: row.id,
                title: row.title,
                latest_cover_url: row.latest_cover_url,
                total_volumes: row.total_volumes,
                read_volumes: row.read_volumes,
                progress_percent,
                publish_status: row.publish_status.clone(),
            }
        })
        .collect();

    Ok(SeriesListResponse {
        items,
        page,
        limit,
        total,
    })
}
