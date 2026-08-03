use axum::{Json, extract::State};

use crate::{
    auth::AuthUser,
    error::AppResult,
    models::SaveTierlistRequest,
    services::tierlist as tierlist_service,
    state::AppState,
};

pub async fn get_tierlist(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> AppResult<Json<crate::models::TierlistResponse>> {
    let response = tierlist_service::get_tierlist(&state, user.id).await?;
    Ok(Json(response))
}

pub async fn save_tierlist(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<SaveTierlistRequest>,
) -> AppResult<Json<crate::models::TierlistResponse>> {
    let response = tierlist_service::save_tierlist(&state, user.id, body).await?;
    Ok(Json(response))
}
