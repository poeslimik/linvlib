use std::collections::{HashMap, HashSet};

use chrono::{NaiveDate, TimeZone, Utc};
use regex::Regex;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::{ImportResponse, ImportSearchResult},
    repositories,
    services::title_rules::{default_rules, TitleRules},
    state::AppState,
};

fn rules() -> &'static TitleRules {
    default_rules()
}

fn rules_of(state: &AppState) -> &TitleRules {
    state.title_rules.as_ref()
}

const ALADIN_SEARCH_URL: &str = "https://www.aladin.co.kr/ttb/api/ItemSearch.aspx";
const ALADIN_LOOKUP_URL: &str = "https://www.aladin.co.kr/ttb/api/ItemLookUp.aspx";
/// 국내도서 > 만화/라이트노벨 > 라이트 노벨
const LIGHT_NOVEL_CATEGORY_ID: &str = "50927";

/// 전자책·종이책에 쓰이는 국내 LN 계열 브랜드(코믹 레이블 제외).
const LN_IMPRINTS: &[&str] = &[
    "시프트노벨",
    "노블엔진",
    "L노벨",
    "엘노벨",
    "에이디노벨라",
    "영상출판미디어",
    "소미미디어",
    "디앤씨미디어",
    "미르아이",
    "노블앤북스",
    "제이노블",
    "J Novel",
    "서울문화사",
    "서울미디어코믹스",
];

#[derive(Debug, Deserialize)]
struct AladinSearchResponse {
    #[serde(default, rename = "errorCode")]
    error_code: Option<i64>,
    #[serde(default, rename = "errorMessage")]
    error_message: Option<String>,
    #[serde(default)]
    item: AladinItems,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AladinItems {
    None,
    One(AladinItem),
    Many(Vec<AladinItem>),
}

impl Default for AladinItems {
    fn default() -> Self {
        AladinItems::None
    }
}

impl AladinItems {
    fn into_vec(self) -> Vec<AladinItem> {
        match self {
            AladinItems::None => vec![],
            AladinItems::One(item) => vec![item],
            AladinItems::Many(items) => items,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AladinItem {
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub pub_date: String,
    #[serde(default)]
    pub isbn13: String,
    pub item_id: i64,
    #[serde(default)]
    pub cover: String,
    #[serde(default)]
    pub category_id: Option<i64>,
    #[serde(default)]
    pub category_name: Option<String>,
    #[serde(default)]
    pub series_info: Option<AladinSeriesInfo>,
    #[serde(default)]
    pub sub_info: Option<AladinSubInfo>,
}

impl AladinItem {
    fn is_light_novel(&self) -> bool {
        self.category_name
            .as_deref()
            .map(|name| name.contains("라이트 노벨") || name.contains("라이트노벨"))
            .unwrap_or(false)
    }

    /// 종이책 LN이거나, 허용 LN 브랜드 전자책/도서인지.
    fn is_catalog_candidate(&self) -> bool {
        if self.is_light_novel() || self.category_id.is_none() {
            return true;
        }
        let cat = self.category_name.as_deref().unwrap_or("");
        // 만화/코믹 원작화는 제외 (요청 범위)
        if cat.contains("만화") || cat.contains("코믹") {
            return false;
        }
        if LN_IMPRINTS.iter().any(|imprint| self.publisher.contains(imprint)) {
            return true;
        }
        // 전자책 쪽 “라이트노벨/장르소설” 트리
        cat.contains("라이트") || cat.contains("장르소설")
    }

    fn series_id(&self) -> Option<i64> {
        self.series_info
            .as_ref()
            .or_else(|| {
                self.sub_info
                    .as_ref()
                    .and_then(|s| s.series_info.as_ref())
            })
            .map(|s| s.series_id)
            .filter(|&id| id > 0)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AladinSubInfo {
    #[serde(default)]
    pub series_info: Option<AladinSeriesInfo>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AladinSeriesInfo {
    pub series_id: i64,
    pub series_name: String,
}

#[derive(Debug, Clone)]
pub struct GroupedSeries {
    pub aladin_series_id: String,
    pub title: String,
    pub author: Option<String>,
    pub publisher: Option<String>,
    pub cover_url: Option<String>,
    pub items: Vec<AladinItem>,
}

async fn parse_aladin_response(
    response: reqwest::Response,
) -> AppResult<AladinSearchResponse> {
    let parsed = response.error_for_status()?.json::<AladinSearchResponse>().await?;
    if let Some(code) = parsed.error_code {
        let message = parsed
            .error_message
            .clone()
            .unwrap_or_else(|| format!("aladin error code {code}"));
        return Err(AppError::Internal(format!("Aladin API: {message}")));
    }
    Ok(parsed)
}

pub async fn search_items(state: &AppState, query: &str) -> AppResult<Vec<AladinItem>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(vec![]);
    }

    let mut all_items = Vec::new();

    // 1) 국내 종이책 · 라이트노벨 카테고리
    all_items.extend(
        search_items_raw(state, query, "Book", Some(LIGHT_NOVEL_CATEGORY_ID), "Title").await?,
    );

    // 2) 전자책 (카테고리 미지정 → 후보 필터로 LN 계열만 남김)
    all_items.extend(search_items_raw(state, query, "eBook", None, "Title").await?);

    // 긴 제목·쉼표 제목은 Title 검색이 약해서 Keyword + 축약 쿼리 보강
    for (q, qtype) in expanded_search_queries(query) {
        if q == query && qtype == "Title" {
            continue;
        }
        all_items.extend(search_items_raw(state, &q, "Book", Some(LIGHT_NOVEL_CATEGORY_ID), qtype).await?);
        all_items.extend(search_items_raw(state, &q, "eBook", None, qtype).await?);
    }

    let mut seen = std::collections::HashSet::new();
    all_items.retain(|item| seen.insert(item.item_id));

    Ok(all_items
        .into_iter()
        .filter(|item| item.is_catalog_candidate())
        .collect())
}

/// 알라딘 ItemSearch는 오래된 전자책을 누락하는 경우가 있다.
/// seriesInfo.seriesId가 있으면 시리즈 상품 페이지에서 ItemId를 모아 LookUp으로 보강한다.
async fn expand_aladin_series_siblings(
    state: &AppState,
    mut items: Vec<AladinItem>,
    only_sparse: bool,
) -> AppResult<Vec<AladinItem>> {
    let mut seen_items: HashSet<i64> = items.iter().map(|i| i.item_id).collect();
    let mut counts: HashMap<i64, usize> = HashMap::new();
    let mut series_ids: Vec<i64> = Vec::new();

    for item in &items {
        if let Some(sid) = item.series_id() {
            let entry = counts.entry(sid).or_insert(0);
            *entry += 1;
            if *entry == 1 {
                series_ids.push(sid);
            }
        }
    }

    // 검색 결과에 seriesInfo가 비어 있으면 대표 권을 LookUp해 seriesId를 채운다.
    if series_ids.is_empty() {
        let seed_ids: Vec<i64> = items
            .iter()
            .filter(|i| !rules_of(state).is_bundle_or_set(&i.title))
            .map(|i| i.item_id)
            .take(3)
            .collect();
        for item_id in seed_ids {
            if let Ok(full) = lookup_item(state, &item_id.to_string()).await {
                if let Some(sid) = full.series_id() {
                    if let Some(slot) = items.iter_mut().find(|i| i.item_id == item_id) {
                        slot.series_info = full.series_info.clone();
                    }
                    series_ids.push(sid);
                    counts.insert(sid, 1);
                    break;
                }
            }
        }
    }

    for sid in series_ids {
        if only_sparse && counts.get(&sid).copied().unwrap_or(0) >= 4 {
            continue;
        }
        let Ok(page_ids) = scrape_series_page_item_ids(&state.http, sid).await else {
            continue;
        };
        for id in page_ids {
            if !seen_items.insert(id) {
                continue;
            }
            let Ok(item) = lookup_item(state, &id.to_string()).await else {
                continue;
            };
            if !item.is_catalog_candidate() {
                continue;
            }
            if rules_of(state).is_bundle_or_set(&item.title) {
                continue;
            }
            // 페이지에 섞인 다른 상품 제외
            if item.series_id() != Some(sid) {
                continue;
            }
            items.push(item);
        }
    }

    Ok(items)
}

async fn scrape_series_page_item_ids(
    http: &reqwest::Client,
    series_id: i64,
) -> AppResult<Vec<i64>> {
    let url = format!("https://www.aladin.co.kr/shop/common/wseriesitem.aspx?SRID={series_id}");
    let text = http
        .get(&url)
        .header(
            reqwest::header::USER_AGENT,
            "linvlib/0.1 (+https://github.com/linvlib)",
        )
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    Ok(parse_series_page_item_ids(&text))
}

fn parse_series_page_item_ids(html: &str) -> Vec<i64> {
    let re = Regex::new(r"(?i)wproduct\.aspx\?ItemId=(\d+)").expect("series page item regex");
    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    for caps in re.captures_iter(html) {
        if let Ok(id) = caps[1].parse::<i64>() {
            if seen.insert(id) {
                ids.push(id);
            }
        }
    }
    ids
}

async fn search_items_raw(
    state: &AppState,
    query: &str,
    search_target: &str,
    category_id: Option<&str>,
    query_type: &str,
) -> AppResult<Vec<AladinItem>> {
    let mut all_items = Vec::new();
    // Aladin OpenAPI: `start` is 1-based PAGE index (not item offset).
    // MaxResults ≤ 50; total results capped at 200 → at most 4 pages of 50.
    const PAGE_SIZE: i64 = 50;
    const MAX_PAGES: i64 = 4; // 200 / 50
    let mut page = 1i64;
    let category = category_id.unwrap_or("0");

    loop {
        crate::services::quota::consume_one(state).await?;
        let response = state
            .http
            .get(ALADIN_SEARCH_URL)
            .query(&[
                ("ttbkey", state.config.aladin_ttb_key.as_str()),
                ("Query", query),
                ("QueryType", query_type),
                ("MaxResults", &PAGE_SIZE.to_string()),
                ("start", &page.to_string()),
                ("SearchTarget", search_target),
                ("CategoryId", category),
                ("output", "js"),
                ("Version", "20131101"),
                ("OptResult", "seriesInfo"),
                ("Cover", "Big"),
            ])
            .send()
            .await?;

        let parsed = parse_aladin_response(response).await?;
        let batch = parsed.item.into_vec();
        if batch.is_empty() {
            break;
        }

        let batch_len = batch.len() as i64;
        all_items.extend(batch);
        // 보강 Keyword도 페이지를 넘겨 긴 시리즈를 놓치지 않는다.
        if batch_len < PAGE_SIZE || page >= MAX_PAGES {
            break;
        }
        page += 1;
    }

    Ok(all_items)
}

/// Title 외에 쓸 보조 검색어·쿼리 타입.
fn expanded_search_queries(query: &str) -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    let chars = query.chars().count();

    if chars >= 24 || query.contains(',') {
        out.push((query.to_string(), "Keyword"));
    }

    // 긴 제목은 Title 정확도가 떨어져 앞부분 키워드로도 보강
    if chars >= 24 {
        let short: String = query
            .split_whitespace()
            .take(4)
            .collect::<Vec<_>>()
            .join(" ");
        if short.chars().count() >= 8 && short != query {
            out.push((short.clone(), "Title"));
            out.push((short, "Keyword"));
        }
    }

    if let Some((head, _)) = query.split_once(',') {
        let head = head.trim();
        if head.chars().count() >= 10 && head != query {
            out.push((head.to_string(), "Title"));
            out.push((head.to_string(), "Keyword"));
        }
    }

    out
}

pub async fn lookup_item(state: &AppState, item_id: &str) -> AppResult<AladinItem> {
    crate::services::quota::consume_one(state).await?;
    let response = state
        .http
        .get(ALADIN_LOOKUP_URL)
        .query(&[
            ("ttbkey", state.config.aladin_ttb_key.as_str()),
            ("ItemId", item_id),
            ("itemIdType", "ItemId"),
            ("output", "js"),
            ("Version", "20131101"),
            ("OptResult", "seriesInfo"),
            ("Cover", "Big"),
        ])
        .send()
        .await?;

    let parsed = parse_aladin_response(response).await?;
    parsed
        .item
        .into_vec()
        .into_iter()
        .next()
        .ok_or_else(|| AppError::NotFound(format!("aladin item {item_id} not found")))
}

/// 라벨/권수 접미를 제거한 시리즈 제목으로 묶는다.
/// 알라딘 seriesInfo는 종종 "S노블레스" 같은 레이블이라 신뢰하지 않는다.
pub fn group_by_series(items: Vec<AladinItem>) -> Vec<GroupedSeries> {
    group_by_series_with(items, default_rules())
}

pub fn group_by_series_with(items: Vec<AladinItem>, title_rules: &TitleRules) -> Vec<GroupedSeries> {
    let mut map: HashMap<String, GroupedSeries> = HashMap::new();

    for item in items
        .into_iter()
        .filter(|item| !title_rules.is_bundle_or_set(&item.title))
    {
        let title = title_rules.normalize_series_title(&item.title);
        if title.is_empty() {
            continue;
        }
        let series_id = format!("title:{title}");

        map.entry(series_id.clone())
            .and_modify(|group| {
                if !group.items.iter().any(|existing| existing.item_id == item.item_id) {
                    group.items.push(item.clone());
                }
            })
            .or_insert_with(|| GroupedSeries {
                aladin_series_id: series_id,
                title,
                author: non_empty(&item.author),
                publisher: non_empty(&item.publisher),
                cover_url: non_empty(&item.cover),
                items: vec![item],
            });
    }

    let mut groups: Vec<_> = map.into_values().collect();
    for group in &mut groups {
        group.items = dedupe_volume_items(std::mem::take(&mut group.items));
        group.items.sort_by(|a, b| {
            parse_pub_date(&a.pub_date)
                .cmp(&parse_pub_date(&b.pub_date))
                .then_with(|| a.item_id.cmp(&b.item_id))
        });
        if let Some(latest) = group.items.last() {
            group.cover_url = non_empty(&latest.cover);
        }
    }
    groups.sort_by(|a, b| a.title.cmp(&b.title));
    groups
}

pub async fn import_search(
    state: &AppState,
    query: &str,
) -> AppResult<Vec<ImportSearchResult>> {
    let query = query.trim();
    let mut items = search_items(state, query).await?;

    // 표기 차이·정규화 보조 검색 (규칙 파일 기반)
    for alt in rules_of(state).alternate_search_queries(query, None) {
        if let Ok(extra) = search_items(state, &alt).await {
            items.extend(extra);
        }
    }
    let mut seen = std::collections::HashSet::new();
    items.retain(|item| seen.insert(item.item_id));

    // 검색에 안 잡히는 시리즈 권을 시리즈 페이지로 보강
    items = expand_aladin_series_siblings(state, items, true).await?;

    let groups = group_by_series_with(items, rules_of(state));
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
        });
    }

