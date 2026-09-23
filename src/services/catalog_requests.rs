use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::{
        CatalogRequestCreate, CatalogRequestItem, CatalogRequestRelatedSeries, CatalogRequestReview,
    },
    repositories::{self, CatalogRequestRow},
    services::search_keys,
    state::AppState,
};

fn parse_related_ids(raw: Option<&str>) -> Vec<Uuid> {
    let Some(raw) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Vec::new();
    };
    if let Ok(ids) = serde_json::from_str::<Vec<Uuid>>(raw) {
        return ids;
    }
    raw.split(',')
        .filter_map(|part| Uuid::parse_str(part.trim()).ok())
        .collect()
}

async fn row_to_item(state: &AppState, row: CatalogRequestRow) -> AppResult<CatalogRequestItem> {
    let related_series_ids = parse_related_ids(row.related_series_ids.as_deref());
    let related_series = repositories::list_series_titles_by_ids(&state.pool, &related_series_ids)
        .await?
        .into_iter()
        .map(|(id, title)| CatalogRequestRelatedSeries { id, title })
        .collect();

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
        related_series_ids,
        related_series,
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
    if !matches!(
        request_type.as_str(),
        "add" | "edit" | "other" | "search_improve"
    ) {
        return Err(AppError::BadRequest(
            "request_type must be add, edit, other, or search_improve".into(),
        ));
    }

    if request_type == "edit" && body.series_id.is_none() {
        return Err(AppError::BadRequest(
            "series_id is required for edit requests".into(),
        ));
    }

    if request_type == "add" {
        let title = body.title.as_deref().map(str::trim).unwrap_or("");
        // JSON field `aladin_series_id` is the external series key (`title:…`, etc.).
        let series_key = body.aladin_series_id.as_deref().map(str::trim).unwrap_or("");
        if title.is_empty() && series_key.is_empty() {
            return Err(AppError::BadRequest(
                "title or series key is required for add requests".into(),
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

    let mut related_ids: Vec<Uuid> = body.series_ids.unwrap_or_default();
    related_ids.sort();
    related_ids.dedup();

    if request_type == "search_improve" {
        let query = body.title.as_deref().map(str::trim).unwrap_or("");
        if query.is_empty() {
            return Err(AppError::BadRequest(
                "title (search query) is required for search_improve".into(),
            ));
        }
        if related_ids.is_empty() {
            return Err(AppError::BadRequest(
                "series_ids is required for search_improve".into(),
            ));
        }
        if related_ids.len() > 20 {
            return Err(AppError::BadRequest(
                "series_ids too many (max 20)".into(),
            ));
        }
        for sid in &related_ids {
            repositories::find_series_by_id(&state.pool, *sid)
                .await?
                .ok_or_else(|| AppError::NotFound(format!("series {sid} not found")))?;
        }
    }

    if let Some(series_id) = body.series_id {
        repositories::find_series_by_id(&state.pool, series_id)
            .await?
            .ok_or_else(|| AppError::NotFound("series not found".into()))?;
    }

    let related_json = if related_ids.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&related_ids).unwrap_or_else(|_| "[]".into()))
    };

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
        related_json.as_deref(),
    )
    .await?;

    let row = repositories::find_catalog_request(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::Internal("failed to load request".into()))?;
    row_to_item(state, row).await
}

pub async fn list_mine(state: &AppState, user_id: Uuid) -> AppResult<Vec<CatalogRequestItem>> {
    let rows = repositories::list_catalog_requests_for_user(&state.pool, user_id).await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(row_to_item(state, row).await?);
    }
    Ok(out)
}

pub async fn list_admin(
    state: &AppState,
    status: Option<&str>,
) -> AppResult<Vec<CatalogRequestItem>> {
    let rows = repositories::list_catalog_requests_admin(&state.pool, status).await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(row_to_item(state, row).await?);
    }
    Ok(out)
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

    if status == "approved" && existing.request_type == "search_improve" {
        let query = existing
            .title
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::BadRequest("search_improve missing query".into()))?;
        let ids = parse_related_ids(existing.related_series_ids.as_deref());
        if ids.is_empty() {
            return Err(AppError::BadRequest(
                "search_improve missing series_ids".into(),
            ));
        }
        for sid in ids {
            search_keys::add_user_alias(state, sid, query).await?;
        }
    }

    let admin_note = body
        .admin_note
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    repositories::update_catalog_request_status(
        &state.pool,
        id,
        &status,
        admin_note.as_deref(),
    )
    .await?;

    let row = repositories::find_catalog_request(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::Internal("failed to load request".into()))?;
    row_to_item(state, row).await
}
