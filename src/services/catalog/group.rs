//! Shared catalog volume grouping and title helpers.
//!
//! [`yes24`](super::yes24) maps API hits into [`CatalogVolume`]; these helpers
//! group series, filter LN candidates, and reconcile volume numbers.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use regex::Regex;
use uuid::Uuid;

use crate::{
    error::AppResult,
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

/// Domestic LN imprint brands (comics labels excluded).
pub(crate) const LN_IMPRINTS: &[&str] = &[
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

#[derive(Debug, Clone)]
pub struct CatalogVolume {
    pub title: String,
    pub author: String,
    pub publisher: String,
    pub pub_date: String,
    pub isbn13: String,
    /// Synthetic numeric id for sorting/dedupe within a session.
    pub item_id: i64,
    pub cover: String,
    pub category_name: Option<String>,
    /// Identity stored on `volumes.aladin_item_id` (`isbn:…` / `yes24:…`).
    pub stored_item_id: Option<String>,
}

fn category_parts(cat: &str) -> Vec<&str> {
    cat.split(['>', '-'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

/// Yes24 files both comics and light novels under `만화/라이트노벨`.
/// That shelf name alone is not a comic or light-novel verdict.
fn is_mixed_shelf(part: &str) -> bool {
    let compact: String = part.chars().filter(|c| !c.is_whitespace()).collect();
    compact.contains("만화") && compact.contains("라이트")
}

fn is_ln_part(part: &str) -> bool {
    if is_mixed_shelf(part) {
        return false;
    }
    let compact: String = part.chars().filter(|c| !c.is_whitespace()).collect();
    compact.contains("라이트노벨")
}

fn is_comic_part(part: &str) -> bool {
    if is_mixed_shelf(part) {
        return false;
    }
    part.contains("만화") || part.contains("코믹")
}

fn title_marks_comic(title: &str) -> bool {
    let title = title.trim();
    title.contains("[만화]")
        || title.contains("[코믹]")
        || title.contains("(만화)")
        || title.contains("(코믹)")
        || title.starts_with("코믹 ")
        || title.starts_with("만화 ")
}

/// Juvenile shelves. Checked on category segments so an LN imprint cannot keep a picture book.
const JUVENILE_MARKERS: &[&str] = &[
    "유아",
    "어린이",
    "아동",
    "그림책",
    "동화",
    "키즈",
    "보드북",
    "사운드북",
    "학습만화",
    "초등",
];

fn category_is_juvenile(cat: &str) -> bool {
    category_parts(cat)
        .iter()
        .any(|part| JUVENILE_MARKERS.iter().any(|marker| part.contains(marker)))
}

fn title_marks_juvenile(title: &str) -> bool {
    let title = title.trim();
    title.contains("[어린이]")
        || title.contains("[유아]")
        || title.contains("[아동]")
        || title.contains("[동화]")
        || title.contains("(어린이)")
        || title.contains("(유아)")
        || title.contains("(아동)")
        || title.contains("(동화)")
        || title.starts_with("어린이 ")
        || title.starts_with("유아 ")
}

impl CatalogVolume {
    fn is_light_novel(&self) -> bool {
        self.category_name
            .as_deref()
            .map(|name| category_parts(name).iter().any(|part| is_ln_part(part)))
            .unwrap_or(false)
    }

    /// Comic leaf, or the mixed shelf with no light-novel leaf under it.
    fn category_is_comic(&self) -> bool {
        let Some(cat) = self.category_name.as_deref() else {
            return false;
        };
        let parts = category_parts(cat);
        if parts.iter().any(|part| is_comic_part(part)) {
            return true;
        }
        parts.iter().any(|part| is_mixed_shelf(part)) && !parts.iter().any(|part| is_ln_part(part))
    }

    /// True for LN candidates: a light-novel leaf, an LN imprint, or a genre label.
    /// `만화/라이트노벨` is only the parent shelf, so it does not reject a `라이트노벨` leaf.
    /// Juvenile categories are rejected even when the publisher is an LN imprint.
    pub(crate) fn is_catalog_candidate(&self) -> bool {
        if title_marks_comic(&self.title) || title_marks_juvenile(&self.title) {
            return false;
        }
        if self
            .category_name
            .as_deref()
            .is_some_and(category_is_juvenile)
        {
            return false;
        }
        if self.category_is_comic() {
            return false;
        }
        if self.is_light_novel() {
            return true;
        }
        if LN_IMPRINTS
            .iter()
            .any(|imprint| self.publisher.contains(imprint))
        {
            return true;
        }
        let cat = self.category_name.as_deref().unwrap_or("");
        cat.contains("장르소설")
    }
}

#[derive(Debug, Clone)]
pub struct GroupedSeries {
    pub aladin_series_id: String,
    pub title: String,
    pub author: Option<String>,
    pub publisher: Option<String>,
    pub cover_url: Option<String>,
    pub items: Vec<CatalogVolume>,
}

pub fn group_by_series(items: Vec<CatalogVolume>) -> Vec<GroupedSeries> {
    group_by_series_with(items, default_rules())
}

fn is_ebook(item: &CatalogVolume) -> bool {
    if let Some(cat) = item.category_name.as_deref() {
        let compact: String = cat.chars().filter(|c| !c.is_whitespace()).collect();
        if compact.to_ascii_lowercase().contains("ebook") || cat.contains("전자책") {
            return true;
        }
    }
    let title = item.title.as_str();
    title.contains("[전자책]")
        || title.contains("(전자책)")
        || title.contains("[eBook]")
        || title.contains("(eBook)")
}

fn ebook_covered_by_paper(ebook: &CatalogVolume, papers: &[&CatalogVolume]) -> bool {
    if papers.is_empty() {
        return false;
    }
    if let Some(key) = volume_identity_key(&ebook.title) {
        return papers
            .iter()
            .any(|paper| volume_identity_key(&paper.title) == Some(key));
    }
    let series = normalize_series_title(&ebook.title);
    !series.is_empty()
        && papers
            .iter()
            .any(|paper| normalize_series_title(&paper.title) == series)
}

/// Ebooks whose paper edition is in `items`. An ebook with no volume number matches
/// a paper volume of the same series name (Yes24 often omits `1` on the ebook).
pub(crate) fn ebooks_covered_by_paper(items: &[CatalogVolume]) -> Vec<CatalogVolume> {
    let papers: Vec<&CatalogVolume> = items.iter().filter(|item| !is_ebook(item)).collect();
    items
        .iter()
        .filter(|item| is_ebook(item) && ebook_covered_by_paper(item, &papers))
        .cloned()
        .collect()
}

fn without_ebooks_covered_by_paper(items: Vec<CatalogVolume>) -> Vec<CatalogVolume> {
    let covered: HashSet<String> = ebooks_covered_by_paper(&items)
        .iter()
        .map(volume_external_id)
        .collect();
    if covered.is_empty() {
        return items;
    }
    items
        .into_iter()
        .filter(|item| !covered.contains(&volume_external_id(item)))
        .collect()
}

pub fn group_by_series_with(items: Vec<CatalogVolume>, title_rules: &TitleRules) -> Vec<GroupedSeries> {
    let items = without_ebooks_covered_by_paper(items);
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


pub(crate) fn volume_external_id(item: &CatalogVolume) -> String {
    item.stored_item_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| item.item_id.to_string())
}


pub(crate) async fn drop_bundle_volumes(state: &AppState, series_id: Uuid) -> AppResult<()> {
    let volumes = repositories::list_volumes(&state.pool, series_id, "asc").await?;
    let rules = rules_of(state);
    for vol in volumes {
        if rules.is_bundle_or_set(&vol.title) {
            repositories::delete_volume(&state.pool, vol.id, series_id).await?;
        }
    }
    Ok(())
}

pub(crate) fn select_group<'a>(
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

#[allow(dead_code)]
fn search_query_for_aladin_key(key: &str) -> Option<String> {
    let title = key.strip_prefix("title:")?;
    let query = normalize_series_title(title);
    if query.is_empty() {
        None
    } else {
        Some(query)
    }
}

#[allow(dead_code)]
fn union_groups_for_series(
    groups: &[GroupedSeries],
    identity_keys: &HashSet<String>,
    known_item_ids: &HashSet<i64>,
) -> Option<GroupedSeries> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    let mut meta: Option<GroupedSeries> = None;

    for group in groups {
        let title_key = format!("title:{}", normalize_series_title(&group.title));
        let matches_identity =
            identity_keys.contains(&group.aladin_series_id) || identity_keys.contains(&title_key);
        let matches_item = group
            .items
            .iter()
            .any(|item| known_item_ids.contains(&item.item_id));
        if !matches_identity && !matches_item {
            continue;
        }
        if meta.is_none() {
            meta = Some(GroupedSeries {
                aladin_series_id: group.aladin_series_id.clone(),
                title: group.title.clone(),
                author: group.author.clone(),
                publisher: group.publisher.clone(),
                cover_url: group.cover_url.clone(),
                items: Vec::new(),
            });
        }
        for item in &group.items {
            if seen.insert(item.item_id) {
                items.push(item.clone());
            }
        }
    }

    let mut group = meta?;
    if items.is_empty() {
        return None;
    }
    group.items = items;
    Some(group)
}

pub(crate) fn non_empty(value: &str) -> Option<String> {
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
fn dedupe_volume_items(items: Vec<CatalogVolume>) -> Vec<CatalogVolume> {
    // (school_year, story_part, major, minor, sangha, sss_arc)
    let mut by_key: HashMap<(u8, u8, i64, u8, u8, u8), Vec<CatalogVolume>> = HashMap::new();
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

    let mut result: Vec<CatalogVolume> = by_key
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
fn dedupe_unnumbered_editions(items: Vec<CatalogVolume>) -> Vec<CatalogVolume> {
    let mut by_key: HashMap<String, Vec<CatalogVolume>> = HashMap::new();
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

fn edition_preference(item: &CatalogVolume) -> i32 {
    rules().edition_preference(&item.title)
}

/// DB용 권번호.
/// - SSS/Ex 등 아크가 섞이면 출간일 순(아크가 본편 사이에 끼도록).
/// - 그 외에는 제목에서 뽑은 권수를 우선(개정판 출간일로 순서가 꼬이는 경우 방지).
pub(crate) fn assign_volume_numbers(items: &[CatalogVolume]) -> Vec<(CatalogVolume, i64)> {
    if items.iter().any(|i| extract_series_arc(&i.title) > 0) {
        return assign_volume_numbers_by_release(items);
    }
    if let Some(by_title) = try_assign_volume_numbers_by_title(items) {
        return by_title;
    }
    assign_volume_numbers_by_release(items)
}

fn try_assign_volume_numbers_by_title(items: &[CatalogVolume]) -> Option<Vec<(CatalogVolume, i64)>> {
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
fn assign_volume_numbers_by_release(items: &[CatalogVolume]) -> Vec<(CatalogVolume, i64)> {
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
/// 권번호가 없는 항목(단편집·SS집 등)은 정규화 제목으로 묶는다.
pub(crate) async fn collapse_duplicate_volumes_by_identity(
    state: &AppState,
    series_id: Uuid,
) -> AppResult<()> {
    let volumes = repositories::list_volumes(&state.pool, series_id, "asc").await?;
    let mut by_key: HashMap<(u8, u8, i64, u8, u8, u8), Vec<_>> = HashMap::new();
    let mut by_title: HashMap<String, Vec<_>> = HashMap::new();
    for vol in volumes {
        if let Some(key) = volume_identity_key(&vol.title) {
            by_key.entry(key).or_default().push(vol);
        } else {
            let tkey = unnumbered_title_key(&vol.title);
            if !tkey.is_empty() {
                by_title.entry(tkey).or_default().push(vol);
            }
        }
    }

    for mut group in by_key
        .into_values()
        .chain(by_title.into_values())
        .filter(|g| g.len() >= 2)
    {
        group.sort_by(|a, b| {
            edition_preference_title(&b.title)
                .cmp(&edition_preference_title(&a.title))
                .then_with(|| {
                    prefer_non_ebook_cover(&b.cover_url).cmp(&prefer_non_ebook_cover(&a.cover_url))
                })
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

/// Drop an ebook row already stored when a paper volume of the same work is in the series.
pub(crate) async fn remove_stored_ebooks_covered_by_paper(
    state: &AppState,
    series_id: Uuid,
    covered: &[CatalogVolume],
) -> AppResult<()> {
    if covered.is_empty() {
        return Ok(());
    }
    let volumes = repositories::list_volumes(&state.pool, series_id, "asc").await?;
    let mut removed = false;
    for ebook in covered {
        let Some(stored) =
            repositories::find_volume_by_aladin_item_id(&state.pool, &volume_external_id(ebook))
                .await?
        else {
            continue;
        };
        if stored.series_id != series_id {
            continue;
        }
        let keep = volumes
            .iter()
            .filter(|vol| vol.id != stored.id && paper_title_covers(&ebook.title, &vol.title))
            .min_by_key(|vol| (extract_volume_parts(&vol.title).is_none(), vol.volume_number));
        if let Some(keep) = keep {
            repositories::merge_and_delete_volume(&state.pool, series_id, keep.id, stored.id)
                .await?;
        } else {
            repositories::delete_volume(&state.pool, stored.id, series_id).await?;
        }
        removed = true;
    }
    if removed {
        repositories::refresh_series_publish_dates(&state.pool, series_id).await?;
    }
    Ok(())
}

fn paper_title_covers(ebook_title: &str, paper_title: &str) -> bool {
    if let Some(key) = volume_identity_key(ebook_title) {
        return volume_identity_key(paper_title) == Some(key);
    }
    let series = normalize_series_title(ebook_title);
    !series.is_empty() && normalize_series_title(paper_title) == series
}

fn unnumbered_title_key(title: &str) -> String {
    clean_volume_title(title).to_lowercase()
}

fn prefer_non_ebook_cover(cover: &Option<String>) -> i32 {
    // Leftover Aladin cover filenames: ebook names often start with `e`, paper with `k`.
    // When collapsing duplicates, prefer the paper-looking URL. Yes24 paths (`…/L`) do not.
    match cover.as_deref().and_then(|u| u.rsplit('/').next()) {
        Some(name) if name.to_ascii_lowercase().starts_with('e') => 0,
        Some(_) => 1,
        None => 0,
    }
}

fn edition_preference_title(title: &str) -> i32 {
    rules().edition_preference(title)
}

/// 이미 저장된 권을 제목 권수 기준으로 재정렬한다.
/// SSS/Ex 아크가 있거나 권수 추출이 안 되면 손대지 않는다.
pub(crate) async fn reconcile_volume_numbers_from_titles(
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

pub(crate) fn parse_pub_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

/// 본권·합본·다른 에디션을 모두 보고 (학년,권수…)별 가장 이른 출간일을 모은다.
#[allow(dead_code)]
fn collect_earliest_volume_dates(
    items: &[CatalogVolume],
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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
#[allow(dead_code)]
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

#[allow(dead_code)]
fn apply_earliest_dates(
    items: &mut [CatalogVolume],
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

    fn item(title: &str, id: i64) -> CatalogVolume {
        CatalogVolume {
            title: title.into(),
            author: "작가".into(),
            publisher: "출판".into(),
            pub_date: "2020-01-01".into(),
            isbn13: String::new(),
            item_id: id,
            cover: "a".into(),
            category_name: Some("국내도서>만화/라이트노벨>라이트 노벨>S 노벨".into()),
            stored_item_id: None,
        }
    }

    #[test]
    fn treats_total_volume_set_as_bundle() {
        assert!(is_bundle_or_set(
            "[세트] 29세 독신은 이세계에서 자유롭게 살고…… 싶었다. (총10권/완결)"
        ));
        assert!(is_bundle_or_set(
            "29세 독신은 이세계에서 자유롭게 살고…… 싶었다. (총10권/완결)"
        ));
        assert!(is_bundle_or_set(
            "[묶음] Re : 제로부터 시작하는 이세계 생활 (총43권/미완결)"
        ));
        assert!(!is_bundle_or_set(
            "29세 독신은 이세계에서 자유롭게 살고…… 싶었다. 10 (완결)"
        ));
        assert_eq!(
            extract_volume_parts(
                "29세 독신은 이세계에서 자유롭게 살고…… 싶었다. (총10권/완결)"
            ),
            None
        );
    }

    #[test]
    fn catalog_candidate_keeps_ln_ebook_imprint() {
        let mut ebook = item("어떤 라노베 1", 1);
        ebook.category_name = Some("전자책>소설".into());
        ebook.publisher = "시프트노벨".into();
        assert!(ebook.is_catalog_candidate());
    }

    #[test]
    fn catalog_candidate_rejects_comic_category() {
        let mut comic = item("눈을 떴더니 최강 무장 01", 2);
        comic.category_name = Some("전자책>만화".into());
        comic.publisher = "시프트코믹스".into();
        assert!(!comic.is_catalog_candidate());
    }

    #[test]
    fn catalog_candidate_rejects_unknown_publisher_without_ln_signal() {
        let mut general = item("클린 코드", 3);
        general.category_name = Some("국내도서".into());
        general.publisher = "인사이트".into();
        assert!(!general.is_catalog_candidate());
    }

    #[test]
    fn catalog_candidate_keeps_ln_imprint_without_category() {
        let mut ln = item("마녀와 용병 1", 4);
        ln.category_name = Some("국내도서".into());
        ln.publisher = "시프트노벨".into();
        assert!(ln.is_catalog_candidate());
    }

    #[test]
    fn catalog_candidate_rejects_comic_title_marker() {
        let mut comic = item("[만화] 어떤 작품 1", 5);
        comic.category_name = Some("국내도서".into());
        comic.publisher = "디앤씨미디어".into();
        assert!(!comic.is_catalog_candidate());
    }

    #[test]
    fn catalog_candidate_keeps_yes24_light_novel_under_comic_shelf() {
        let mut novel = item("이세계 식당 1", 6);
        novel.category_name = Some("국내도서-만화/라이트노벨-라이트노벨".into());
        novel.publisher = "디앤씨미디어(D&C미디어)".into();
        assert!(novel.is_catalog_candidate());
    }

    #[test]
    fn catalog_candidate_rejects_yes24_comic_on_mixed_shelf() {
        let mut comic = item("코믹 이세계 식당 1", 7);
        comic.category_name = Some("국내도서-만화/라이트노벨".into());
        comic.publisher = "디앤씨미디어(D&C미디어)".into();
        assert!(!comic.is_catalog_candidate());
    }

    #[test]
    fn catalog_candidate_rejects_juvenile_even_from_ln_imprint() {
        let mut picture = item("구름빵", 8);
        picture.category_name = Some("국내도서-어린이-그림책".into());
        picture.publisher = "서울문화사".into();
        assert!(!picture.is_catalog_candidate());

        let mut fairy = item("유아 동화 1", 9);
        fairy.category_name = Some("국내도서-유아-동화".into());
        fairy.publisher = "소미미디어".into();
        assert!(!fairy.is_catalog_candidate());

        let mut tagged = item("[어린이] 마법 학교 1", 10);
        tagged.category_name = Some("국내도서".into());
        tagged.publisher = "시프트노벨".into();
        assert!(!tagged.is_catalog_candidate());
    }

    #[test]
    fn catalog_candidate_keeps_ln_from_broad_publisher() {
        let mut novel = item("어떤 라노베 2", 11);
        novel.category_name = Some("국내도서-만화/라이트노벨-라이트노벨".into());
        novel.publisher = "서울문화사".into();
        assert!(novel.is_catalog_candidate());
    }

    #[test]
    fn catalog_candidate_rejects_juvenile_genre_shelf() {
        let mut kids = item("초등 판타지 1", 12);
        kids.category_name = Some("국내도서-어린이-장르소설".into());
        kids.publisher = "인사이트".into();
        assert!(!kids.is_catalog_candidate());
    }

    #[test]
    fn drops_unnumbered_ebook_when_paper_volume_exists() {
        let mut paper = item("마녀에게 목줄은 채울 수 없다 1", 1);
        paper.category_name = Some("국내도서-만화/라이트노벨-라이트노벨".into());
        paper.stored_item_id = Some("isbn:paper".into());
        let mut ebook = item("마녀에게 목줄은 채울 수 없다", 2);
        ebook.category_name = Some("ebook-라이트노벨".into());
        ebook.stored_item_id = Some("isbn:ebook".into());
        let groups = group_by_series(vec![paper, ebook]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].items.len(), 1);
        assert_eq!(
            groups[0].items[0].stored_item_id.as_deref(),
            Some("isbn:paper")
        );
    }

    #[test]
    fn keeps_ebook_volume_that_has_no_paper_edition() {
        let mut paper = item("마녀에게 목줄은 채울 수 없다 1", 1);
        paper.category_name = Some("국내도서-만화/라이트노벨-라이트노벨".into());
        let mut ebook = item("마녀에게 목줄은 채울 수 없다 2", 2);
        ebook.category_name = Some("ebook-라이트노벨".into());
        let groups = group_by_series(vec![paper, ebook]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].items.len(), 2);
    }

    #[test]
    fn keeps_ebook_when_it_is_the_only_edition() {
        let mut ebook = item("어떤 라노베 1", 1);
        ebook.category_name = Some("전자책>소설".into());
        ebook.publisher = "시프트노벨".into();
        let groups = group_by_series(vec![ebook]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].items.len(), 1);
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
            normalize_series_title(
                "VTuber인데 방송 끄는 걸 깜빡했더니 전설이 되어있었다 10권 (완결)"
            ),
            "VTuber인데 방송 끄는 걸 깜빡했더니 전설이 되어있었다"
        );
        assert_eq!(
            normalize_series_title("전생했더니 슬라임이었던 건에 대하여 23권 (완결)"),
            "전생했더니 슬라임이었던 건에 대하여"
        );
        assert_eq!(
            normalize_series_title("쌍둥이 둘 다 ‘여자 친구’ 삼아줄래? 4"),
            "쌍둥이 둘 다 '여자 친구' 삼아줄래?"
        );
        assert_eq!(
            normalize_series_title("쌍둥이 둘 다 '여자 친구' 삼아줄래?"),
            "쌍둥이 둘 다 '여자 친구' 삼아줄래?"
        );
        assert_eq!(
            normalize_series_title("[세트] 전생했더니 슬라임이었던 건에 대하여 (총25권/완결)"),
            "전생했더니 슬라임이었던 건에 대하여"
        );
        assert_eq!(
            normalize_series_title("[묶음] Re : 제로부터 시작하는 이세계 생활 (총43권/미완결)"),
            "Re : 제로부터 시작하는 이세계 생활"
        );
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
        assert_eq!(
            normalize_series_title("새벽의 부기팝 - 부기팝 시리즈 6, NT Novel"),
            "새벽의 부기팝"
        );
    }

    #[test]
    fn groups_boogiepop_dawn_as_own_series() {
        let items = vec![item("새벽의 부기팝 - 부기팝 시리즈 6, NT Novel", 432346)];
        let groups = group_by_series(items);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "새벽의 부기팝");
        assert_eq!(groups[0].items.len(), 1);
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
        assert_eq!(
            extract_volume_parts("언리쉬드 앤솔로지 : AREA 1 - Novel Engine"),
            Some((1, 0))
        );
        assert_eq!(
            extract_volume_parts("언리쉬드 앤솔로지 : AREA 1-2 - Novel Engine"),
            Some((1, 2))
        );
        assert_eq!(
            extract_volume_parts("언리쉬드 앤솔로지 : AREA 2-2 - Novel Engine"),
            Some((2, 2))
        );
    }

    #[test]
    fn keeps_hyphen_subvolumes_distinct() {
        let items = vec![
            item("언리쉬드 앤솔로지 : AREA 1 - Novel Engine", 1),
            item("언리쉬드 앤솔로지 : AREA 1-2 - Novel Engine", 2),
            item("언리쉬드 앤솔로지 : AREA 2 - Novel Engine", 3),
            item("언리쉬드 앤솔로지 : AREA 2-2 - Novel Engine", 4),
            item("언리쉬드 앤솔로지 : AREA 3 - Novel Engine", 5),
        ];
        let groups = group_by_series(items);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].title, "언리쉬드 앤솔로지 : AREA");
        assert_eq!(groups[0].items.len(), 5);
        let nums = assign_volume_numbers(&groups[0].items);
        let titles: Vec<_> = nums.iter().map(|(i, n)| (i.title.as_str(), *n)).collect();
        assert_eq!(
            titles,
            vec![
                ("언리쉬드 앤솔로지 : AREA 1 - Novel Engine", 1),
                ("언리쉬드 앤솔로지 : AREA 1-2 - Novel Engine", 2),
                ("언리쉬드 앤솔로지 : AREA 2 - Novel Engine", 3),
                ("언리쉬드 앤솔로지 : AREA 2-2 - Novel Engine", 4),
                ("언리쉬드 앤솔로지 : AREA 3 - Novel Engine", 5),
            ]
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

    fn item_with_date(title: &str, id: i64, pub_date: &str) -> CatalogVolume {
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

    #[test]
    fn unions_manually_merged_aladin_groups() {
        let groups = group_by_series(vec![
            item("부기팝은 웃지 않는다 - 미디어웍스 문고", 1),
            item("새벽의 부기팝 - 부기팝 시리즈 6, NT Novel", 2),
            item("관련 없는 다른 작품 1 - NT Novel", 99),
        ]);
        let mut keys = HashSet::new();
        keys.insert("title:부기팝은 웃지 않는다".into());
        keys.insert("title:새벽의 부기팝".into());
        let known = HashSet::from([1_i64]);
        let merged = union_groups_for_series(&groups, &keys, &known).expect("union");
        let ids: HashSet<i64> = merged.items.iter().map(|i| i.item_id).collect();
        assert!(ids.contains(&1));
        assert!(ids.contains(&2));
        assert!(!ids.contains(&99));
        assert_eq!(
            search_query_for_aladin_key("title:새벽의 부기팝"),
            Some("새벽의 부기팝".into())
        );
    }
}
