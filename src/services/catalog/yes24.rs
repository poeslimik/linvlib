//! Yes24 Open API (`apis.yes24.com`).
//!
//! - Search/import: Goods `itemList`
//! - New releases: Category `newproduct` (LN categories) + imprint `RECENT` search
//! - LN-only filtering: [`group::CatalogVolume::is_catalog_candidate`]
//! - Call limits: [`crate::services::yes24_limit`] (10 RPS / soft daily cap)

use chrono::{Duration, NaiveDate, TimeZone, Utc};
use serde::Deserialize;
use uuid::Uuid;

use super::group::{self, CatalogVolume, GroupedSeries, LN_IMPRINTS};
use super::{CatalogItem, ProviderId};
use crate::{
    error::{AppError, AppResult},
    models::ImportResponse,
    repositories,
    services::quota,
    state::AppState,
};

const YES24_ITEM_LIST_URL: &str = "https://apis.yes24.com/v1/goods/itemList";
const YES24_NEW_PRODUCT_URL: &str = "https://apis.yes24.com/v1/category/newproduct";
const PAGE_SIZE: i64 = 50;
const MAX_SEARCH_PAGES: i64 = 3;
const MAX_NEW_PAGES: i64 = 5;

/// Domestic comics/LN parent + eBook light-novel category ids on Yes24.
const LN_CATEGORY_IDS: &[&str] = &[
    "001001008", // 국내도서 > 만화/라이트노벨
    "017001063", // eBook > 라이트노벨
];

#[derive(Debug, Deserialize)]
struct Yes24Envelope {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    message: Option<String>,
    #[serde(default, rename = "errorCode")]
    error_code: Option<String>,
    #[serde(default)]
    data: Option<Yes24ListData>,
}

#[derive(Debug, Deserialize)]
struct Yes24ListData {
    #[serde(default)]
    items: Vec<Yes24Item>,
    #[serde(default, rename = "pageSize")]
    page_size: i64,
    #[serde(default, rename = "totalCount")]
    total_count: i64,
}

#[derive(Debug, Deserialize)]
struct Yes24Item {
    #[serde(default, rename = "itemId", deserialize_with = "deserialize_item_id")]
    item_id: i64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    publisher: String,
    #[serde(default)]
    isbn13: String,
    #[serde(default, rename = "publishDate")]
    publish_date: String,
    #[serde(default)]
    cover: String,
    #[serde(default, rename = "goodsSortNm")]
    goods_sort_nm: String,
}

