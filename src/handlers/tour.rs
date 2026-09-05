use axum::{Json, extract::State};

use crate::{
    auth::AuthUser, error::AppResult, models::TourDemoResponse, services::tour, state::AppState,
};

/// Authenticated tour viewers receive a live snapshot of the showcase account.
pub async fn get_demo(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
) -> AppResult<Json<TourDemoResponse>> {
    Ok(Json(tour::get_demo(&state).await?))
}
