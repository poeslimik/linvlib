//! Soft-coded title / series normalization rules loaded from `config/title_rules.toml`.
//!
//! General rules apply to all works. Per-series exceptions live in `[[series_overrides]]`
//! so follow-up volumes do not require code changes (no hardcoded volume numbers).

use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;
use serde::Deserialize;

const EMBEDDED_TOML: &str = include_str!("../../config/title_rules.toml");

#[derive(Debug, Deserialize)]
struct RawRules {
    bundle: RawBundle,
    limited_edition: RawLimited,
    imprint: RawImprint,
    clean: RawClean,
    normalize: RawNormalize,
    edition_scoring: RawEditionScoring,
    #[serde(default)]
    arcs: Vec<RawArc>,
    search: RawSearch,
    #[serde(default)]
    series_overrides: Vec<RawOverride>,
}

#[derive(Debug, Deserialize)]
struct RawBundle {
    #[serde(default)]
    contains: Vec<String>,
    #[serde(default)]
    contains_ci: Vec<String>,
    #[serde(default)]
    patterns: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawLimited {
    #[serde(default)]
    contains: Vec<String>,
    #[serde(default)]
    patterns: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawImprint {
    edition_paren: String,
    special_edition_tail: String,
    #[serde(default)]
    label_patterns: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawClean {
    edition_paren: String,
    xmas_ko: String,
    xmas_en: String,
}

#[derive(Debug, Deserialize)]
struct RawNormalize {
    school_year_strip: String,
    story_part_strip: String,
    chapter_subtitle_strip: String,
    #[serde(default)]
    volume_strip_patterns: Vec<String>,
    #[serde(default)]
    fanbook_markers: Vec<String>,
    #[serde(default)]
    guidebook_volume_skip: Vec<String>,
    art_works_detect: String,
    art_works_strip: String,
    art_works_suffix: String,
    story_part_for_volume: String,
    school_year_extract: String,
    story_part_extract: String,
    sangha_extract: String,
    chapter_label: String,
    #[serde(default)]
    base_volume_extract_patterns: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawEditionScoring {
    tilde_bonus: i32,
    mid_length_bonus: i32,
    mid_length_min: usize,
    mid_length_max: usize,
    long_title_penalty: i32,
    long_title_chars: usize,
    revision_bonus: i32,
    #[serde(default)]
    revision_markers: Vec<String>,
    standard_edition_bonus: i32,
    standard_edition_marker: String,
    limited_penalty: i32,
    #[serde(default)]
    adjustments: Vec<RawScoreAdj>,
}

#[derive(Debug, Deserialize, Clone)]
struct RawScoreAdj {
    contains: String,
    score: i32,
}

#[derive(Debug, Deserialize)]
struct RawArc {
    id: String,
    code: u8,
    label: String,
    #[serde(default)]
    global: bool,
    #[serde(default)]
    search_suffixes: Vec<String>,
    #[serde(default)]
    detect_patterns: Vec<String>,
    volume_extract_pattern: Option<String>,
    #[serde(default)]
    strip: Vec<RawReplace>,
    #[serde(default)]
    display_replacements: Vec<RawReplace>,
    #[serde(default)]
    search_rewrites: Vec<RawSearchRewrite>,
}

#[derive(Debug, Deserialize)]
struct RawReplace {
    pattern: String,
    replace: String,
}

#[derive(Debug, Deserialize)]
struct RawSearchRewrite {
    when_contains: Option<String>,
    unless_contains: Option<String>,
    when_pattern: Option<String>,
    replace_from: Option<String>,
    replace_pattern: Option<String>,
    replace_to: String,
}

#[derive(Debug, Deserialize)]
struct RawSearch {
    #[serde(default)]
    strip_trailing_series_word: bool,
}

#[derive(Debug, Deserialize)]
struct RawOverride {
    id: String,
    match_title_regex: Option<String>,
    #[serde(default)]
    match_contains: Vec<String>,
    canonical_title: Option<String>,
    #[serde(default)]
    extra_search_queries: Vec<String>,
    #[serde(default)]
    enable_arcs: Vec<String>,
    #[serde(default)]
    edition_adjustments: Vec<RawScoreAdj>,
}

#[derive(Debug)]
struct CompiledReplace {
    re: Regex,
    replace: String,
}

#[derive(Debug)]
struct CompiledSearchRewrite {
    when_contains: Option<String>,
    unless_contains: Option<String>,
    when_pattern: Option<Regex>,
    replace_from: Option<String>,
    replace_pattern: Option<Regex>,
    replace_to: String,
}

#[derive(Debug)]
pub struct CompiledArc {
    pub id: String,
    pub code: u8,
    pub label: String,
    pub global: bool,
    pub search_suffixes: Vec<String>,
    detect: Vec<Regex>,
    volume_extract: Option<Regex>,
    strip: Vec<CompiledReplace>,
    display_replacements: Vec<CompiledReplace>,
    search_rewrites: Vec<CompiledSearchRewrite>,
}

#[derive(Debug)]
struct CompiledOverride {
    #[allow(dead_code)]
    id: String,
    match_title_regex: Option<Regex>,
    match_contains: Vec<String>,
    canonical_title: Option<String>,
    extra_search_queries: Vec<String>,
    enable_arcs: Vec<String>,
    edition_adjustments: Vec<RawScoreAdj>,
}

#[derive(Debug)]
pub struct TitleRules {
    bundle_contains: Vec<String>,
    bundle_contains_ci: Vec<String>,
    bundle_patterns: Vec<Regex>,
    limited_contains: Vec<String>,
    limited_patterns: Vec<Regex>,
    imprint_edition_paren: Regex,
    imprint_special_tail: Regex,
    imprint_labels: Vec<Regex>,
    clean_edition_paren: Regex,
    clean_xmas_ko: Regex,
    clean_xmas_en: Regex,
    school_year_strip: Regex,
    story_part_strip: Regex,
    chapter_subtitle_strip: Regex,
    volume_strip_patterns: Vec<Regex>,
    fanbook_markers: Vec<String>,
    guidebook_volume_skip: Vec<String>,
    art_works_detect: Regex,
    art_works_strip: Regex,
    art_works_suffix: String,
    story_part_for_volume: Regex,
    school_year_extract: Regex,
    story_part_extract: Regex,
    sangha_extract: Regex,
    chapter_label: Regex,
    base_volume_extract: Vec<Regex>,
    edition: RawEditionScoring,
    arcs: Vec<CompiledArc>,
    series_overrides: Vec<CompiledOverride>,
    strip_trailing_series_word: bool,
}

fn re(pattern: &str, ctx: &str) -> Result<Regex, String> {
    Regex::new(pattern).map_err(|e| format!("invalid regex in {ctx}: {pattern}: {e}"))
}

impl TitleRules {
    pub fn embedded() -> Self {
        Self::parse(EMBEDDED_TOML).expect("embedded title_rules.toml must be valid")
    }

    pub fn load_or_embedded(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        match std::fs::read_to_string(path) {
            Ok(text) => match Self::parse(&text) {
                Ok(rules) => {
                    tracing::info!("loaded title rules from {}", path.display());
                    rules
                }
                Err(err) => {
                    tracing::warn!(
                        "failed to parse title rules at {}: {err}; using embedded",
                        path.display()
                    );
                    Self::embedded()
                }
            },
            Err(_) => Self::embedded(),
        }
    }

    pub fn parse(toml_text: &str) -> Result<Self, String> {
        let raw: RawRules =
            toml::from_str(toml_text).map_err(|e| format!("title_rules.toml: {e}"))?;

        let limited_patterns = raw
            .limited_edition
            .patterns
            .iter()
            .map(|p| re(p, "limited_edition.patterns"))
            .collect::<Result<Vec<_>, _>>()?;

        let imprint_labels = raw
            .imprint
            .label_patterns
            .iter()
            .map(|p| re(p, "imprint.label_patterns"))
            .collect::<Result<Vec<_>, _>>()?;

        let volume_strip_patterns = raw
            .normalize
            .volume_strip_patterns
            .iter()
            .map(|p| re(p, "normalize.volume_strip_patterns"))
            .collect::<Result<Vec<_>, _>>()?;

        let base_volume_extract = raw
            .normalize
            .base_volume_extract_patterns
            .iter()
            .map(|p| re(p, "normalize.base_volume_extract_patterns"))
            .collect::<Result<Vec<_>, _>>()?;

        let mut arcs = Vec::with_capacity(raw.arcs.len());
        for arc in raw.arcs {
            let detect = arc
                .detect_patterns
                .iter()
                .map(|p| re(p, &format!("arcs.{}.detect", arc.id)))
                .collect::<Result<Vec<_>, _>>()?;
            let strip = arc
                .strip
                .iter()
                .map(|s| {
                    Ok(CompiledReplace {
                        re: re(&s.pattern, &format!("arcs.{}.strip", arc.id))?,
                        replace: s.replace.clone(),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let display_replacements = arc
                .display_replacements
                .iter()
                .map(|s| {
                    Ok(CompiledReplace {
                        re: re(&s.pattern, &format!("arcs.{}.display", arc.id))?,
                        replace: s.replace.clone(),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let search_rewrites = arc
                .search_rewrites
                .into_iter()
                .map(|s| {
                    Ok(CompiledSearchRewrite {
                        when_contains: s.when_contains,
                        unless_contains: s.unless_contains,
                        when_pattern: s
                            .when_pattern
                            .as_deref()
                            .map(|p| re(p, &format!("arcs.{}.search_rewrites", arc.id)))
                            .transpose()?,
                        replace_from: s.replace_from,
                        replace_pattern: s
                            .replace_pattern
                            .as_deref()
                            .map(|p| re(p, &format!("arcs.{}.search_rewrites", arc.id)))
                            .transpose()?,
                        replace_to: s.replace_to,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            let volume_extract = arc
                .volume_extract_pattern
                .as_deref()
                .map(|p| re(p, &format!("arcs.{}.volume_extract", arc.id)))
                .transpose()?;
            arcs.push(CompiledArc {
                id: arc.id,
                code: arc.code,
                label: arc.label,
                global: arc.global,
                search_suffixes: arc.search_suffixes,
                detect,
                volume_extract,
                strip,
                display_replacements,
                search_rewrites,
            });
        }

        let mut series_overrides = Vec::with_capacity(raw.series_overrides.len());
        for ov in raw.series_overrides {
            series_overrides.push(CompiledOverride {
                id: ov.id.clone(),
                match_title_regex: ov
                    .match_title_regex
                    .as_deref()
                    .map(|p| re(p, &format!("series_overrides.{}.match_title_regex", ov.id)))
                    .transpose()?,
                match_contains: ov.match_contains,
                canonical_title: ov.canonical_title,
                extra_search_queries: ov.extra_search_queries,
                enable_arcs: ov.enable_arcs,
                edition_adjustments: ov.edition_adjustments,
            });
        }

        Ok(Self {
            bundle_contains: raw.bundle.contains,
            bundle_contains_ci: raw.bundle.contains_ci,
            bundle_patterns: raw
                .bundle
                .patterns
                .iter()
                .map(|p| re(p, "bundle.patterns"))
                .collect::<Result<Vec<_>, _>>()?,
            limited_contains: raw.limited_edition.contains,
            limited_patterns,
            imprint_edition_paren: re(&raw.imprint.edition_paren, "imprint.edition_paren")?,
            imprint_special_tail: re(
                &raw.imprint.special_edition_tail,
                "imprint.special_edition_tail",
            )?,
            imprint_labels,
            clean_edition_paren: re(&raw.clean.edition_paren, "clean.edition_paren")?,
            clean_xmas_ko: re(&raw.clean.xmas_ko, "clean.xmas_ko")?,
            clean_xmas_en: re(&raw.clean.xmas_en, "clean.xmas_en")?,
            school_year_strip: re(&raw.normalize.school_year_strip, "normalize.school_year_strip")?,
            story_part_strip: re(&raw.normalize.story_part_strip, "normalize.story_part_strip")?,
            chapter_subtitle_strip: re(
                &raw.normalize.chapter_subtitle_strip,
                "normalize.chapter_subtitle_strip",
            )?,
            volume_strip_patterns,
            fanbook_markers: raw.normalize.fanbook_markers,
            guidebook_volume_skip: raw.normalize.guidebook_volume_skip,
            art_works_detect: re(&raw.normalize.art_works_detect, "normalize.art_works_detect")?,
            art_works_strip: re(&raw.normalize.art_works_strip, "normalize.art_works_strip")?,
            art_works_suffix: raw.normalize.art_works_suffix,
            story_part_for_volume: re(
                &raw.normalize.story_part_for_volume,
                "normalize.story_part_for_volume",
            )?,
            school_year_extract: re(
                &raw.normalize.school_year_extract,
                "normalize.school_year_extract",
            )?,
            story_part_extract: re(
                &raw.normalize.story_part_extract,
                "normalize.story_part_extract",
            )?,
            sangha_extract: re(&raw.normalize.sangha_extract, "normalize.sangha_extract")?,
            chapter_label: re(&raw.normalize.chapter_label, "normalize.chapter_label")?,
            base_volume_extract,
            edition: raw.edition_scoring,
            arcs,
            series_overrides,
            strip_trailing_series_word: raw.search.strip_trailing_series_word,
        })
    }

    fn find_override(&self, title: &str) -> Option<&CompiledOverride> {
        self.series_overrides.iter().find(|ov| {
            if let Some(re) = &ov.match_title_regex {
                if re.is_match(title) {
                    return true;
                }
            }
            !ov.match_contains.is_empty() && ov.match_contains.iter().any(|s| title.contains(s))
        })
    }

    /// Arcs that should be merged into the main series title / searched as auxiliaries.
    pub fn merge_arcs_for(&self, title: &str) -> Vec<&CompiledArc> {
        let enabled: Vec<&str> = self
            .find_override(title)
            .map(|ov| ov.enable_arcs.iter().map(String::as_str).collect())
            .unwrap_or_default();
        self.arcs
            .iter()
            .filter(|a| a.global || enabled.iter().any(|id| *id == a.id))
            .collect()
    }

    pub fn arc_by_code(&self, code: u8) -> Option<&CompiledArc> {
        self.arcs.iter().find(|a| a.code == code)
    }

    pub fn is_bundle_or_set(&self, title: &str) -> bool {
        let lower = title.to_lowercase();
        self.bundle_contains.iter().any(|s| title.contains(s))
            || self.bundle_contains_ci.iter().any(|s| lower.contains(s))
            || self.bundle_patterns.iter().any(|re| re.is_match(title))
    }

    pub fn is_limited_edition(&self, title: &str) -> bool {
        if self.limited_contains.iter().any(|s| title.contains(s)) {
            return true;
        }
        self.limited_patterns.iter().any(|re| re.is_match(title))
    }

    /// 개정판/리커버 등: edition_preference에서 원판보다 낮은 점수.
    pub fn is_revision_edition(&self, title: &str) -> bool {
        self.edition
            .revision_markers
            .iter()
            .any(|m| title.contains(m))
    }

    pub fn strip_imprint_suffix(&self, title: &str) -> String {
        let mut result = title.trim().to_string();
        result = result
            .replace("&quot;", "\"")
            .replace("&#34;", "\"")
            .replace("&apos;", "'")
            .replace("&#39;", "'")
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace('！', "!")
            .replace('－', "-")
            .replace('–', "-")
            .replace('—', "-")
            .replace('，', ",")
            .replace('Ｓ', "S")
            .replace('ｓ', "s")
            .replace('`', "")
            .replace('"', "")
            .replace('\u{201c}', "")
            .replace('\u{201d}', "")
            .replace('\u{2018}', "")
            .replace('\u{2019}', "");
        result = Regex::new(r"\s+")
            .unwrap()
            .replace_all(&result, " ")
            .to_string();

        result = self
            .imprint_edition_paren
            .replace_all(&result, "")
            .to_string();
        result = self.imprint_special_tail.replace(&result, "").to_string();

        for re in &self.imprint_labels {
            result = re.replace(&result, "").trim().to_string();
        }
        result
    }

    pub fn clean_volume_title(&self, title: &str) -> String {
        let mut result = self.strip_imprint_suffix(title);
        result = self
            .clean_edition_paren
            .replace_all(&result, " ")
            .to_string();
        result = self.clean_xmas_ko.replace_all(&result, "").to_string();
        result = self.clean_xmas_en.replace_all(&result, "").to_string();

        // Display replacements from any arc whose detect matches (scoped patterns).
        for arc in &self.arcs {
            if arc.detect.iter().any(|re| re.is_match(&result)) {
                for rep in &arc.display_replacements {
                    result = rep.re.replace_all(&result, rep.replace.as_str()).to_string();
                }
            }
        }

        result = Regex::new(r"\s+")
            .unwrap()
            .replace_all(&result, " ")
            .trim()
            .to_string();
        result
            .trim_end_matches(['-', ',', ' ', '"', '\'', '”', '“', '`'])
            .trim()
            .to_string()
    }

    pub fn normalize_series_title(&self, title: &str) -> String {
        let mut result = normalize_fullwidth_digits(&self.strip_imprint_suffix(title));

        if let Some(ov) = self.find_override(&result) {
            if let Some(canonical) = &ov.canonical_title {
                // match_title_regex 또는 match_contains로 매칭되면 정규화명을 고정
                return canonical.clone();
            }
        }

        if self.art_works_detect.is_match(&result) {
            let mut base = self.art_works_strip.replace(&result, "").to_string();
            base = self.school_year_strip.replace_all(&base, " ").to_string();
            base = Regex::new(r"\s*제\d+부\s*[:：]?\s*")
                .unwrap()
                .replace_all(&base, " ")
                .to_string();
            base = Regex::new(r"\s*[-–—:]\s*.*$")
                .unwrap()
                .replace(&base, "")
                .to_string();
            base = Regex::new(r"\s+")
                .unwrap()
                .replace_all(&base, " ")
                .trim()
                .trim_end_matches(['-', ':', ' '])
                .to_string();
            if !base.is_empty() {
                return format!("{base} {}", self.art_works_suffix);
            }
            return self.art_works_suffix.clone();
        }

        let is_fanbook = self
            .fanbook_markers
            .iter()
            .any(|m| result.contains(m));

        result = self
            .school_year_strip
            .replace_all(&result, " ")
            .to_string();

        if !is_fanbook {
            result = self.story_part_strip.replace(&result, "").to_string();
        }

        for arc in self.merge_arcs_for(&result) {
            for rep in &arc.strip {
                result = rep
                    .re
                    .replace_all(&result, rep.replace.as_str())
                    .to_string();
            }
        }
        result = Regex::new(r"\s+")
            .unwrap()
            .replace_all(&result, " ")
            .trim()
            .to_string();

        result = self
            .chapter_subtitle_strip
            .replace(&result, "")
            .trim()
            .to_string();

        result = protect_numeric_ratios(&result);

        for re in &self.volume_strip_patterns {
            let next = re.replace(&result, "").trim().to_string();
            if next != result && !next.is_empty() {
                result = next;
                break;
            }
        }

        restore_numeric_ratios(
            result
                .trim_end_matches(['-', '–', '—', ':', '~', ',', '，', ' ', '.'])
                .trim(),
        )
    }

    pub fn extract_series_arc(&self, title: &str) -> u8 {
        let t = normalize_fullwidth_digits(title);
        // Prefer higher-specificity / declared order; check non-zero codes in list order.
        for arc in &self.arcs {
            if arc.detect.iter().any(|re| re.is_match(&t)) {
                // Non-global arcs only count when series override enables them,
                // unless the title already clearly matches the series (override match).
                if arc.global || self.merge_arcs_for(&t).iter().any(|a| a.id == arc.id) {
                    return arc.code;
                }
            }
        }
        0
    }

    pub fn edition_preference(&self, title: &str) -> i32 {
        let mut score = 0;
        let len = title.chars().count();
        let e = &self.edition;
        if title.contains('~') {
            score += e.tilde_bonus;
        } else if (e.mid_length_min..=e.mid_length_max).contains(&len) {
            score += e.mid_length_bonus;
        } else if len > e.long_title_chars {
            score += e.long_title_penalty;
        }
        if e.revision_markers.iter().any(|m| title.contains(m)) {
            score += e.revision_bonus;
        }
        if title.contains(&e.standard_edition_marker) {
            score += e.standard_edition_bonus;
        }
        for adj in &e.adjustments {
            if title.contains(&adj.contains) {
                score += adj.score;
            }
        }
        if let Some(ov) = self.find_override(title) {
            for adj in &ov.edition_adjustments {
                if title.contains(&adj.contains) {
                    score += adj.score;
                }
            }
        }
        if self.is_limited_edition(title) {
            score += e.limited_penalty;
        }
        score
    }

    pub fn alternate_search_queries(
        &self,
        search_query: &str,
        title_hint: Option<&str>,
    ) -> Vec<String> {
        let mut out = Vec::new();
        let norm = self.normalize_series_title(search_query);
        if norm != search_query {
            out.push(norm.clone());
        }

        for arc in self.merge_arcs_for(search_query) {
            for rw in &arc.search_rewrites {
                let when_ok = if let Some(pat) = &rw.when_pattern {
                    pat.is_match(search_query)
                } else if let Some(s) = &rw.when_contains {
                    search_query.contains(s)
                } else {
                    false
                };
                if !when_ok {
                    continue;
                }
                if rw
                    .unless_contains
                    .as_ref()
                    .is_some_and(|u| search_query.contains(u))
                {
                    continue;
                }
                let rewritten = if let Some(from) = &rw.replace_from {
                    search_query.replace(from, &rw.replace_to)
                } else if let Some(pat) = &rw.replace_pattern {
                    pat.replace(search_query, rw.replace_to.as_str()).to_string()
                } else {
                    continue;
                };
                let rewritten = rewritten.trim().to_string();
                if !rewritten.is_empty() && rewritten != search_query {
                    out.push(rewritten);
                }
            }
        }

        if self.art_works_detect.is_match(search_query)
            || self.art_works_detect.is_match(&norm)
            || search_query.contains("아트북")
        {
            let base = self
                .art_works_strip
                .replace(&norm, "")
                .trim()
                .to_string();
            if !base.is_empty() {
                out.push(format!("{base} {}", self.art_works_suffix));
                out.push(format!("{base} 아트북"));
                if let Some(tail) = base.split_whitespace().last() {
                    if tail.chars().count() >= 2 {
                        out.push(format!("{tail} {}", self.art_works_suffix));
                    }
                }
            }
        }

        if let Some(hint) = title_hint.map(str::trim).filter(|s| !s.is_empty()) {
            let hint_norm = self.normalize_series_title(hint);
            if !hint_norm.is_empty() && !out.contains(&hint_norm) && hint_norm != search_query {
                out.push(hint_norm);
            }
        }

        if self.strip_trailing_series_word && norm.ends_with("시리즈") {
            let base = norm.trim_end_matches("시리즈").trim().to_string();
            if !base.is_empty() {
                out.push(base);
            }
        }

        if let Some(ov) = self.find_override(search_query).or_else(|| self.find_override(&norm))
        {
            out.extend(ov.extra_search_queries.clone());
        }

        out.sort();
        out.dedup();
        out.retain(|q| q != search_query && !q.is_empty());
        out
    }

    pub fn import_arc_search_queries(&self, search_query: &str) -> Vec<String> {
        let mut out = Vec::new();
        for arc in self.merge_arcs_for(search_query) {
            for suffix in &arc.search_suffixes {
                let q = format!("{search_query} {suffix}");
                if q != search_query {
                    out.push(q);
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    pub fn is_art_works_title(&self, title: &str) -> bool {
        self.art_works_detect.is_match(title)
    }

    pub fn should_skip_volume_number(&self, title: &str) -> bool {
        self.is_bundle_or_set(title)
            || self.is_art_works_title(title)
            || self
                .guidebook_volume_skip
                .iter()
                .any(|m| title.contains(m))
    }

    pub fn extract_volume_parts(&self, title: &str) -> Option<(i64, u8)> {
        if self.should_skip_volume_number(title) {
            return None;
        }
        let stripped = self.school_year_strip.replace_all(title, " ").to_string();
        let stripped = self
            .story_part_for_volume
            .replace_all(&stripped, " ")
            .to_string();
        let stripped = Regex::new(r"\s+")
            .unwrap()
            .replace_all(&stripped, " ")
            .trim()
            .to_string();
        let stripped = protect_numeric_ratios(&normalize_fullwidth_digits(&stripped));

        let mut patterns: Vec<&Regex> = self.base_volume_extract.iter().collect();
        // Arc volume patterns (SSS/Ex) — insert after vol./권 patterns conceptually:
        // keep order: base first two (권, vol), then arc patterns, then trailing.
        // Simpler: try arc patterns after first two base patterns.
        let arc_patterns: Vec<&Regex> = self
            .arcs
            .iter()
            .filter_map(|a| a.volume_extract.as_ref())
            .collect();

        let ordered: Vec<&Regex> = if self.base_volume_extract.len() >= 2 {
            let mut v = Vec::new();
            v.extend(self.base_volume_extract.iter().take(2));
            v.extend(arc_patterns);
            v.extend(self.base_volume_extract.iter().skip(2));
            v
        } else {
            patterns.extend(arc_patterns);
            patterns
        };

        for re in ordered {
            if let Some(caps) = re.captures(&stripped) {
                let major: i64 = caps.get(1)?.as_str().parse().ok()?;
                let minor: u8 = caps
                    .get(2)
                    .and_then(|m| m.as_str().parse().ok())
                    .unwrap_or(0);
                return Some((major, minor));
            }
        }
        if let Ok(major) = stripped.parse::<i64>() {
            return Some((major, 0));
        }
        None
    }

    pub fn extract_school_year(&self, title: &str) -> u8 {
        self.school_year_extract
            .captures(title)
            .and_then(|c| c.get(1))
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(1)
    }

    pub fn extract_story_part(&self, title: &str) -> u8 {
        self.story_part_extract
            .captures(title)
            .and_then(|c| c.get(1))
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0)
    }

    pub fn extract_volume_part(&self, title: &str) -> u8 {
        let stripped = self.school_year_strip.replace_all(title, " ").to_string();
        match self
            .sangha_extract
            .captures(&stripped)
            .and_then(|caps| caps.get(1))
            .map(|m| m.as_str())
        {
            Some("상") => 1,
            Some("중") => 2,
            Some("하") => 3,
            _ => 0,
        }
    }

    pub fn format_volume_label_from_title(
        &self,
        title: &str,
        fallback_number: i64,
        arc_volume_count: usize,
    ) -> String {
        let cleaned = self.clean_volume_title(title);
        let arc_code = self.extract_series_arc(&cleaned);
        if let Some(arc) = self.arc_by_code(arc_code) {
            if arc_code > 0 {
                if arc_volume_count <= 1 {
                    return arc.label.clone();
                }
                if let Some((num, _)) = self.extract_volume_parts(&cleaned) {
                    return format!("{} {num}권", arc.label);
                }
                return arc.label.clone();
            }
        }

        if let Some((major, minor)) = self.extract_volume_parts(&cleaned) {
            let num = if minor == 0 {
                format!("{major}")
            } else if hyphen_subvolume_in_title(&cleaned, major, minor) {
                format!("{major}-{minor}")
            } else {
                format!("{major}.{minor}")
            };
            let body = match self.extract_volume_part(&cleaned) {
                1 => format!("{num}상"),
                2 => format!("{num}중"),
                3 => format!("{num}하"),
                _ => format!("{num}권"),
            };
            let year = self.extract_school_year(&cleaned);
            if year >= 2 {
                return format!("{year}학년 {body}");
            }
            let story = self.extract_story_part(&cleaned);
            if story >= 1 {
                return format!("{story}부 {body}");
            }
            return body;
        }
        if let Some(caps) = self.chapter_label.captures(&cleaned) {
            return caps.get(1).unwrap().as_str().trim().to_string();
        }
        // 작품별 정규화명 뒤 부제·상하 (예: 춘하추동 대행자 봄의 춤 : 상 → 봄의 춤 상)
        if let Some(ov) = self.find_override(&cleaned) {
            if let Some(canonical) = &ov.canonical_title {
                if let Some(rest) = cleaned.strip_prefix(canonical.as_str()) {
                    let rest = rest
                        .trim()
                        .trim_start_matches(['-', '–', '—', ':', '：', ' '])
                        .trim();
                    if !rest.is_empty() {
                        return Regex::new(r"\s*[:：]\s*")
                            .unwrap()
                            .replace_all(rest, " ")
                            .to_string();
                    }
                }
            }
        }
        format!("{fallback_number}권")
    }
}

pub fn default_rules() -> &'static TitleRules {
    static RULES: OnceLock<TitleRules> = OnceLock::new();
    RULES.get_or_init(TitleRules::embedded)
}

fn hyphen_subvolume_in_title(title: &str, major: i64, minor: u8) -> bool {
    let compact = format!("{major}-{minor}");
    let spaced = format!("{major} - {minor}");
    title.contains(&compact)
        || title.contains(&spaced)
        || title.contains(&format!("{major}–{minor}"))
        || title.contains(&format!("{major}—{minor}"))
}

fn protect_numeric_ratios(title: &str) -> String {
    Regex::new(r"(\d+)\s*[:：]\s*(\d+)")
        .unwrap()
        .replace_all(title, "$1\u{2215}$2")
        .to_string()
}

fn restore_numeric_ratios(title: &str) -> String {
    title.replace('\u{2215}', ":")
}

fn normalize_fullwidth_digits(s: &str) -> String {
    s.chars()
        .map(|c| {
            if ('０'..='９').contains(&c) {
                char::from_u32(u32::from(c) - u32::from('０') + u32::from('0')).unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_embedded_rules() {
        let rules = TitleRules::embedded();
        assert!(rules.arcs.iter().any(|a| a.id == "sss" && a.global));
        assert!(rules.arcs.iter().any(|a| a.id == "sajok" && !a.global));
        assert!(rules.find_override("무직전생 1").is_some());
    }

    #[test]
    fn sajok_merge_only_for_mushoku() {
        let rules = TitleRules::embedded();
        assert!(
            rules
                .merge_arcs_for("무직전생 1 - 사족 편")
                .iter()
                .any(|a| a.id == "sajok")
        );
        assert!(
            !rules
                .merge_arcs_for("다른작품 1 - 사족 편")
                .iter()
                .any(|a| a.id == "sajok")
        );
    }

    #[test]
    fn merges_shunkashuto_season_subtitles() {
        let rules = TitleRules::embedded();
        for title in [
            "춘하추동 대행자 봄의 춤 : 상 - L Books",
            "춘하추동 대행자 여름의 춤 : 하 - L Books",
            "춘하추동 대행자 가을의 춤 : 상 - L Books 초판 한정 특전 띠지, 책갈피",
            "춘하추동 대행자 새벽의 사수 - L Books",
        ] {
            assert_eq!(
                rules.normalize_series_title(title),
                "춘하추동 대행자",
                "title={title}"
            );
        }
        assert_eq!(
            rules.format_volume_label_from_title(
                "춘하추동 대행자 봄의 춤 : 상 - L Books",
                1,
                0
            ),
            "봄의 춤 상"
        );
        assert_eq!(
            rules.format_volume_label_from_title(
                "춘하추동 대행자 새벽의 사수 - L Books",
                7,
                0
            ),
            "새벽의 사수"
        );
    }

    #[test]
    fn strips_named_series_volume_suffix() {
        let rules = TitleRules::embedded();
        assert_eq!(
            rules.normalize_series_title("새벽의 부기팝 - 부기팝 시리즈 6, NT Novel"),
            "새벽의 부기팝"
        );
        assert_eq!(
            rules.extract_volume_parts("새벽의 부기팝 - 부기팝 시리즈 6, NT Novel"),
            Some((6, 0))
        );
    }

    #[test]
    fn hyphen_subvolumes_stay_in_same_series() {
        let rules = TitleRules::embedded();
        for title in [
            "언리쉬드 앤솔로지 : AREA 1 - Novel Engine",
            "언리쉬드 앤솔로지 : AREA 1-2 - Novel Engine",
            "언리쉬드 앤솔로지 : AREA 2-2 - Novel Engine",
            "언리쉬드 앤솔로지 : AREA 8 - Novel Engine",
        ] {
            assert_eq!(
                rules.normalize_series_title(title),
                "언리쉬드 앤솔로지 : AREA",
                "title={title}"
            );
        }
        assert_eq!(
            rules.extract_volume_parts("언리쉬드 앤솔로지 : AREA 1 - Novel Engine"),
            Some((1, 0))
        );
        assert_eq!(
            rules.extract_volume_parts("언리쉬드 앤솔로지 : AREA 1-2 - Novel Engine"),
            Some((1, 2))
        );
        assert_eq!(
            rules.format_volume_label_from_title(
                "언리쉬드 앤솔로지 : AREA 1-2 - Novel Engine",
                2,
                0
            ),
            "1-2권"
        );
        assert_eq!(
            rules.format_volume_label_from_title(
                "어서 오세요 실력지상주의 교실에 7.5 - S Novel",
                8,
                0
            ),
            "7.5권"
        );
    }
}