    Ok(results)
}

pub async fn import_series(
    state: &AppState,
    aladin_series_id: Option<String>,
    seed_item_id: Option<String>,
    title_hint: Option<String>,
) -> AppResult<ImportResponse> {
    let series_key = match (aladin_series_id, seed_item_id) {
        (Some(id), _) => id,
        (None, Some(seed)) => format!("item:{seed}"),
        (None, None) => {
            return Err(AppError::BadRequest(
                "aladin_series_id or seed_item_id is required".into(),
            ));
        }
    };

    let search_query =
        resolve_search_query(state, &series_key, title_hint.as_deref()).await?;
    let mut items = search_items(state, &search_query).await?;

    let title_rules = rules_of(state);

    // 검색어가 너무 길거나 특수(Art Works 등)하면 짧은 보조 쿼리도 시도
    for alt in title_rules.alternate_search_queries(&search_query, title_hint.as_deref()) {
        if let Ok(extra) = search_items(state, &alt).await {
            items.extend(extra);
        }
    }

    // 본편 검색만으로는 외전/특별편이 빠질 수 있어 작품·전역 아크 보조 검색을 합친다
    for arc_query in title_rules.import_arc_search_queries(&search_query) {
        if let Ok(extra) = search_items(state, &arc_query).await {
            items.extend(extra);
        }
    }
    let raw_hint = title_hint.as_deref().unwrap_or("").trim();
    if !raw_hint.is_empty()
        && title_rules.normalize_series_title(raw_hint) != search_query
        && title_rules.extract_series_arc(raw_hint) > 0
    {
        if let Ok(extra) = search_items(state, raw_hint).await {
            items.extend(extra);
        }
    }

    // item_id 기준 중복 제거
    let mut seen = std::collections::HashSet::new();
    items.retain(|item| seen.insert(item.item_id));

    // ItemSearch가 일부 권만 줄 때(특히 전자책) 시리즈 페이지로 형제 권을 채운다.
    items = expand_aladin_series_siblings(state, items, false).await?;

    let groups = group_by_series_with(items.clone(), title_rules);
    let mut group = select_group(&groups, &series_key, &search_query)
        .ok_or_else(|| AppError::NotFound("series not found in aladin".into()))?
        .clone();

    // 합본/다른 에디션의 더 이른 출간일을 본권에 반영 (알라딘 재등록 날짜 오인 보정)
    let date_hints = collect_earliest_volume_dates(&items);
    apply_earliest_dates(&mut group.items, &date_hints);

    let mut dates: Vec<_> = group
        .items
        .iter()
        .filter_map(|i| parse_pub_date(&i.pub_date))
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

    // 요청한 시리즈(갱신)가 있으면 그 행에 붙인다. 정규화로 키가 바뀌어도 새 행을 만들지 않는다.
    let series = if let Some(existing) =
        repositories::find_series_by_aladin_id(&state.pool, &series_key).await?
    {
        repositories::canonicalize_series_identity(
            &state.pool,
            existing.id,
            &group.aladin_series_id,
            &group.title,
        )
        .await?
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
            group.cover_url.as_deref(),
        )
        .await?
    };

    let existing_count = repositories::count_volumes(&state.pool, series.id).await?;

