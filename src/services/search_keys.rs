//! Rebuild / apply series search aliases.

use uuid::Uuid;

use crate::{
    error::AppResult,
    repositories,
    search_text::auto_keys_for_title,
    state::AppState,
};

pub async fn rebuild_auto_aliases(state: &AppState, series_id: Uuid, title: &str) -> AppResult<()> {
    repositories::replace_auto_search_aliases(
        &state.pool,
        series_id,
        &auto_keys_for_title(title),
    )
    .await
}

pub async fn rebuild_all_auto_aliases(state: &AppState) -> AppResult<usize> {
    let rows = repositories::list_all_series_id_titles(&state.pool).await?;
    let mut n = 0;
    for (id, title) in rows {
        rebuild_auto_aliases(state, id, &title).await?;
        n += 1;
    }
    Ok(n)
}

pub async fn add_user_alias(state: &AppState, series_id: Uuid, alias: &str) -> AppResult<()> {
    let alias = alias.trim();
    if alias.is_empty() {
        return Ok(());
    }
    repositories::upsert_search_alias(&state.pool, series_id, alias, "user").await
}

pub async fn add_admin_alias(state: &AppState, series_id: Uuid, alias: &str) -> AppResult<()> {
    let alias = alias.trim();
    if alias.is_empty() {
        return Err(crate::error::AppError::BadRequest("alias required".into()));
    }
    if alias.chars().count() > 80 {
        return Err(crate::error::AppError::BadRequest("alias too long".into()));
    }
    repositories::find_series_by_id(&state.pool, series_id)
        .await?
        .ok_or_else(|| crate::error::AppError::NotFound("series not found".into()))?;
    repositories::upsert_search_alias(&state.pool, series_id, alias, "admin").await
}

pub async fn add_admin_aliases_batch(
    state: &AppState,
    alias: &str,
    series_ids: &[Uuid],
) -> AppResult<usize> {
    let alias = alias.trim();
    if alias.is_empty() {
        return Err(crate::error::AppError::BadRequest("alias required".into()));
    }
    if alias.chars().count() > 80 {
        return Err(crate::error::AppError::BadRequest("alias too long".into()));
    }
    if series_ids.is_empty() {
        return Err(crate::error::AppError::BadRequest(
            "series_ids required".into(),
        ));
    }
    if series_ids.len() > 50 {
        return Err(crate::error::AppError::BadRequest(
            "series_ids too many (max 50)".into(),
        ));
    }
    let mut unique = series_ids.to_vec();
    unique.sort();
    unique.dedup();
    for sid in &unique {
        repositories::find_series_by_id(&state.pool, *sid)
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound(format!("series {sid} not found")))?;
    }
    for sid in &unique {
        repositories::upsert_search_alias(&state.pool, *sid, alias, "admin").await?;
    }
    Ok(unique.len())
}

pub async fn merge_search_bundle(state: &AppState, series_ids: &[Uuid]) -> AppResult<Uuid> {
    repositories::merge_search_bundle(&state.pool, series_ids).await
}

pub async fn remove_alias(state: &AppState, series_id: Uuid, alias_id: Uuid) -> AppResult<()> {
    repositories::find_series_by_id(&state.pool, series_id)
        .await?
        .ok_or_else(|| crate::error::AppError::NotFound("series not found".into()))?;
    repositories::delete_search_alias(&state.pool, series_id, alias_id).await
}