fn deserialize_item_id<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    use std::fmt;

    struct ItemIdVisitor;
    impl<'de> Visitor<'de> for ItemIdVisitor {
        type Value = i64;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("item id as integer or string")
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<i64, E> {
            Ok(v)
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<i64, E> {
            i64::try_from(v).map_err(E::custom)
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<i64, E> {
            v.trim().parse().map_err(E::custom)
        }

        fn visit_none<E: de::Error>(self) -> Result<i64, E> {
            Ok(0)
        }

        fn visit_unit<E: de::Error>(self) -> Result<i64, E> {
            Ok(0)
        }
    }

    deserializer.deserialize_any(ItemIdVisitor)
}

pub async fn search_grouped_series(
    state: &AppState,
    query: &str,
) -> AppResult<Vec<GroupedSeries>> {
    let items = search_as_catalog_volumes(state, query).await?;
    if items.is_empty() {
        return Ok(vec![]);
    }
    Ok(group::group_by_series_with(items, state.title_rules.as_ref()))
}

pub async fn import_series(
    state: &AppState,
    series_key: Option<String>,
    title_hint: Option<String>,
) -> AppResult<ImportResponse> {
    let series_key = series_key
        .ok_or_else(|| AppError::BadRequest("series key is required".into()))?;
    if series_key.starts_with("item:") || series_key.starts_with("manual:") {
        return Err(AppError::NotFound("series not found in yes24".into()));
    }

    let search_query = search_query_from_key(&series_key, title_hint.as_deref())?;
    let mut items = search_as_catalog_volumes(state, &search_query).await?;
    for alt in state
        .title_rules
        .alternate_search_queries(&search_query, title_hint.as_deref())
    {
        if let Ok(extra) = search_as_catalog_volumes(state, &alt).await {
            items.extend(extra);
        }
    }
    let mut seen = std::collections::HashSet::new();
    items.retain(|item| seen.insert(group::volume_external_id(item)));

    let groups = group::group_by_series_with(items, state.title_rules.as_ref());
    let group = group::select_group(&groups, &series_key, &search_query)
        .cloned()
        .ok_or_else(|| AppError::NotFound("series not found in yes24".into()))?;

    persist_group(state, &series_key, group).await
}

async fn persist_group(
    state: &AppState,
    series_key: &str,
    group: GroupedSeries,
) -> AppResult<ImportResponse> {
    let mut dates: Vec<_> = group
        .items
        .iter()
        .filter_map(|i| group::parse_pub_date(&i.pub_date))
        .collect();
    dates.sort();
    let first_published_at = dates
        .first()
        .copied()
        .map(|d| Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap()));
    let latest_published_at = dates
        .last()
        .copied()
        .map(|d| Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap()));

    let existing = repositories::find_series_by_aladin_id(&state.pool, series_key)
        .await?
        .or(repositories::find_series_by_aladin_id(&state.pool, &group.aladin_series_id).await?);

    let series = if let Some(existing) = existing {
        existing
    } else {
        repositories::upsert_series_with_cover(
            &state.pool,
            Uuid::new_v4(),
            &group.title,
            group.author.as_deref(),
            group.publisher.as_deref(),
            &group.aladin_series_id,
            first_published_at,
            latest_published_at,
            None,
        )
        .await?
    };

    let existing_count = repositories::count_volumes(&state.pool, series.id).await?;
    if existing_count == 0 {
        let numbered = group::assign_volume_numbers(&group.items);
        for (item, volume_number) in &numbered {
            repositories::upsert_volume(
                &state.pool,
                Uuid::new_v4(),
                series.id,
                *volume_number,
                &group::clean_volume_title(&item.title),
                group::non_empty(&item.cover).as_deref(),
                group::parse_pub_date(&item.pub_date),
                &group::volume_external_id(item),
                if item.isbn13.is_empty() {
                    None
                } else {
                    Some(item.isbn13.as_str())
                },
            )
            .await?;
        }
    } else {
        let mut next = repositories::max_volume_number(&state.pool, series.id).await? + 1;
        for item in &group.items {
            let item_id = group::volume_external_id(item);
            let exists = repositories::find_volume_by_aladin_item_id(&state.pool, &item_id)
                .await?
                .is_some();
            if exists {
                repositories::upsert_volume(
                    &state.pool,
                    Uuid::new_v4(),
                    series.id,
                    0,
                    &group::clean_volume_title(&item.title),
                    group::non_empty(&item.cover).as_deref(),
                    group::parse_pub_date(&item.pub_date),
                    &item_id,
                    if item.isbn13.is_empty() {
                        None
                    } else {
                        Some(item.isbn13.as_str())
                    },
                )
                .await?;
                continue;
            }
            repositories::upsert_volume(
                &state.pool,
                Uuid::new_v4(),
                series.id,
                next,
                &group::clean_volume_title(&item.title),
                group::non_empty(&item.cover).as_deref(),
                group::parse_pub_date(&item.pub_date),
                &item_id,
                if item.isbn13.is_empty() {
                    None
                } else {
                    Some(item.isbn13.as_str())
                },
            )
            .await?;
            next += 1;
        }
    }

    group::drop_bundle_volumes(state, series.id).await?;
    group::collapse_duplicate_volumes_by_identity(state, series.id).await?;
    group::reconcile_volume_numbers_from_titles(state, series.id).await?;
    repositories::refresh_series_publish_dates(&state.pool, series.id).await?;
    repositories::touch_series_refreshed(&state.pool, series.id).await?;

    let volume_count = repositories::count_volumes(&state.pool, series.id).await?;
    Ok(ImportResponse {
        series_id: series.id,
        title: series.title,
        volume_count,
    })
}

fn search_query_from_key(series_key: &str, title_hint: Option<&str>) -> AppResult<String> {
    if let Some(hint) = title_hint.map(str::trim).filter(|s| !s.is_empty()) {
        return Ok(group::normalize_series_title(hint));
    }
    if let Some(title) = series_key.strip_prefix("title:") {
        let query = group::normalize_series_title(title);
        if !query.is_empty() {
            return Ok(query);
        }
    }
    Err(AppError::BadRequest(
        "title is required to import this series".into(),
    ))
}

pub async fn search_as_catalog_volumes(
    state: &AppState,
    query: &str,
) -> AppResult<Vec<CatalogVolume>> {
    Ok(search_items(state, query)
        .await?
        .into_iter()
        .map(CatalogItem::into_catalog_volume)
        .filter(|item| item.is_catalog_candidate())
        .collect())
}

