use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::Response,
};
use tokio::fs::File;
use tokio_util::io::ReaderStream;

use crate::{
    auth::AdminUser,
    error::{AppError, AppResult},
    models::{AdminStatusResponse, BatchSearchAliasRequest, BatchSearchBundleRequest, UserResponse},
    repositories,
    services::{backup, catalog, new_releases, quota, search_keys, yes24_limit},
    state::AppState,
};

pub async fn status(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
) -> AppResult<Json<AdminStatusResponse>> {
    let pending = repositories::list_catalog_requests_admin(&state.pool, Some("pending"))
        .await?
        .len() as i64;
    let pending_suggestions = repositories::count_pending_new_release_suggestions(&state.pool).await?;
    let users = repositories::list_users(&state.pool).await?;
    let manual_series_count = repositories::count_manual_series(&state.pool).await?;
    let backups = backup::list_backups(&state).await.unwrap_or_default();
    let yes24_q = yes24_limit::status_today(&state).await?;
    Ok(Json(AdminStatusResponse {
        server_time_kst: quota::seoul_now_display(),
        last_refresh_at: repositories::get_app_meta(&state.pool, "last_refresh_at").await?,
        last_refresh_note: repositories::get_app_meta(&state.pool, "last_refresh_note").await?,
        refresh_running: new_releases::is_running_persisted(&state).await,
        last_scheduled_refresh_date: repositories::get_app_meta(
            &state.pool,
            "last_scheduled_refresh_date",
        )
        .await?,
        last_backup_at: repositories::get_app_meta(&state.pool, "last_backup_at").await?,
        backup_count: backups.len() as i64,
        backup_retain_days: state.config.backup_retain_days,
        pending_requests: pending,
        pending_suggestions,
        user_count: users.len() as i64,
        manual_series_count,
        yes24_quota_used: yes24_q.used,
        yes24_quota_soft_limit: yes24_q.soft_limit,
        yes24_quota_hard_limit: yes24_q.hard_limit,
        yes24_quota_date: yes24_q.usage_date,
    }))
}

pub async fn list_manual_series(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
) -> AppResult<Json<Vec<crate::models::AdminManualSeriesItem>>> {
    let items = repositories::list_manual_series(&state.pool).await?;
    Ok(Json(items))
}

pub async fn list_users(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
) -> AppResult<Json<Vec<UserResponse>>> {
    let users = repositories::list_users(&state.pool).await?;
    Ok(Json(users.into_iter().map(UserResponse::from).collect()))
}

pub async fn move_volumes(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Json(body): Json<crate::models::MoveVolumesRequest>,
) -> AppResult<Json<crate::models::ImportResponse>> {
    let response = crate::services::catalog_edit::move_volumes(&state, body).await?;
    Ok(Json(response))
}

pub async fn merge_series(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Json(body): Json<crate::models::MergeSeriesRequest>,
) -> AppResult<Json<crate::models::ImportResponse>> {
    let response = crate::services::catalog_edit::merge_series(&state, body).await?;
    Ok(Json(response))
}

pub async fn split_volumes(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Json(body): Json<crate::models::SplitVolumesRequest>,
) -> AppResult<Json<crate::models::ImportResponse>> {
    let response = crate::services::catalog_edit::split_volumes(&state, body).await?;
    Ok(Json(response))
}

pub async fn trigger_refresh(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
) -> AppResult<Json<crate::models::RefreshStartResponse>> {
    let response =
        new_releases::start_background(state, new_releases::RefreshOrigin::Manual).await?;
    Ok(Json(response))
}

pub async fn list_new_release_suggestions(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
) -> AppResult<Json<Vec<crate::models::NewReleaseSuggestionItem>>> {
    let items = repositories::list_new_release_suggestions(&state.pool, Some("pending")).await?;
    Ok(Json(items))
}

pub async fn dismiss_new_release_suggestion(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Path(id): Path<uuid::Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    repositories::set_new_release_suggestion_status(&state.pool, id, "dismissed").await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn import_new_release_suggestion(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Path(id): Path<uuid::Uuid>,
) -> AppResult<Json<crate::models::ImportResponse>> {
    let suggestion = repositories::find_new_release_suggestion(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound("suggestion not found".into()))?;
    if suggestion.status == "imported" {
        return Err(AppError::BadRequest("already imported".into()));
    }
    let response = catalog::import_series(
        &state,
        suggestion.aladin_series_id.clone(),
        if suggestion.aladin_series_id.is_some() {
            Vec::new()
        } else {
            vec![suggestion.sample_item_id.clone()]
        },
        Some(suggestion.title.clone()),
    )
    .await?;
    repositories::set_new_release_suggestion_status(&state.pool, id, "imported").await?;
    Ok(Json(response))
}

pub async fn list_backups(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
) -> AppResult<Json<Vec<backup::BackupInfo>>> {
    Ok(Json(backup::list_backups(&state).await?))
}

pub async fn create_backup(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
) -> AppResult<Json<backup::BackupInfo>> {
    Ok(Json(backup::create_backup(&state).await?))
}

pub async fn delete_backup(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Path(name): Path<String>,
) -> AppResult<StatusCode> {
    backup::delete_backup(&state, &name).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn download_backup(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Path(name): Path<String>,
) -> AppResult<Response> {
    let path = backup::backup_path(&state, &name)?;
    if !path.exists() {
        return Err(AppError::NotFound(format!("backup {name}")));
    }
    let file = File::open(&path)
        .await
        .map_err(|e| AppError::Internal(format!("open backup: {e}")))?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);
    let mut res = Response::new(body);
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    let disposition = format!("attachment; filename=\"{name}\"");
    res.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition)
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );
    Ok(res)
}

pub async fn batch_search_aliases(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Json(body): Json<BatchSearchAliasRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let count =
        search_keys::add_admin_aliases_batch(&state, &body.alias, &body.series_ids).await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "alias": body.alias.trim(),
        "series_count": count,
    })))
}

pub async fn create_search_bundle(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Json(body): Json<BatchSearchBundleRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let bundle_id = search_keys::merge_search_bundle(&state, &body.series_ids).await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "bundle_id": bundle_id,
        "series_count": body.series_ids.len(),
    })))
}
