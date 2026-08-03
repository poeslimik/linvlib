use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::{CatalogRequestCreate, CatalogRequestItem, CatalogRequestReview},
    repositories::{self, CatalogRequestRow},
    state::AppState,
};

fn row_to_item(row: CatalogRequestRow) -> AppResult<CatalogRequestItem> {
    Ok(CatalogRequestItem {
        id: crate::models::parse_uuid(&row.id)?,
        user_id: crate::models::parse_uuid(&row.user_id)?,
        user_email: row.user_email,
        request_type: row.request_type,
        series_id: row
            .series_id
            .as_deref()
            .map(crate::models::parse_uuid)
            .transpose()?,
        series_title: row.series_title,
        title: row.title,
        author: row.author,
        publisher: row.publisher,
        aladin_series_id: row.aladin_series_id,
        note: row.note,
        status: row.status,
        admin_note: row.admin_note,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

pub async fn create_request(
    state: &AppState,
    user_id: Uuid,
    body: CatalogRequestCreate,
) -> AppResult<CatalogRequestItem> {
    let request_type = body.request_type.trim().to_lowercase();
    if !matches!(request_type.as_str(), "add" | "edit" | "delete" | "other") {
        return Err(AppError::BadRequest(
            "request_type must be add, edit, delete, or other".into(),
        ));
    }

    if matches!(request_type.as_str(), "edit" | "delete") && body.series_id.is_none() {
        return Err(AppError::BadRequest(
            "series_id is required for edit/delete requests".into(),
        ));
    }

    if request_type == "add" {
        let title = body.title.as_deref().map(str::trim).unwrap_or("");
        let aladin = body.aladin_series_id.as_deref().map(str::trim).unwrap_or("");
        if title.is_empty() && aladin.is_empty() {
            return Err(AppError::BadRequest(
                "title or aladin_series_id is required for add requests".into(),
            ));
        }
    }

    if request_type == "other" {
        let note = body.note.as_deref().map(str::trim).unwrap_or("");
        if note.is_empty() {
            return Err(AppError::BadRequest(
                "note is required for other requests".into(),
            ));
        }
    }

    if let Some(series_id) = body.series_id {
        repositories::find_series_by_id(&state.pool, series_id)
            .await?
            .ok_or_else(|| AppError::NotFound("series not found".into()))?;
    }

    let id = Uuid::new_v4();
    repositories::insert_catalog_request(
        &state.pool,
        id,
        user_id,
        &request_type,
        body.series_id,
        body.title.as_deref().map(str::trim).filter(|s| !s.is_empty()),
        body.author.as_deref().map(str::trim).filter(|s| !s.is_empty()),
        body.publisher
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
        body.aladin_series_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty()),
        body.note.as_deref().map(str::trim).filter(|s| !s.is_empty()),
    )
    .await?;

    repositories::find_catalog_request(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::Internal("failed to load request".into()))
        .and_then(row_to_item)
}

pub async fn list_mine(state: &AppState, user_id: Uuid) -> AppResult<Vec<CatalogRequestItem>> {
    repositories::list_catalog_requests_for_user(&state.pool, user_id)
        .await?
        .into_iter()
        .map(row_to_item)
        .collect()
}

pub async fn list_admin(
    state: &AppState,
    status: Option<&str>,
) -> AppResult<Vec<CatalogRequestItem>> {
    repositories::list_catalog_requests_admin(&state.pool, status)
        .await?
        .into_iter()
        .map(row_to_item)
        .collect()
}

pub async fn review(
    state: &AppState,
    id: Uuid,
    body: CatalogRequestReview,
) -> AppResult<CatalogRequestItem> {
    let status = body.status.trim().to_lowercase();
    if !matches!(status.as_str(), "approved" | "rejected") {
        return Err(AppError::BadRequest(
            "status must be approved or rejected".into(),
        ));
    }

    let existing = repositories::find_catalog_request(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound("request not found".into()))?;
    if existing.status != "pending" {
        return Err(AppError::BadRequest("request is already reviewed".into()));
    }

    repositories::update_catalog_request_status(
        &state.pool,
        id,
        &status,
        body.admin_note.as_deref().map(str::trim).filter(|s| !s.is_empty()),
    )
    .await?;

    repositories::find_catalog_request(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::Internal("failed to load request".into()))
        .and_then(row_to_item)
}