pub async fn search_items(state: &AppState, query: &str) -> AppResult<Vec<CatalogItem>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(vec![]);
    }

    let mut items = search_items_raw(state, query, "BOOK", "RELATION").await?;
    if items.len() < 10 {
        let extra = search_items_raw(state, query, "ALL", "DEFAULT").await?;
        items.extend(extra);
    }
    for alt in state.title_rules.alternate_search_queries(query, None) {
        if alt == query {
            continue;
        }
        items.extend(search_items_raw(state, &alt, "BOOK", "RELATION").await?);
    }

    let mut seen = std::collections::HashSet::new();
    items.retain(|item| seen.insert(item.item_key.clone()));
    Ok(items)
}

/// New releases: LN category newproduct + recent imprint searches (~3 KST days).
pub async fn fetch_new_release_items(state: &AppState) -> AppResult<Vec<CatalogVolume>> {
    let today = quota::seoul_today_date();
    let start = today - Duration::days(2);

    let mut all = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for category_id in LN_CATEGORY_IDS {
        let batch = fetch_category_new_products(state, category_id).await?;
        for item in batch {
            if !is_recent_enough(&item.pub_date, start, today) {
                continue;
            }
            if seen.insert(item.item_key.clone()) {
                all.push(item);
            }
        }
    }

    for imprint in LN_IMPRINTS {
        let batch = search_items_raw(state, imprint, "BOOK", "RECENT").await?;
        for item in batch {
            if !publisher_matches_imprint(&item.publisher, imprint) {
                continue;
            }
            if !is_recent_enough(&item.pub_date, start, today) {
                continue;
            }
            if seen.insert(item.item_key.clone()) {
                all.push(item);
            }
        }
    }

    Ok(all
        .into_iter()
        .map(CatalogItem::into_catalog_volume)
        .filter(|item| item.is_catalog_candidate())
        .collect())
}

async fn fetch_category_new_products(
    state: &AppState,
    category_id: &str,
) -> AppResult<Vec<CatalogItem>> {
    let mut items = Vec::new();
    let mut page = 1i64;
    loop {
        state.yes24_limiter.acquire(state).await?;
        let response = state
            .http
            .get(YES24_NEW_PRODUCT_URL)
            .header("X-Api-Key", state.config.yes24_api_key.as_str())
            .query(&[
                ("categoryId", category_id),
                ("sort", "RegDate"),
                ("page", &page.to_string()),
                ("pageSize", &PAGE_SIZE.to_string()),
                ("detail", "N"),
            ])
            .send()
            .await?;

        let (batch, total, page_size) = parse_list_response(response).await?;
        let batch_len = batch.len();
        items.extend(batch.into_iter().filter_map(item_to_catalog));

        if batch_len == 0 || page >= MAX_NEW_PAGES {
            break;
        }
        if total > 0 && (page * page_size.max(1)) >= total {
            break;
        }
        if (batch_len as i64) < PAGE_SIZE {
            break;
        }
        page += 1;
    }
    Ok(items)
}

async fn search_items_raw(
    state: &AppState,
    query: &str,
    category: &str,
    sort: &str,
) -> AppResult<Vec<CatalogItem>> {
    let mut items = Vec::new();
    let mut page = 1i64;
    loop {
        state.yes24_limiter.acquire(state).await?;
        let response = state
            .http
            .get(YES24_ITEM_LIST_URL)
            .header("X-Api-Key", state.config.yes24_api_key.as_str())
            .query(&[
                ("query", query),
                ("category", category),
                ("sort", sort),
                ("page", &page.to_string()),
                ("pageSize", &PAGE_SIZE.to_string()),
                ("detail", "N"),
            ])
            .send()
            .await?;

        let (batch, total, page_size) = parse_list_response(response).await?;
        let batch_len = batch.len();
        items.extend(batch.into_iter().filter_map(item_to_catalog));

        if batch_len == 0 || page >= MAX_SEARCH_PAGES {
            break;
        }
        if total > 0 && (page * page_size.max(1)) >= total {
            break;
        }
        if (batch_len as i64) < PAGE_SIZE {
            break;
        }
        page += 1;
    }
    Ok(items)
}