    if existing_count == 0 {
        // 첫 등록: 제목 권수(가능하면) 또는 출간일 순으로 1..n
        let numbered_items = assign_volume_numbers(&group.items);
        for (item, volume_number) in &numbered_items {
            repositories::upsert_volume(
                &state.pool,
                Uuid::new_v4(),
                series.id,
                *volume_number,
                &clean_volume_title(&item.title),
                non_empty(&item.cover).as_deref(),
                parse_pub_date(&item.pub_date),
                &item.item_id.to_string(),
                if item.isbn13.is_empty() {
                    None
                } else {
                    Some(item.isbn13.as_str())
                },
            )
            .await?;
        }
    } else {
        // 갱신: 기존 item_id는 메타만 갱신. 같은 권수 키의 다른 에디션(전자책 등)은 추가하지 않는다.
        let existing_vols = repositories::list_volumes(&state.pool, series.id, "asc").await?;
        let mut key_to_id: HashMap<(u8, u8, i64, u8, u8, u8), Uuid> = HashMap::new();
        for vol in &existing_vols {
            if let Some(key) = volume_identity_key(&vol.title) {
                key_to_id.entry(key).or_insert(vol.id);
            }
        }

        let mut next = repositories::max_volume_number(&state.pool, series.id).await? + 1;
        let mut fresh = Vec::new();
        for item in &group.items {
            let item_id = item.item_id.to_string();
            let exists = repositories::find_volume_by_aladin_item_id(&state.pool, &item_id)
                .await?
                .is_some();
            if exists {
                repositories::upsert_volume(
                    &state.pool,
                    Uuid::new_v4(),
                    series.id,
                    0,
                    &clean_volume_title(&item.title),
                    non_empty(&item.cover).as_deref(),
                    parse_pub_date(&item.pub_date),
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

            let cleaned = clean_volume_title(&item.title);
            if let Some(key) = volume_identity_key(&cleaned) {
                if let Some(&existing_id) = key_to_id.get(&key) {
                    repositories::refresh_volume_metadata(
                        &state.pool,
                        existing_id,
                        non_empty(&item.cover).as_deref(),
                        parse_pub_date(&item.pub_date),
                        if item.isbn13.is_empty() {
                            None
                        } else {
                            Some(item.isbn13.as_str())
                        },
                    )
                    .await?;
                    continue;
                }
            }

            fresh.push(item.clone());
        }
        fresh.sort_by(|a, b| {
            parse_pub_date(&a.pub_date)
                .cmp(&parse_pub_date(&b.pub_date))
                .then_with(|| a.item_id.cmp(&b.item_id))
        });
        for item in fresh {
            let cleaned = clean_volume_title(&item.title);
            let inserted = repositories::upsert_volume(
                &state.pool,
                Uuid::new_v4(),
                series.id,
                next,
                &cleaned,
                non_empty(&item.cover).as_deref(),
                parse_pub_date(&item.pub_date),
                &item.item_id.to_string(),
                if item.isbn13.is_empty() {
                    None
                } else {
                    Some(item.isbn13.as_str())
                },
            )
            .await?;
            if let Some(key) = volume_identity_key(&cleaned) {
                key_to_id.entry(key).or_insert(inserted.id);
            }
            next += 1;
        }
    }

    // 과거에 생긴 동일 권수 중복(종이/전자 등)을 정리한 뒤 권번호 재정렬
    collapse_duplicate_volumes_by_identity(state, series.id).await?;
    reconcile_volume_numbers_from_titles(state, series.id).await?;

    repositories::refresh_series_publish_dates(&state.pool, series.id).await?;
    repositories::touch_series_refreshed(&state.pool, series.id).await?;

    let volume_count = repositories::count_volumes(&state.pool, series.id).await?;

    Ok(ImportResponse {
        series_id: series.id,
        title: series.title,
        volume_count,
    })
}

/// Re-import Aladin series oldest-first until soft daily quota is exhausted.
pub async fn refresh_all_aladin(
    state: &AppState,
) -> AppResult<crate::models::BulkRefreshResponse> {
    use crate::models::{BulkRefreshItem, BulkRefreshResponse};
    use std::time::Duration;
    use tokio::time::sleep;

    let series_list = repositories::list_aladin_series_for_refresh(&state.pool).await?;
    let total = series_list.len() as i64;
    let mut refreshed = 0i64;
    let mut failed = 0i64;
    let mut skipped = 0i64;
    let mut items = Vec::with_capacity(series_list.len());
    let mut stopped_for_quota = false;

    for (idx, (series_id, title, aladin_series_id)) in series_list.into_iter().enumerate() {
        if crate::services::quota::remaining_soft_quota(state).await? == 0 {
            stopped_for_quota = true;
            skipped = total - (refreshed + failed);
            tracing::info!(
                refreshed,
                failed,
                skipped,
                "stopping bulk refresh: daily soft quota reached"
            );
            break;
        }
        if idx > 0 {
            sleep(Duration::from_millis(250)).await;
        }
        match import_series(
            state,
            Some(aladin_series_id),
            None,
            Some(title.clone()),
        )
        .await
        {
            Ok(res) => {
                refreshed += 1;
                items.push(BulkRefreshItem {
                    series_id: res.series_id,
                    title: res.title,
                    ok: true,
                    volume_count: Some(res.volume_count),
                    error: None,
                });
            }
            Err(AppError::QuotaExceeded(msg)) => {
                failed += 1;
                stopped_for_quota = true;
                items.push(BulkRefreshItem {
                    series_id,
                    title,
                    ok: false,
                    volume_count: None,
                    error: Some(msg),
                });
                skipped = total - (refreshed + failed);
                break;
            }
            Err(err) => {
                failed += 1;
                tracing::warn!(%series_id, %title, error = %err, "bulk refresh failed");
                items.push(BulkRefreshItem {
                    series_id,
                    title,
                    ok: false,
                    volume_count: None,
                    error: Some(err.to_string()),
                });
            }
        }
    }

    let note = if stopped_for_quota {
        format!("quota pause: refreshed={refreshed} failed={failed} deferred={skipped}")
    } else {
        format!("complete: refreshed={refreshed} failed={failed}")
    };
    let _ = repositories::set_app_meta(&state.pool, "last_refresh_note", &note).await;
    let _ = repositories::set_app_meta(
        &state.pool,
        "last_refresh_at",
        &Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
    )
    .await;

    Ok(BulkRefreshResponse {
        total,
        refreshed,
        failed,
        items,
    })
}

async fn resolve_search_query(
    state: &AppState,
    series_key: &str,
    title_hint: Option<&str>,
) -> AppResult<String> {
    if let Some(existing) =
        repositories::find_series_by_aladin_id(&state.pool, series_key).await?
    {
        // 과거에 "… 09 (상)"처럼 권 접미가 붙은 제목으로 저장된 경우에도
        // 본편 제목으로 검색해야 나머지 권이 잡힌다.
        return Ok(normalize_series_title(&existing.title));
    }

    if let Some(hint) = title_hint.map(str::trim).filter(|s| !s.is_empty()) {
        return Ok(normalize_series_title(hint));
    }

    if let Some(item_id) = series_key.strip_prefix("item:") {
        let item = lookup_item(state, item_id).await?;
        return Ok(normalize_series_title(&item.title));
    }

    if let Some(title) = series_key.strip_prefix("title:") {
        return Ok(normalize_series_title(title));
    }

    Err(AppError::BadRequest(
        "title is required to import this series".into(),
    ))
}

fn select_group<'a>(
    groups: &'a [GroupedSeries],
    series_key: &str,
    search_query: &str,
) -> Option<&'a GroupedSeries> {
    let normalized_key = if let Some(title) = series_key.strip_prefix("title:") {
        format!("title:{}", normalize_series_title(title))
    } else {
        series_key.to_string()
    };
    let query_key = format!("title:{}", normalize_series_title(search_query));
    let query_norm = normalize_series_title(search_query);

    if let Some(group) = groups
        .iter()
        .find(|g| g.aladin_series_id == series_key || g.aladin_series_id == normalized_key)
    {
        return Some(group);
    }

    if let Some(item_id) = series_key.strip_prefix("item:") {
        if let Ok(id) = item_id.parse::<i64>() {
            if let Some(group) = groups.iter().find(|g| g.items.iter().any(|i| i.item_id == id)) {
                return Some(group);
            }
        }
    }

    if let Some(group) = groups.iter().find(|g| g.aladin_series_id == query_key) {
        return Some(group);
    }

    if let Some(group) = groups
        .iter()
        .find(|g| normalize_series_title(&g.title) == query_norm)
    {
        return Some(group);
    }

