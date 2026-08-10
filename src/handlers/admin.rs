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
    services::{aladin, backup, quota, search_keys},
    state::AppState,
};

pub async fn status(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
) -> AppResult<Json<AdminStatusResponse>> {
    let (quota_date, quota_used, quota_soft, quota_hard) = quota::usage_snapshot(&state).await?;
    let recent = repositories::list_recent_aladin_quota(&state.pool, 7).await?;
    let pending = repositories::list_catalog_requests_admin(&state.pool, Some("pending"))
        .await?
        .len() as i64;
    let users = repositories::list_users(&state.pool).await?;
    let manual_series_count = repositories::count_manual_series(&state.pool).await?;
    let backups = backup::list_backups(&state).await.unwrap_or_default();
    Ok(Json(AdminStatusResponse {
        quota_date: quota_date.clone(),
        quota_used,
        quota_soft,
        quota_hard,
        quota_remaining: (quota_soft - quota_used).max(0),
        server_time_kst: quota::seoul_now_display(),
        recent_quota: recent
            .into_iter()
            .map(|(date, used)| crate::models::AdminQuotaDay { date, used })
            .collect(),
        last_refresh_at: repositories::get_app_meta(&state.pool, "last_refresh_at").await?,
        last_refresh_note: repositories::get_app_meta(&state.pool, "last_refresh_note").await?,
        last_scheduled_refresh_date: repositories::get_app_meta(
            &state.pool,
            "last_scheduled_refresh_date",
        )
        .await?,
        last_backup_at: repositories::get_app_meta(&state.pool, "last_backup_at").await?,
        backup_count: backups.len() as i64,
        backup_retain_days: state.config.backup_retain_days,
        pending_requests: pending,
        user_count: users.len() as i64,
        manual_series_count,
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
) -> AppResult<Json<crate::models::BulkRefreshResponse>> {
    let response = aladin::refresh_all_aladin(&state).await?;
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
