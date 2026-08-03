use axum::{Json, extract::{Query, State}};
use serde::Deserialize;

use crate::{
    auth::AuthUser,
    error::AppResult,
    services::search,
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

pub async fn search(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    Query(query): Query<SearchQuery>,
) -> AppResult<Json<Vec<crate::models::SearchResultItem>>> {
    let results = search::search(&state, &query.q).await?;
    Ok(Json(results))
}