async fn parse_list_response(
    response: reqwest::Response,
) -> AppResult<(Vec<Yes24Item>, i64, i64)> {
    let status = response.status();
    let parsed: Yes24Envelope = response.json().await.map_err(|err| {
        AppError::Internal(format!("Yes24 API returned invalid JSON: {err}"))
    })?;

    let error_code = parsed.error_code.as_deref().unwrap_or("");
    if matches!(error_code, "SEARCH_001" | "NEW_001" | "GOODS_001" | "GOODS_002") {
        return Ok((Vec::new(), 0, PAGE_SIZE));
    }
    if status.as_u16() == 404 {
        return Ok((Vec::new(), 0, PAGE_SIZE));
    }

    if !parsed.success || !status.is_success() {
        let message = parsed
            .message
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| status.to_string());
        let code = if error_code.is_empty() {
            String::new()
        } else {
            format!(" ({error_code})")
        };
        return Err(AppError::Internal(format!("Yes24 API{code}: {message}")));
    }

    let data = parsed.data.unwrap_or(Yes24ListData {
        items: Vec::new(),
        page_size: PAGE_SIZE,
        total_count: 0,
    });
    Ok((data.items, data.total_count, data.page_size.max(1)))
}

fn item_to_catalog(item: Yes24Item) -> Option<CatalogItem> {
    let title = item.title.trim();
    if title.is_empty() {
        return None;
    }
    let isbn13 = normalize_isbn13(&item.isbn13).unwrap_or_default();
    let item_key = if !isbn13.is_empty() {
        format!("isbn:{isbn13}")
    } else if item.item_id > 0 {
        format!("yes24:{}", item.item_id)
    } else {
        return None;
    };

    let category_name = non_empty_owned(&item.goods_sort_nm);
    Some(CatalogItem {
        provider: ProviderId::Yes24,
        item_key,
        title: title.to_string(),
        author: item.author.trim().to_string(),
        publisher: item.publisher.trim().to_string(),
        pub_date: parse_publish_date(&item.publish_date),
        isbn13,
        cover: item.cover.trim().to_string(),
        yes24_item_id: item.item_id,
        category_name,
    })
}

fn normalize_isbn13(raw: &str) -> Option<String> {
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() == 13 && (digits.starts_with("978") || digits.starts_with("979")) {
        Some(digits)
    } else if digits.len() == 13 {
        Some(digits)
    } else {
        None
    }
}

fn parse_publish_date(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.len() >= 10 && trimmed.as_bytes().get(4) == Some(&b'-') {
        return trimmed[..10].to_string();
    }
    let digits: String = trimmed.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 8 {
        let y = &digits[0..4];
        let m = &digits[4..6];
        let d = &digits[6..8];
        return format!("{y}-{m}-{d}");
    }
    String::new()
}

fn is_recent_enough(pub_date: &str, start: NaiveDate, end: NaiveDate) -> bool {
    match group::parse_pub_date(pub_date) {
        Some(d) => d >= start && d <= end,
        None => false,
    }
}

fn publisher_matches_imprint(publisher: &str, imprint: &str) -> bool {
    publisher.contains(imprint)
}

fn non_empty_owned(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_isbn13() {
        assert_eq!(
            normalize_isbn13("978-89-6626-095-9").as_deref(),
            Some("9788966260959")
        );
        assert_eq!(normalize_isbn13("").as_deref(), None);
    }

    #[test]
    fn parses_publish_dates() {
        assert_eq!(parse_publish_date("2013-12-24"), "2013-12-24");
        assert_eq!(parse_publish_date("20131224"), "2013-12-24");
        assert_eq!(parse_publish_date(""), "");
    }

    #[test]
    fn item_key_prefers_isbn() {
        let item = item_to_catalog(Yes24Item {
            item_id: 123,
            title: "테스트 1".into(),
            author: "작가".into(),
            publisher: "시프트노벨".into(),
            isbn13: "9788966260959".into(),
            publish_date: "2024-01-01".into(),
            cover: "https://image.yes24.com/goods/123/L".into(),
            goods_sort_nm: "라이트노벨".into(),
        })
        .unwrap();
        assert_eq!(item.item_key, "isbn:9788966260959");
        assert_eq!(item.yes24_item_id, 123);
    }

    #[test]
    fn item_key_falls_back_to_yes24_id() {
        let item = item_to_catalog(Yes24Item {
            item_id: 99,
            title: "테스트".into(),
            author: String::new(),
            publisher: String::new(),
            isbn13: String::new(),
            publish_date: String::new(),
            cover: String::new(),
            goods_sort_nm: String::new(),
        })
        .unwrap();
        assert_eq!(item.item_key, "yes24:99");
    }
}
