//! Book catalog (Yes24 Open API).
//!
//! Search/import map API hits to [`CatalogItem`], then group with title rules.
//! LN filtering lives in [`group::CatalogVolume::is_catalog_candidate`].

pub mod group;
pub mod yes24;

use crate::{
    error::AppResult,
    models::{ImportResponse, ImportSearchResult},
    repositories,
    services::catalog::group::CatalogVolume,
    state::AppState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderId {
    Yes24,
}

impl ProviderId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Yes24 => "yes24",
        }
    }
}

/// Provider-agnostic volume used for search grouping and import.
#[derive(Debug, Clone)]
pub struct CatalogItem {
    pub provider: ProviderId,
    pub item_key: String,
    pub title: String,
    pub author: String,
    pub publisher: String,
    pub pub_date: String,
    pub isbn13: String,
    pub cover: String,
    /// Yes24 goods number when known (used as numeric `item_id`).
    pub yes24_item_id: i64,
    pub category_name: Option<String>,
}

impl CatalogItem {
    pub fn into_catalog_volume(self) -> CatalogVolume {
        let item_id = if self.yes24_item_id > 0 {
            self.yes24_item_id
        } else {
            parse_synthetic_item_id(&self.item_key, &self.isbn13, &self.title)
        };
        CatalogVolume {
            title: self.title,
            author: self.author,
            publisher: self.publisher,
            pub_date: self.pub_date,
            isbn13: self.isbn13,
            item_id,
            cover: self.cover,
            category_name: self.category_name,
            stored_item_id: Some(self.item_key),
        }
    }
}

fn parse_synthetic_item_id(item_key: &str, isbn13: &str, title: &str) -> i64 {
    let digits = item_key
        .strip_prefix("isbn:")
        .or_else(|| item_key.strip_prefix("yes24:"))
        .unwrap_or(isbn13)
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>();
    if digits.len() >= 10 {
        if let Ok(n) = digits.parse::<i64>() {
            if n > 0 {
                return n;
            }
        }
    }
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    item_key.hash(&mut hasher);
    title.hash(&mut hasher);
    (hasher.finish() as i64).unsigned_abs() as i64
}

pub async fn import_search(state: &AppState, query: &str) -> AppResult<Vec<ImportSearchResult>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(vec![]);
    }

    let groups = yes24::search_grouped_series(state, query).await?;
    let mut results = Vec::new();
    for group in groups {
        let existing =
            repositories::find_series_by_aladin_id(&state.pool, &group.aladin_series_id).await?;
        results.push(ImportSearchResult {
            aladin_series_id: group.aladin_series_id,
            title: group.title,
            author: group.author,
            publisher: group.publisher,
            cover_url: group.cover_url,
            volume_count: group.items.len(),
            already_imported: existing.is_some(),
            series_id: existing.map(|s| s.id),
            sources: vec![ProviderId::Yes24.as_str().to_string()],
        });
    }
    Ok(results)
}

pub async fn import_series(
    state: &AppState,
    aladin_series_id: Option<String>,
    _seed_item_ids: Vec<String>,
    title: Option<String>,
) -> AppResult<ImportResponse> {
    // Seed item ids are unused. Import uses the series key and optional title.
    yes24::import_series(state, aladin_series_id, title).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_item_uses_yes24_item_id() {
        let vol = CatalogItem {
            provider: ProviderId::Yes24,
            item_key: "isbn:9788966260959".into(),
            title: "클린 코드".into(),
            author: "마틴".into(),
            publisher: "인사이트".into(),
            pub_date: "2013-12-24".into(),
            isbn13: "9788966260959".into(),
            cover: "https://image.yes24.com/goods/1/L".into(),
            yes24_item_id: 12345678,
            category_name: Some("국내도서".into()),
        }
        .into_catalog_volume();
        assert_eq!(vol.item_id, 12345678);
        assert_eq!(vol.stored_item_id.as_deref(), Some("isbn:9788966260959"));
        assert_eq!(vol.category_name.as_deref(), Some("국내도서"));
    }
}