    groups.iter().find(|g| {
        let gt = normalize_series_title(&g.title);
        (!query_norm.is_empty() && (gt.contains(&query_norm) || query_norm.contains(&gt)))
            || (series_key
                .strip_prefix("title:")
                .map(normalize_series_title)
                .is_some_and(|k| !k.is_empty() && (gt.contains(&k) || k.contains(&gt))))
    })
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn is_bundle_or_set(title: &str) -> bool {
    rules().is_bundle_or_set(title)
}

/// 한정판/초회판/특별판 등은 본편 권과 중복이므로 가져오지 않는다.
/// (본편이 없고 특별판만 있으면 dedupe에서 남긴다)
fn is_limited_edition(title: &str) -> bool {
    rules().is_limited_edition(title)
}

/// 같은 권번호·같은 학년/부·같은 상중하·같은 아크의 다른 에디션이 있으면 대표 한 권만 남긴다.
fn dedupe_volume_items(items: Vec<AladinItem>) -> Vec<AladinItem> {
    // (school_year, story_part, major, minor, sangha, sss_arc)
    let mut by_key: HashMap<(u8, u8, i64, u8, u8, u8), Vec<AladinItem>> = HashMap::new();
    let mut unnumbered = Vec::new();

    for item in items {
        match extract_volume_parts(&item.title) {
            Some((major, minor)) => {
                let school_year = extract_school_year(&item.title);
                let story_part = extract_story_part(&item.title);
                let part = extract_volume_part(&item.title);
                let arc = extract_series_arc(&item.title);
                by_key
                    .entry((school_year, story_part, major, minor, part, arc))
                    .or_default()
                    .push(item);
            }
            None => {
                // 권수 없는 박스/달력 한정 구성은 제외. 번호 있는 초회판만 본편 없을 때 살린다.
                if !is_limited_edition(&item.title) {
                    unnumbered.push(item);
                }
            }
        }
    }

    let mut result: Vec<AladinItem> = by_key
        .into_values()
        .map(|mut editions| {
            editions.sort_by(|a, b| {
                edition_preference(b)
                    .cmp(&edition_preference(a))
                    // 같은 점수면 더 이른 출간일(원판) 우선
                    .then_with(|| {
                        parse_pub_date(&a.pub_date).cmp(&parse_pub_date(&b.pub_date))
                    })
                    .then_with(|| b.title.chars().count().cmp(&a.title.chars().count()))
            });
            // 에디션 중 가장 이른 출간일을 대표 권에 반영
            let earliest = editions
                .iter()
                .filter_map(|e| parse_pub_date(&e.pub_date))
                .min();
            let mut chosen = editions.remove(0);
            if let Some(date) = earliest {
                let date_str = date.format("%Y-%m-%d").to_string();
                if parse_pub_date(&chosen.pub_date).is_none_or(|d| date < d) {
                    chosen.pub_date = date_str;
                }
            }
            chosen
        })
        .collect();
    result.extend(dedupe_unnumbered_editions(unnumbered));
    result
}

/// 권수 없는 항목(청춘 돼지 등)도 특별판/본편을 하나로 합친다.
fn dedupe_unnumbered_editions(items: Vec<AladinItem>) -> Vec<AladinItem> {
    let mut by_key: HashMap<String, Vec<AladinItem>> = HashMap::new();
    for item in items {
        let key = clean_volume_title(&item.title).to_lowercase();
        by_key.entry(key).or_default().push(item);
    }
    by_key
        .into_values()
        .map(|mut editions| {
            editions.sort_by(|a, b| {
                edition_preference(b)
                    .cmp(&edition_preference(a))
                    .then_with(|| parse_pub_date(&a.pub_date).cmp(&parse_pub_date(&b.pub_date)))
            });
            editions.remove(0)
        })
        .collect()
}

fn edition_preference(item: &AladinItem) -> i32 {
    rules().edition_preference(&item.title)
}

/// DB용 권번호.
/// - SSS/Ex 등 아크가 섞이면 출간일 순(아크가 본편 사이에 끼도록).
/// - 그 외에는 제목에서 뽑은 권수를 우선(개정판 출간일로 순서가 꼬이는 경우 방지).
fn assign_volume_numbers(items: &[AladinItem]) -> Vec<(AladinItem, i64)> {
    if items.iter().any(|i| extract_series_arc(&i.title) > 0) {
        return assign_volume_numbers_by_release(items);
    }
    if let Some(by_title) = try_assign_volume_numbers_by_title(items) {
        return by_title;
    }
    assign_volume_numbers_by_release(items)
}

fn try_assign_volume_numbers_by_title(items: &[AladinItem]) -> Option<Vec<(AladinItem, i64)>> {
    let mut keyed = Vec::with_capacity(items.len());
    let mut seen = HashSet::new();
    for item in items {
        let (major, minor) = extract_volume_parts(&item.title)?;
        let key = (
            extract_school_year(&item.title),
            extract_story_part(&item.title),
            major,
            minor,
            extract_volume_part(&item.title),
        );
        if !seen.insert(key) {
            return None;
        }
        keyed.push((item.clone(), key));
    }
    keyed.sort_by(|a, b| {
        a.1.cmp(&b.1)
            .then_with(|| parse_pub_date(&a.0.pub_date).cmp(&parse_pub_date(&b.0.pub_date)))
            .then_with(|| a.0.item_id.cmp(&b.0.item_id))
    });
    Some(
        keyed
            .into_iter()
            .enumerate()
            .map(|(idx, (item, _))| (item, (idx + 1) as i64))
            .collect(),
    )
}

/// DB용 권번호: 출간일 오름차순으로 1..n
fn assign_volume_numbers_by_release(items: &[AladinItem]) -> Vec<(AladinItem, i64)> {
    let mut sorted = items.to_vec();
    sorted.sort_by(|a, b| {
        parse_pub_date(&a.pub_date)
            .cmp(&parse_pub_date(&b.pub_date))
            .then_with(|| a.item_id.cmp(&b.item_id))
    });
    sorted
        .into_iter()
        .enumerate()
        .map(|(idx, item)| (item, (idx + 1) as i64))
        .collect()
}

fn volume_identity_key(title: &str) -> Option<(u8, u8, i64, u8, u8, u8)> {
    let (major, minor) = extract_volume_parts(title)?;
    Some((
        extract_school_year(title),
        extract_story_part(title),
        major,
        minor,
        extract_volume_part(title),
        extract_series_arc(title),
    ))
}

/// 같은 시리즈 안에서 동일 권수 키로 쌓인 중복 행을 하나로 합친다.
async fn collapse_duplicate_volumes_by_identity(
    state: &AppState,
    series_id: Uuid,
) -> AppResult<()> {
    let volumes = repositories::list_volumes(&state.pool, series_id, "asc").await?;
    let mut by_key: HashMap<(u8, u8, i64, u8, u8, u8), Vec<_>> = HashMap::new();
    for vol in volumes {
        let Some(key) = volume_identity_key(&vol.title) else {
            continue;
        };
        by_key.entry(key).or_default().push(vol);
    }

    for mut group in by_key.into_values() {
        if group.len() < 2 {
            continue;
        }
        group.sort_by(|a, b| {
            edition_preference_title(&b.title)
                .cmp(&edition_preference_title(&a.title))
                .then_with(|| a.published_at.cmp(&b.published_at))
                .then_with(|| a.volume_number.cmp(&b.volume_number))
                .then_with(|| a.id.cmp(&b.id))
        });
        let keep = group.remove(0);
        for drop in group {
            tracing::info!(
                %series_id,
                keep = %keep.id,
                drop = %drop.id,
                title = %keep.title,
                "collapsing duplicate volume"
            );
            repositories::merge_and_delete_volume(&state.pool, series_id, keep.id, drop.id)
                .await?;
        }
    }
    Ok(())
}

fn edition_preference_title(title: &str) -> i32 {
    rules().edition_preference(title)
}

/// 이미 저장된 권을 제목 권수 기준으로 재정렬한다.
/// SSS/Ex 아크가 있거나 권수 추출이 안 되면 손대지 않는다.
async fn reconcile_volume_numbers_from_titles(
    state: &AppState,
    series_id: Uuid,
) -> AppResult<()> {
    let volumes = repositories::list_volumes(&state.pool, series_id, "asc").await?;
    if volumes.len() < 2 {
        return Ok(());
    }
    if volumes
        .iter()
        .any(|v| extract_series_arc(&v.title) > 0)
    {
        return Ok(());
    }

    let mut keyed = Vec::with_capacity(volumes.len());
    let mut seen = HashSet::new();
    for vol in &volumes {
        let Some((major, minor)) = extract_volume_parts(&vol.title) else {
            return Ok(());
        };
        let key = (
            extract_school_year(&vol.title),
            extract_story_part(&vol.title),
            major,
            minor,
            extract_volume_part(&vol.title),
        );
        if !seen.insert(key) {
            return Ok(());
        }
        keyed.push((vol.id, key, vol.volume_number));
    }

    keyed.sort_by(|a, b| a.1.cmp(&b.1));
    let already_ok = keyed
        .iter()
        .enumerate()
        .all(|(idx, (_, _, num))| *num == (idx + 1) as i64);
    if already_ok {
        return Ok(());
    }

    let ids: Vec<Uuid> = keyed.into_iter().map(|(id, _, _)| id).collect();
    repositories::renumber_volumes_in_order(&state.pool, series_id, &ids).await
}

/// DB/화면용 권 제목: 출판사 레이블·완결 표기만 제거
pub fn clean_volume_title(title: &str) -> String {
    rules().clean_volume_title(title)
}

/// 시리즈 제목 정규화 (.5권·학년 편·제N부·Art Works·아크·프랜차이즈 별칭 포함)
pub fn normalize_series_title(title: &str) -> String {
    rules().normalize_series_title(title)
}

/// (major, minor_tenths) — 7 → (7,0), 7.5 → (7,5)
fn extract_volume_parts(title: &str) -> Option<(i64, u8)> {
    rules().extract_volume_parts(title)
}

/// 1=1학년(표기 없음 포함), 2=2학년, 3=3학년 …
fn extract_school_year(title: &str) -> u8 {
    rules().extract_school_year(title)
}

/// 제N부 → N, 없으면 0
fn extract_story_part(title: &str) -> u8 {
    rules().extract_story_part(title)
}

/// 0=본편, 그 외는 title_rules.toml [[arcs]].code
fn extract_series_arc(title: &str) -> u8 {
    rules().extract_series_arc(title)
}

fn extract_volume_part(title: &str) -> u8 {
    rules().extract_volume_part(title)
}

pub fn format_volume_label_from_title(
    title: &str,
    fallback_number: i64,
    arc_volume_count: usize,
) -> String {
    rules().format_volume_label_from_title(title, fallback_number, arc_volume_count)
}

fn parse_pub_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

/// 본권·합본·다른 에디션을 모두 보고 (학년,권수…)별 가장 이른 출간일을 모은다.
fn collect_earliest_volume_dates(
    items: &[AladinItem],
) -> HashMap<(u8, u8, i64, u8, u8, u8), NaiveDate> {
    let mut map: HashMap<(u8, u8, i64, u8, u8, u8), NaiveDate> = HashMap::new();
    for item in items {
        let Some(date) = parse_pub_date(&item.pub_date) else {
            continue;
        };
        let Some(key) = volume_date_key(&item.title) else {
            continue;
        };
        map.entry(key)
            .and_modify(|existing| {
                if date < *existing {
                    *existing = date;
                }
            })
            .or_insert(date);
    }
    map
}

fn volume_date_key(title: &str) -> Option<(u8, u8, i64, u8, u8, u8)> {
    let (major, minor) = extract_volume_parts_for_date_hint(title)?;
    Some((
        extract_school_year(title),
        extract_story_part(title),
        major,
        minor,
        extract_volume_part(title),
        extract_series_arc(title),
    ))
}

fn extract_volume_parts_for_date_hint(title: &str) -> Option<(i64, u8)> {
    if rules().is_art_works_title(title) {
        return None;
    }
    if is_bundle_or_set(title) {
        return extract_bundle_primary_volume(title);
    }
    extract_volume_parts(title)
}

/// `무직전생 26 + 스페셜북 합본` → 26권 출간일 힌트
fn extract_bundle_primary_volume(title: &str) -> Option<(i64, u8)> {
    let stripped = Regex::new(r"\s*\d+학년\s*편\s*")
        .unwrap()
        .replace_all(title, " ")
        .to_string();
    let re = Regex::new(r"(?i)(?:^|[^\d.])(\d+)(?:\.(\d+))?\s*(?:권)?\s*\+").unwrap();
    let caps = re.captures(&stripped)?;
    let major: i64 = caps.get(1)?.as_str().parse().ok()?;
    let minor: u8 = caps
        .get(2)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0);
    Some((major, minor))
}

