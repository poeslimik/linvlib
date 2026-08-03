use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::{AdminUser, AuthUser},
    error::AppResult,
    models::{CatalogRequestCreate, CatalogRequestReview},
    services::catalog_requests,
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct RequestListQuery {
    #[serde(default)]
    pub status: Option<String>,
}

pub async fn create_request(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<CatalogRequestCreate>,
) -> AppResult<Json<crate::models::CatalogRequestItem>> {
    let item = catalog_requests::create_request(&state, user.id, body).await?;
    Ok(Json(item))
}

pub async fn list_my_requests(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> AppResult<Json<Vec<crate::models::CatalogRequestItem>>> {
    let items = catalog_requests::list_mine(&state, user.id).await?;
    Ok(Json(items))
}

pub async fn list_admin_requests(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Query(query): Query<RequestListQuery>,
) -> AppResult<Json<Vec<crate::models::CatalogRequestItem>>> {
    let items =
        catalog_requests::list_admin(&state, query.status.as_deref()).await?;
    Ok(Json(items))
}

pub async fn review_request(
    State(state): State<AppState>,
    AdminUser(_admin): AdminUser,
    Path(id): Path<Uuid>,
    Json(body): Json<CatalogRequestReview>,
) -> AppResult<Json<crate::models::CatalogRequestItem>> {
    let item = catalog_requests::review(&state, id, body).await?;
    Ok(Json(item))
}
