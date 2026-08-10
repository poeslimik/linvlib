use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::{AdminUser, AuthUser},
    error::AppResult,
    models::{ImportRequest, ManualSeriesRequest, SaveRatingRequest, SaveReadsRequest},
    repositories,
    services::{aladin, catalog_edit, manual, series},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct SeriesListQuery {
    #[serde(default = "default_sort")]
    pub sort: String,
    /// asc | desc (default desc)
    #[serde(default = "default_list_order")]
    pub order: String,
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub q: String,
    /// all | read | unread | unrated (legacy; prefer read_f / rated_f)
    #[serde(default = "default_status")]
    pub status: String,
    /// Tri-state read filter: in | ex | (empty = off)
    #[serde(default)]
    pub read_f: String,
    /// Tri-state rated filter: in | ex | (empty = off)
    #[serde(default)]
    pub rated_f: String,
    /// Comma-separated publish_status values to include
    #[serde(default)]
    pub ps_in: String,
    /// Comma-separated publish_status values to exclude
    #[serde(default)]
    pub ps_ex: String,
}

fn default_sort() -> String {
    "latest".to_string()
}
fn default_list_order() -> String {
    "desc".to_string()
}
fn default_page() -> i64 {
    1
}
fn default_limit() -> i64 {
    20
}
fn default_status() -> String {
    "all".to_string()
}

#[derive(Debug, Deserialize)]
pub struct VolumeOrderQuery {
    #[serde(default = "default_order")]
    pub order: String,
}

fn default_order() -> String {
    "desc".to_string()
}

pub async fn list_series(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Query(query): Query<SeriesListQuery>,
) -> AppResult<Json<crate::models::SeriesListResponse>> {
    let response = series::list_series(
        &state,
        user.id,
        &query.sort,
        &query.order,
        query.page,
        query.limit,
        &query.q,
        &query.status,
        &query.read_f,
        &query.rated_f,
        &query.ps_in,
        &query.ps_ex,
    )
    .await?;
    Ok(Json(response))
}

pub async fn get_series(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<Uuid>,
    Query(query): Query<VolumeOrderQuery>,
) -> AppResult<Json<crate::models::SeriesDetailResponse>> {
    let response = series::get_series_detail(&state, user.id, id, &query.order).await?;
    Ok(Json(response))
}

pub async fn save_reads(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SaveReadsRequest>,
) -> AppResult<Json<serde_json::Value>> {
    series::save_reads(&state, user.id, id, body).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn save_rating(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SaveRatingRequest>,
) -> AppResult<Json<serde_json::Value>> {
    series::save_rating(&state, user.id, id, body).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
pub struct ImportSearchQuery {
    pub q: String,
}

pub async fn import_search(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    Query(query): Query<ImportSearchQuery>,
) -> AppResult<Json<Vec<crate::models::ImportSearchResult>>> {
    let results = aladin::import_search(&state, &query.q).await?;
    Ok(Json(results))
}

pub async fn import_series(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Json(body): Json<ImportRequest>,
) -> AppResult<Json<crate::models::ImportResponse>> {
    let response = aladin::import_series(
        &state,
        body.aladin_series_id,
        body.seed_item_id,
        body.title,
    )
    .await?;
    Ok(Json(response))
}

pub async fn create_manual_series(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Json(body): Json<ManualSeriesRequest>,
) -> AppResult<Json<crate::models::ImportResponse>> {
    let response = manual::create_manual_series(&state, body).await?;
    Ok(Json(response))
}

pub async fn update_manual_series(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ManualSeriesRequest>,
) -> AppResult<Json<crate::models::ImportResponse>> {
    let response = catalog_edit::update_series(&state, id, body).await?;
    Ok(Json(response))
}

pub async fn reorder_volumes(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<crate::models::VolumeOrderRequest>,
) -> AppResult<Json<crate::models::ImportResponse>> {
    let response = catalog_edit::reorder_volumes(&state, id, body).await?;
    Ok(Json(response))
}

pub async fn delete_manual_series(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    manual::delete_manual_series(&state, id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn set_series_publish_status(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<crate::models::SetSeriesPublishStatusRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let series =
        repositories::set_series_publish_status(&state.pool, id, &body.publish_status).await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "id": series.id,
        "publish_status": series.publish_status,
    })))
}

pub async fn add_search_alias(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<crate::models::AddSearchAliasRequest>,
) -> AppResult<Json<serde_json::Value>> {
    crate::services::search_keys::add_admin_alias(&state, id, &body.alias).await?;
    let aliases = repositories::list_search_aliases(&state.pool, id)
        .await?
        .into_iter()
        .map(|(aid, alias, source)| {
            serde_json::json!({ "id": aid, "alias": alias, "source": source })
        })
        .collect::<Vec<_>>();
    Ok(Json(serde_json::json!({ "ok": true, "search_aliases": aliases })))
}

pub async fn delete_search_alias(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Path((id, alias_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<serde_json::Value>> {
    crate::services::search_keys::remove_alias(&state, id, alias_id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