fn apply_earliest_dates(
    items: &mut [AladinItem],
    hints: &HashMap<(u8, u8, i64, u8, u8, u8), NaiveDate>,
) {
    for item in items.iter_mut() {
        let Some(key) = volume_date_key(&item.title) else {
            continue;
        };
        let Some(hint) = hints.get(&key).copied() else {
            continue;
        };
        match parse_pub_date(&item.pub_date) {
            Some(current) if hint < current => {
                item.pub_date = hint.format("%Y-%m-%d").to_string();
            }
            None => {
                item.pub_date = hint.format("%Y-%m-%d").to_string();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str, id: i64) -> AladinItem {
        AladinItem {
            title: title.into(),
            author: "작가".into(),
            publisher: "출판".into(),
            pub_date: "2020-01-01".into(),
            isbn13: String::new(),
            item_id: id,
            cover: "a".into(),
            category_id: Some(50927),
            category_name: Some("국내도서>만화/라이트노벨>라이트 노벨>S 노벨".into()),
            series_info: None,
            sub_info: None,
        }
    }

    #[test]
    fn catalog_candidate_keeps_ln_ebook_imprint() {
        let mut ebook = item("어떤 라노베 1", 1);
        ebook.category_id = Some(12345);
        ebook.category_name = Some("전자책>소설".into());
        ebook.publisher = "시프트노벨".into();
        assert!(ebook.is_catalog_candidate());
    }

    #[test]
    fn catalog_candidate_rejects_comic_category() {
        let mut comic = item("눈을 떴더니 최강 무장 01", 2);
        comic.category_id = Some(999);
        comic.category_name = Some("전자책>만화".into());
        comic.publisher = "시프트코믹스".into();
        assert!(!comic.is_catalog_candidate());
    }

    #[test]
    fn expands_long_comma_titles_for_search() {
        let q = "눈을 떴더니 최강 무장과 우주선을 가지고 있어서, 집 한채를 목표로 용병으로 자유롭게 살고 싶다";
        let variants = expanded_search_queries(q);
        assert!(variants.iter().any(|(s, t)| s == q && *t == "Keyword"));
        assert!(variants
            .iter()
            .any(|(s, t)| s.contains("우주선") && !s.contains("집 한채") && *t == "Title"));
    }

    #[test]
    fn normalizes_label_suffix_titles() {
        assert_eq!(
            normalize_series_title("마녀와 용병 5 - S Novel+"),
            "마녀와 용병"
        );
        assert_eq!(
            normalize_series_title("마녀와 용병 1 - S노블레스"),
            "마녀와 용병"
        );
        assert_eq!(normalize_series_title("데스마치 3권"), "데스마치");
        assert_eq!(
            normalize_series_title("무직전생 스페셜북 - Premium Extreme Novel"),
            "무직전생 스페셜북"
        );
        assert_eq!(
            normalize_series_title(
                "무직전생 1 - 이세계에 갔으면 최선을 다한다, Premium Extreme Novel"
            ),
            "무직전생"
        );
    }

    #[test]
    fn extracts_volume_number() {
        assert_eq!(extract_volume_parts("데스마치 1권").map(|(n, _)| n), Some(1));
        assert_eq!(extract_volume_parts("제12권").map(|(n, _)| n), Some(12));
        assert_eq!(
            extract_volume_parts("마녀와 용병 5 - S Novel+").map(|(n, _)| n),
            Some(5)
        );
        assert_eq!(
            extract_volume_parts(
                "무직전생 1 - 이세계에 갔으면 최선을 다한다, Premium Extreme Novel"
            )
            .map(|(n, _)| n),
            Some(1)
        );
        assert_eq!(
            extract_volume_parts("무직전생 스페셜북 - Premium Extreme Novel"),
            None
        );
        assert_eq!(
            extract_volume_parts(
                "남녀비 1:5 세계에서도 평범하게 살 수 있을 줄 알았어? 1 - S Novel"
            )
            .map(|(n, _)| n),
            Some(1)
        );
        assert_eq!(
            extract_volume_parts(
                "남녀비 1:5 세계에서도 평범하게 살 수 있을 줄 알았어? 4 - S Novel"
            )
            .map(|(n, _)| n),
            Some(4)
        );
    }

    #[test]
    fn keeps_ratio_in_series_title() {
        assert_eq!(
            normalize_series_title(
                "남녀비 1:5 세계에서도 평범하게 살 수 있을 줄 알았어? 1 - S Novel"
            ),
            "남녀비 1:5 세계에서도 평범하게 살 수 있을 줄 알았어?"
        );
        assert_eq!(
            normalize_series_title(
                "남녀비 1:5 세계에서도 평범하게 살 수 있을 줄 알았어? 3 - S Novel"
            ),
            "남녀비 1:5 세계에서도 평범하게 살 수 있을 줄 알았어?"
        );
        let items = vec![
            item(
                "남녀비 1:5 세계에서도 평범하게 살 수 있을 줄 알았어? 1 - S Novel",
                1,
            ),
            item(
                "남녀비 1:5 세계에서도 평범하게 살 수 있을 줄 알았어? 2 - S Novel",
                2,
            ),
            item(
                "남녀비 1:5 세계에서도 평범하게 살 수 있을 줄 알았어? 3 - S Novel",
                3,
            ),
            item(
                "남녀비 1:5 세계에서도 평범하게 살 수 있을 줄 알았어? 4 - S Novel",
                4,
            ),
        ];
        let groups = group_by_series(items);
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].title,
            "남녀비 1:5 세계에서도 평범하게 살 수 있을 줄 알았어?"
        );
        assert_eq!(groups[0].items.len(), 4);
    }

    #[test]
    fn groups_volumes_with_label_suffix() {
        let items = vec![
            item("마녀와 용병 1 - S Novel+", 1),
            item("마녀와 용병 2 - S Novel+", 2),
            item("마녀와 용병 3 - S Novel+", 3),
        ];
        let groups = group_by_series(items);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "마녀와 용병");
        assert_eq!(groups[0].aladin_series_id, "title:마녀와 용병");
        assert_eq!(groups[0].items.len(), 3);
    }

    #[test]
    fn dedupes_same_volume_editions() {
        let items = vec![
            item("무직전생 1 - 어린 날, Premium Extreme Novel", 1),
            item(
                "무직전생 1 - 이세계에 갔으면 최선을 다한다, Premium Extreme Novel",
                2,
            ),
            item(
                "무직전생 2 - 이세계에 갔으면 최선을 다한다, Premium Extreme Novel",
                3,
            ),
        ];
        let groups = group_by_series(items);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].items.len(), 2);
        assert!(groups[0]
            .items
            .iter()
            .all(|i| !i.title.contains("어린 날")));
    }

    #[test]
    fn keeps_special_book_separate() {
        let items = vec![
            item(
                "무직전생 1 - 이세계에 갔으면 최선을 다한다, Premium Extreme Novel",
                1,
            ),
            item("무직전생 스페셜북 - Premium Extreme Novel", 2),
            item(
                "무직전생 26 + 무직전생 스페셜북 합본 세트 - 전2권 - Premium Extreme Novel",
                3,
            ),
        ];
        let groups = group_by_series(items);
        let titles: Vec<_> = groups.iter().map(|g| g.title.as_str()).collect();
        assert!(titles.contains(&"무직전생"));
        assert!(titles.contains(&"무직전생 스페셜북"));
        assert!(!titles.iter().any(|t| t.contains("합본")));
    }

    #[test]
    fn merges_chapter_subtitle_volumes() {
        assert_eq!(
            normalize_series_title("누가 용사를 죽였는가 - S Novel+"),
            "누가 용사를 죽였는가"
        );
        assert_eq!(
            normalize_series_title("누가 용사를 죽였는가 : 예언의 장 - S Novel+"),
            "누가 용사를 죽였는가"
        );
        assert_eq!(
            normalize_series_title("누가 용사를 죽였는가 : 용사의 장 - S Novel+"),
            "누가 용사를 죽였는가"
        );

        let items = vec![
            item("누가 용사를 죽였는가 - S Novel+", 1),
            item("누가 용사를 죽였는가 : 예언의 장 - S Novel+", 2),
            item(
                "누가 용사를 죽였는가 : 용사의 장 - S Novel+ / 초판 한정 책갈피, 양면 커버",
                3,
            ),
        ];
        let groups = group_by_series(items);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "누가 용사를 죽였는가");
        assert_eq!(groups[0].items.len(), 3);
        assert_eq!(
            format_volume_label_from_title("누가 용사를 죽였는가 : 예언의 장 - S Novel+", 2, 0),
            "예언의 장"
        );
        assert_eq!(
            format_volume_label_from_title("누가 용사를 죽였는가 : 용사의 장 - S Novel+", 3, 0),
            "용사의 장"
        );
    }

    #[test]
    fn merges_sajok_into_main_series() {
        assert_eq!(
            normalize_series_title("무직전생 1 - 사족 편, Premium Extreme Novel"),
            "무직전생"
        );
        assert_eq!(
            normalize_series_title("무직전생 3 - 사족 편, Premium Extreme Novel"),
            "무직전생"
        );
        assert_eq!(normalize_series_title("무직전생 사족편"), "무직전생");
        assert_eq!(normalize_series_title("무직전생 사족 편"), "무직전생");
        assert_eq!(
            normalize_series_title("무직전생 ~사족 편~ 1권"),
            "무직전생"
        );
        assert_eq!(
            clean_volume_title("무직전생 1 - 사족 편, Premium Extreme Novel"),
            "무직전생 1 - 사족편"
        );
        assert_eq!(
            format_volume_label_from_title("무직전생 1 - 사족 편, Premium Extreme Novel", 27, 3),
            "사족편 1권"
        );

        let items = vec![
            item(
                "무직전생 1 - 이세계에 갔으면 최선을 다한다, Premium Extreme Novel",
                1,
            ),
            item("무직전생 1 - 사족 편, Premium Extreme Novel", 2),
            item("무직전생 2 - 사족 편, Premium Extreme Novel", 3),
            item("무직전생 3 - 사족 편, Premium Extreme Novel", 4),
        ];
        let groups = group_by_series(items);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "무직전생");
        assert_eq!(groups[0].items.len(), 4);
    }

    fn item_with_date(title: &str, id: i64, pub_date: &str) -> AladinItem {
        let mut i = item(title, id);
        i.pub_date = pub_date.into();
        i
    }

    #[test]
    fn merges_sss_into_main_series() {
        let items = vec![
            item_with_date("패배 히로인이 너무 많아! 1", 1, "2023-07-01"),
            item_with_date("패배 히로인이 너무 많아! 2", 2, "2023-10-01"),
            item_with_date("패배 히로인이 너무 많아! SSS 1", 3, "2024-05-01"),
            item_with_date("패배 히로인이 너무 많아! SSS 2", 4, "2024-11-01"),
        ];
        let groups = group_by_series(items);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "패배 히로인이 너무 많아!");
        assert_eq!(groups[0].items.len(), 4);

        let nums = assign_volume_numbers_by_release(&groups[0].items);
        let encoded: Vec<i64> = nums.iter().map(|(_, n)| *n).collect();
        assert_eq!(encoded, vec![1, 2, 3, 4]);
        assert_eq!(
            format_volume_label_from_title("패배 히로인이 너무 많아! SSS 1", 3, 2),
            "SSS 1권"
        );
        assert_eq!(
            format_volume_label_from_title("패배 히로인이 너무 많아! SSS - Novel Engine", 9, 1),
            "SSS"
        );
        assert_eq!(
            normalize_series_title("패배 히로인이 너무 많아! SSS - Novel Engine"),
            "패배 히로인이 너무 많아!"
        );
        assert_eq!(
            clean_volume_title("패배 히로인이 너무 많아! 4 - Novel Engine"),
            "패배 히로인이 너무 많아! 4"
        );
    }

    #[test]
    fn orders_by_release_date_not_arc() {
        // SSS가 본편 사이에 출간된 경우 출시 순으로 끼어든다
        let items = vec![
            item_with_date("패배 히로인이 너무 많아! 1", 1, "2023-01-01"),
            item_with_date("패배 히로인이 너무 많아! SSS 1", 2, "2023-06-01"),
            item_with_date("패배 히로인이 너무 많아! 2", 3, "2023-12-01"),
        ];
        let nums = assign_volume_numbers_by_release(&items);
        let titles: Vec<_> = nums.iter().map(|(i, _)| i.title.as_str()).collect();
        assert_eq!(
            titles,
            vec![
                "패배 히로인이 너무 많아! 1",
                "패배 히로인이 너무 많아! SSS 1",
                "패배 히로인이 너무 많아! 2",
            ]
        );
        assert_eq!(
            nums.iter().map(|(_, n)| *n).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn keeps_sang_ha_volumes_separate() {
        let items = vec![
            item("변경의 팔라딘 1 - 부제", 1),
            item("변경의 팔라딘 2 - 부제", 2),
            item("변경의 팔라딘 3 - 상 - 철원의 궤적", 3),
            item("변경의 팔라딘 3 - 하 - 철원의 궤적", 4),
            item("변경의 팔라딘 4 - 부제", 5),
        ];
        let groups = group_by_series(items);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].items.len(), 5);

        assert_eq!(
            format_volume_label_from_title("변경의 팔라딘 3 - 상 - 철원의 궤적", 3, 0),
            "3상"
        );
        assert_eq!(
            format_volume_label_from_title("변경의 팔라딘 3 - 하 - 철원의 궤적", 4, 0),
            "3하"
        );
    }

    #[test]
    fn parses_series_page_item_ids() {
        let html = r#"
            <a href="/shop/wproduct.aspx?ItemId=322382456">01</a>
            <a href="https://www.aladin.co.kr/shop/wproduct.aspx?ItemId=383428338&amp;start=p1">08</a>
            <a href="/shop/wproduct.aspx?ItemId=322382456">dup</a>
            <a href="/shop/wbasket.aspx?ItemId=999">ignore</a>
        "#;
        assert_eq!(
            parse_series_page_item_ids(html),
            vec![322382456, 383428338]
        );
    }

    #[test]
    fn merges_zero_padded_sangha_volumes() {
        let base = "TRPG 플레이어가 이세계에서 최강 빌드를 목표로 하다";
        let items = vec![
            item(&format!("{base} 1 - 시프트노벨"), 1),
            item(&format!("{base} 08"), 2),
            item(&format!("{base} 09 상"), 3),
            item(&format!("{base} 09 (상)"), 4),
            item(&format!("{base} 09 (하) - 시프트노벨"), 5),
            item(&format!("{base} 9권(상)"), 6),
        ];
        let groups = group_by_series(items);
        assert_eq!(groups.len(), 1, "groups: {:?}", groups.iter().map(|g| &g.title).collect::<Vec<_>>());
        assert_eq!(groups[0].title, base);
        // 09 상 / 09 (상) / 9권(상)은 같은 권으로 dedupe
        assert_eq!(groups[0].items.len(), 4);
        assert_eq!(
            normalize_series_title(&format!("{base} 09 (상)")),
            base
        );
        assert_eq!(extract_volume_parts(&format!("{base} 09 (상)")), Some((9, 0)));
        assert_eq!(extract_volume_part(&format!("{base} 09 (상)")), 1);
        assert_eq!(
            format_volume_label_from_title(&format!("{base} 09 (상)"), 3, 0),
            "9상"
        );
        assert_eq!(
            format_volume_label_from_title(&format!("{base} 9권(상)"), 3, 0),
            "9상"
        );
    }

    #[test]
    fn merges_sss_without_volume_number() {
        let items = vec![
            item("패배 히로인이 너무 많아! 1 - Novel Engine", 1),
            item("패배 히로인이 너무 많아! SSS - Novel Engine", 2),
        ];
        let groups = group_by_series(items);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "패배 히로인이 너무 많아!");
        assert_eq!(groups[0].items.len(), 2);
        assert_eq!(
            format_volume_label_from_title(&groups[0].items[1].title, 2, 1),
            "SSS"
        );
    }

    #[test]
    fn merges_ex_into_main_series() {
        let items = vec![
            item_with_date("데스마치에서 시작되는 이세계 광상곡 1 - L Novel", 1, "2015-08-01"),
            item_with_date("데스마치에서 시작되는 이세계 광상곡 2 - L Novel", 2, "2015-11-01"),
            item_with_date("데스마치에서 시작되는 이세계 광상곡 Ex - L Novel", 3, "2018-01-01"),
            item_with_date("데스마치에서 시작되는 이세계 광상곡 Ex 2 - L Novel", 4, "2021-08-01"),
            item_with_date("데스마치에서 시작되는 이세계 광상곡 Ex３ - L Novel", 5, "2024-05-01"),
        ];
        let groups = group_by_series(items);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "데스마치에서 시작되는 이세계 광상곡");
        assert_eq!(groups[0].items.len(), 5);
        assert_eq!(
            normalize_series_title("데스마치에서 시작되는 이세계 광상곡 Ex 2 - L Novel"),
            "데스마치에서 시작되는 이세계 광상곡"
        );
        assert_eq!(
            normalize_series_title("데스마치에서 시작되는 이세계 광상곡 Ex３ - L Novel"),
            "데스마치에서 시작되는 이세계 광상곡"
        );
        assert_eq!(
            format_volume_label_from_title(
                "데스마치에서 시작되는 이세계 광상곡 Ex - L Novel",
                3,
                3
            ),
            "Ex"
        );
        assert_eq!(
            format_volume_label_from_title(
                "데스마치에서 시작되는 이세계 광상곡 Ex 2 - L Novel",
                4,
                3
            ),
            "Ex 2권"
        );
        assert_eq!(
            format_volume_label_from_title(
                "데스마치에서 시작되는 이세계 광상곡 Ex３ - L Novel",
                5,
                3
            ),
            "Ex 3권"
        );
    }

    #[test]
    fn volume_identity_key_treats_zero_padded_as_same() {
        assert_eq!(
            volume_identity_key("마녀와 용병 4 - S Novel+"),
            volume_identity_key("마녀와 용병 04 - S Novel+")
        );
        assert_ne!(
            volume_identity_key("마녀와 용병 4 - S Novel+"),
            volume_identity_key("마녀와 용병 5 - S Novel+")
        );
    }

    #[test]
    fn orders_by_title_volume_when_no_arc() {
        // 개정판 출간일이 늦어도 제목 권수 순으로 정렬
        let items = vec![
            item_with_date("소드 아트 온라인 1 - J Novel", 1, "2009-12-10"),
            item_with_date("소드 아트 온라인 3 - J Novel", 3, "2010-06-10"),
            item_with_date("소드 아트 온라인 4 - J Novel", 4, "2010-12-10"),
            item_with_date("소드 아트 온라인 2 - J Novel", 2, "2012-05-01"),
        ];
        let nums = assign_volume_numbers(&items);
        let titles: Vec<_> = nums.iter().map(|(i, n)| (i.title.as_str(), *n)).collect();
        assert_eq!(
            titles,
            vec![
                ("소드 아트 온라인 1 - J Novel", 1),
                ("소드 아트 온라인 2 - J Novel", 2),
                ("소드 아트 온라인 3 - J Novel", 3),
                ("소드 아트 온라인 4 - J Novel", 4),
            ]
        );
    }

    #[test]
    fn assign_volume_numbers_are_unique() {
        let special = vec![item("무직전생 스페셜북 - Premium Extreme Novel", 1)];
        let nums = assign_volume_numbers_by_release(&special);
        assert_eq!(nums.len(), 1);
        assert_eq!(nums[0].1, 1);
    }

    #[test]
    fn cleans_extreme_novel_wanetsu_and_bonus() {
        assert_eq!(
            clean_volume_title("천경의 알데라민 14 - 태엽 감는 정령전기, Extreme Novel, 완결"),
            "천경의 알데라민 14 - 태엽 감는 정령전기"
        );
        assert_eq!(
            clean_volume_title(
                "경험 많은 너와 경험 없는 내가 사귀게 된 이야기. 10 - 완결 /초판 한정 북마크 1종"
            ),
            "경험 많은 너와 경험 없는 내가 사귀게 된 이야기. 10"
        );
        assert_eq!(
            normalize_series_title(
                "천경의 알데라민 14 - 태엽 감는 정령전기, Extreme Novel, 완결"
            ),
            "천경의 알데라민"
        );
    }

    #[test]
    fn merges_half_volumes_and_school_year_arcs() {
        let items = vec![
            item("어서 오세요 실력지상주의 교실에 7 - S Novel", 1),
            item("어서 오세요 실력지상주의 교실에 7.5 - S Novel", 2),
            item("어서 오세요 실력지상주의 교실에 2학년 편 12.5 - S Novel", 3),
            item("어서 오세요 실력지상주의 교실에 2학년 편 1 - S Novel", 6),
            item("어서 오세요 실력지상주의 교실에 1 - S Novel", 7),
            item("어서 오세요 실력지상주의 교실에 9 (달력 한정판) - S Novel", 4),
            item(
                "어서 오세요 실력지상주의 교실에 아트북 - 토모세 슌사쿠 Art Works",
                5,
            ),
        ];
        let groups = group_by_series(items);
        let titles: Vec<_> = groups.iter().map(|g| g.title.as_str()).collect();
        assert!(titles.contains(&"어서 오세요 실력지상주의 교실에"));
        assert!(titles.contains(&"어서 오세요 실력지상주의 교실에 Art Works"));
        let main = groups
            .iter()
            .find(|g| g.title == "어서 오세요 실력지상주의 교실에")
            .unwrap();
        // 달력 한정 9는 일반판이 없어 유지, Art Works는 별도
        assert_eq!(main.items.len(), 6);
        assert_eq!(
            format_volume_label_from_title("어서 오세요 실력지상주의 교실에 7.5 - S Novel", 2, 0),
            "7.5권"
        );
        assert_eq!(
            format_volume_label_from_title(
                "어서 오세요 실력지상주의 교실에 2학년 편 12.5 - S Novel",
                3,
                0
            ),
            "2학년 12.5권"
        );
        assert_eq!(
            normalize_series_title(
                "어서 오세요 실력지상주의 교실에 2학년 편 : Start 토모세슌사쿠 Art Works - S Novel"
            ),
            "어서 오세요 실력지상주의 교실에 Art Works"
        );
    }

    #[test]
    fn keeps_all_classroom_volumes_across_years() {
        let titles = [
            "어서 오세요 실력지상주의 교실에 1 - S Novel",
            "어서 오세요 실력지상주의 교실에 2 - S Novel",
            "어서 오세요 실력지상주의 교실에 3 - S Novel",
            "어서 오세요 실력지상주의 교실에 4 - S Novel",
            "어서 오세요 실력지상주의 교실에 4.5 - S Novel",
            "어서 오세요 실력지상주의 교실에 5 - S Novel",
            "어서 오세요 실력지상주의 교실에 6 - S Novel",
            "어서 오세요 실력지상주의 교실에 7 - S Novel",
            "어서 오세요 실력지상주의 교실에 7.5 - S Novel",
            "어서 오세요 실력지상주의 교실에 8 - S Novel",
            "어서 오세요 실력지상주의 교실에 9 - S Novel",
            "어서 오세요 실력지상주의 교실에 10 - S Novel",
            "어서 오세요 실력지상주의 교실에 11 - S Novel",
            "어서 오세요 실력지상주의 교실에 11.5 - S Novel",
            "어서 오세요 실력지상주의 교실에 2학년 편 1 - S Novel",
            "어서 오세요 실력지상주의 교실에 2학년 편 2 - S Novel",
            "어서 오세요 실력지상주의 교실에 2학년 편 3 - S Novel",
            "어서 오세요 실력지상주의 교실에 2학년 편 4 - S Novel",
            "어서 오세요 실력지상주의 교실에 2학년 편 4.5 - S Novel",
            "어서 오세요 실력지상주의 교실에 2학년 편 5 - S Novel",
            "어서 오세요 실력지상주의 교실에 2학년 편 6 - S Novel",
            "어서 오세요 실력지상주의 교실에 2학년 편 7 - S Novel",
            "어서 오세요 실력지상주의 교실에 2학년 편 8 - S Novel",
            "어서 오세요 실력지상주의 교실에 2학년 편 9 - S Novel",
            "어서 오세요 실력지상주의 교실에 2학년 편 9.5 - S Novel",
            "어서 오세요 실력지상주의 교실에 2학년 편 10 - S Novel",
            "어서 오세요 실력지상주의 교실에 2학년 편 11 - S Novel",
            "어서 오세요 실력지상주의 교실에 2학년 편 12 - S Novel",
            "어서 오세요 실력지상주의 교실에 2학년 편 12.5 - S Novel",
            "어서 오세요 실력지상주의 교실에 3학년 편 1 - S Novel",
            "어서 오세요 실력지상주의 교실에 3학년 편 2 - S Novel",
            "어서 오세요 실력지상주의 교실에 3학년 편 3 - S Novel 초판 한정 책갈피, 쇼트스토리",
            // noise
            "어서 오세요 실력지상주의 교실에 5 (한정 박스판) - S Novel",
            "어서 오세요 실력지상주의 교실에 9 (달력 한정판) - S Novel",
            "어서 오세요 실력지상주의 교실에 1학년 편 (박스 한정판) - S Novel",
            "어서 오세요 실력지상주의 교실에 1학년 편 공식 가이드북 First File - S Novel",
            "어서 오세요 실력지상주의 교실에 아트북 - 토모세 슌사쿠 Art Works",
        ];
        let items: Vec<_> = titles
            .iter()
            .enumerate()
            .map(|(i, t)| item(t, i as i64 + 1))
            .collect();
        let groups = group_by_series(items);
        let main = groups
            .iter()
            .find(|g| g.title == "어서 오세요 실력지상주의 교실에")
            .expect("main series");
        assert_eq!(main.items.len(), 32);
    }

    #[test]
    fn prefers_standard_over_christmas_edition() {
        let items = vec![
            item_with_date(
                "전생했더니 슬라임이었던 건에 대하여 1 : 크리스마스 에디션 - S Novel+",
                1,
                "2019-12-01",
            ),
            item_with_date(
                "전생했더니 슬라임이었던 건에 대하여 1 - S Novel+",
                2,
                "2015-01-01",
            ),
        ];
        let groups = group_by_series(items);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].items.len(), 1);
        assert!(
            !groups[0].items[0].title.contains("크리스마스"),
            "got {}",
            groups[0].items[0].title
        );
        assert!(is_limited_edition(
            "전생했더니 슬라임이었던 건에 대하여 1 : 크리스마스 에디션 - S Novel+"
        ));
        assert_eq!(
            clean_volume_title(
                "전생했더니 슬라임이었던 건에 대하여 1 : 크리스마스 에디션 - S Novel+"
            ),
            "전생했더니 슬라임이었던 건에 대하여 1"
        );
    }

    #[test]
    fn ignores_limited_editions() {
        let items = vec![
            item(
                "경험 많은 너와 경험 없는 내가 사귀게 된 이야기. 8 (초회판)",
                1,
            ),
            item("경험 많은 너와 경험 없는 내가 사귀게 된 이야기. 8", 2),
            item(
                "경험 많은 너와 경험 없는 내가 사귀게 된 이야기. 10 (한정판) - 완결 /초판 한정 북마크 + 한정판 부록 아크릴 스탠드",
                3,
            ),
            item(
                "경험 많은 너와 경험 없는 내가 사귀게 된 이야기. 10 - 완결 /초판 한정 북마크 1종",
                4,
            ),
            // 6권은 초회판만 존재 → 남겨야 함
            item(
                "경험 많은 너와 경험 없는 내가 사귀게 된 이야기. 6 (초회판)",
                5,
            ),
        ];
        let groups = group_by_series(items);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].items.len(), 3);
        // 일반판이 있으면 초회/한정 탈락
        assert!(groups[0].items.iter().any(|i| i.title.contains(" 8") && !i.title.contains("초회판")));
        assert!(groups[0].items.iter().any(|i| i.title.contains(" 6 (초회판)")));
    }

    #[test]
    fn uses_bundle_date_as_earlier_hint() {
        let mut items = vec![
            item_with_date(
                "무직전생 26 - 이세계에 갔으면 최선을 다한다, Premium Extreme Novel",
                1,
                "2026-07-20",
            ),
            item_with_date(
                "무직전생 26 + 무직전생 스페셜북 합본 세트 - 전2권 - Premium Extreme Novel",
                2,
                "2024-04-25",
            ),
        ];
        let hints = collect_earliest_volume_dates(&items);
        apply_earliest_dates(&mut items, &hints);
        assert_eq!(items[0].pub_date, "2024-04-25");
        assert_eq!(
            extract_bundle_primary_volume(
                "무직전생 26 + 무직전생 스페셜북 합본 세트 - 전2권 - Premium Extreme Novel"
            ),
            Some((26, 0))
        );
    }

    #[test]
    fn merges_bookworm_parts_and_excludes_special() {
        let items = vec![
            item("책벌레의 하극상 제1부 병사의 딸 1 - 사서가 되기 위해서라면 뭐든지 할 수 있어!, V+", 1),
            item("책벌레의 하극상 제2부 신전의 견습무녀 1 - V+", 2),
            item("책벌레의 하극상 제5부 : 여신의 화신 10 - 사서가 되기 위해서라면 뭐든지 할 수 있어, V+", 3),
            item("책벌레의 하극상 제1부 병사의 딸 3 (특별판) - 사서가 되기 위해서라면 뭐든지 할 수 있어!, V+", 4),
            item("책벌레의 하극상 제1부 병사의 딸 3 (일반판) - 사서가 되기 위해서라면 뭐든지 할 수 있어!, V+", 5),
            item("책벌레의 하극상 오피셜 팬북 2 - 사서가 되기 위해서라면 뭐든지 할 수 있어, V+", 6),
        ];
        let groups = group_by_series(items);
        let titles: Vec<_> = groups.iter().map(|g| g.title.as_str()).collect();
        assert!(titles.contains(&"책벌레의 하극상"));
        assert!(titles.contains(&"책벌레의 하극상 오피셜 팬북"));
        let main = groups.iter().find(|g| g.title == "책벌레의 하극상").unwrap();
        assert_eq!(main.items.len(), 4); // 특별판 제외, 1부1·2부1·5부10·1부3
        assert_eq!(
            format_volume_label_from_title(
                "책벌레의 하극상 제5부 : 여신의 화신 10 - 사서가 되기 위해서라면 뭐든지 할 수 있어, V+",
                10,
                0
            ),
            "5부 10권"
        );
        assert_eq!(
            normalize_series_title("책벌레의 하극상 제3부 영주의 양녀 2 - V+"),
            "책벌레의 하극상"
        );
    }

    #[test]
    fn merges_seishun_buta_franchise() {
        let items = vec![
            item("청춘 돼지는 바니걸 선배의 꿈을 꾸지 않는다 - L Novel", 1),
            item("청춘 돼지는 소악마 후배의 꿈을 꾸지 않는다 - L Novel", 2),
            item("청춘 돼지는 책가방 소녀의 꿈을 꾸지 않는다 (특별판) - L Novel", 3),
            item("청춘 돼지는 책가방 소녀의 꿈을 꾸지 않는다 - L Novel", 4),
        ];
        let groups = group_by_series(items);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "청춘 돼지 시리즈");
        assert_eq!(groups[0].items.len(), 3); // 특별판 제외
        assert_eq!(
            normalize_series_title("청춘 돼지는 꿈꾸는 소녀의 꿈을 꾸지 않는다 - L Novel"),
            "청춘 돼지 시리즈"
        );
    }

    #[test]
    fn strips_jm_novel_and_groups_single_volume() {
        assert_eq!(
            normalize_series_title("여자친구의 여동생과 키스를 했다 - JM 노벨"),
            "여자친구의 여동생과 키스를 했다"
        );
        assert_eq!(
            clean_volume_title("여자친구의 여동생과 키스를 했다 - JM 노벨"),
            "여자친구의 여동생과 키스를 했다"
        );
        let items = vec![item("여자친구의 여동생과 키스를 했다 - JM 노벨", 1)];
        let groups = group_by_series(items);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "여자친구의 여동생과 키스를 했다");
        assert_eq!(groups[0].items.len(), 1);
    }

    #[test]
    fn cleans_package_suffix_and_html_entities() {
        assert_eq!(
            clean_volume_title(
                "옆집 천사님 때문에 어느샌가 인간적으로 타락한 사연 11.5 - Novel Engine, ①특별판 전용 커버 사양 11.5권 본편 ②별책 단편집 (※커버 없음) ③전용 슬리브 타입 케이스"
            ),
            "옆집 천사님 때문에 어느샌가 인간적으로 타락한 사연 11.5"
        );
        assert!(is_limited_edition(
            "옆집 천사님 때문에 어느샌가 인간적으로 타락한 사연 8.5 특별판 (소책자 포함) - Novel Engine"
        ));
        let items = vec![
            item(
                "옆집 천사님 때문에 어느샌가 인간적으로 타락한 사연 8.5 - Novel Engine",
                1,
            ),
            item(
                "옆집 천사님 때문에 어느샌가 인간적으로 타락한 사연 8.5 특별판 (소책자 포함) - Novel Engine",
                2,
            ),
            item(
                "옆집 천사님 때문에 어느샌가 인간적으로 타락한 사연 11.5 - Novel Engine",
                3,
            ),
            item(
                "옆집 천사님 때문에 어느샌가 인간적으로 타락한 사연 11.5 - Novel Engine, ①특별판 전용 커버 사양 11.5권 본편 ②별책",
                4,
            ),
        ];
        let groups = group_by_series(items);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].items.len(), 2);
        assert!(groups[0].items.iter().all(|i| !i.title.contains("특별판") && !i.title.contains("①")));
        assert_eq!(
            clean_volume_title(
                "기교소녀는 상처받지 않아 12 - Facing &quot;Master's Doll&quot;, NT Novel"
            ),
            "기교소녀는 상처받지 않아 12 - Facing Master's Doll"
        );
        assert_eq!(
            normalize_series_title(
                "기교소녀는 상처받지 않아 12 - Facing &quot;Master's Doll&quot;, NT Novel"
            ),
            "기교소녀는 상처받지 않아"
        );
    }

    #[test]
    fn prefers_original_over_revision_edition() {
        let items = vec![
            item_with_date(
                "Re : 제로부터 시작하는 이세계 생활 1 - Novel Engine",
                1,
                "2014-10-01",
            ),
            item_with_date(
                "Re : 제로부터 시작하는 이세계 생활 1 - 개정판, Novel Engine",
                2,
                "2025-04-21",
            ),
            item_with_date(
                "Re : 제로부터 시작하는 이세계 생활 2 - Novel Engine",
                3,
                "2014-12-01",
            ),
        ];
        let groups = group_by_series(items);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].items.len(), 2);
        assert!(groups[0]
            .items
            .iter()
            .all(|i| !i.title.contains("개정판")));
        assert!(rules().is_revision_edition(
            "Re : 제로부터 시작하는 이세계 생활 1 - 개정판, Novel Engine"
        ));
    }
}
