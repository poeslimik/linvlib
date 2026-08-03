use crate::{
    error::AppResult,
    models::SearchResultItem,
    repositories,
    state::AppState,
};

pub async fn search(state: &AppState, query: &str) -> AppResult<Vec<SearchResultItem>> {
    if query.trim().is_empty() {
        return Ok(vec![]);
    }

    let rows = repositories::search_series(&state.pool, query.trim()).await?;
    let items = rows
        .into_iter()
        .map(|(id, title, cover, score)| SearchResultItem {
            id,
            title,
            latest_cover_url: cover,
            score,
        })
        .collect();

    Ok(items)
}
