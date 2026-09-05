//! First-login tour demo data (live snapshot of a fixed showcase account).

use crate::{
    error::{AppError, AppResult},
    models::TourDemoResponse,
    repositories,
    services::{series, tierlist},
    state::AppState,
};

/// Showcase account whose live reads/tierlist appear in the tour for every viewer.
const DEMO_USER_EMAIL: &str = "iskim070208@gmail.com";
/// Tearmoon Empire — fixed series used in tour step 2.
const TEARMOON_SERIES_ID: &str = "5cc17307-3bca-4206-9088-0f3c0f0d48df";

pub async fn get_demo(state: &AppState) -> AppResult<TourDemoResponse> {
    let user = repositories::find_by_email(&state.pool, DEMO_USER_EMAIL)
        .await?
        .ok_or_else(|| AppError::NotFound("tour demo user not found".into()))?;

    let series_id = uuid::Uuid::parse_str(TEARMOON_SERIES_ID)
        .map_err(|_| AppError::Internal("invalid tour series id".into()))?;

    let tearmoon = series::get_series_detail(state, user.id, series_id, "desc").await?;
    let tierlist = tierlist::get_tierlist_snapshot(state, user.id).await?;

    Ok(TourDemoResponse {
        display_email: "example@linvlib.cloud".into(),
        demo_user_email: DEMO_USER_EMAIL.into(),
        tearmoon,
        tierlist,
    })
}
