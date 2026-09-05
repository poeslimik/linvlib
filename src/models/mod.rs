use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::error::{AppError, AppResult};

/// Valid `series.publish_status` values.
pub const PUBLISH_STATUSES: &[&str] = &[
    "complete",
    "complete_partial",
    "complete_stalled",
    "ongoing",
    "ongoing_stalled",
    "hiatus",
    "hiatus_done",
];

pub fn normalize_publish_status(raw: &str) -> Option<&'static str> {
    match raw.trim() {
        "complete" => Some("complete"),
        "complete_partial" => Some("complete_partial"),
        "complete_stalled" => Some("complete_stalled"),
        "ongoing" => Some("ongoing"),
        "ongoing_stalled" => Some("ongoing_stalled"),
        "hiatus" => Some("hiatus"),
        "hiatus_done" => Some("hiatus_done"),
        _ => None,
    }
}

pub fn publish_status_label(status: &str) -> &'static str {
    match status {
        "complete" => "완결",
        "complete_partial" => "완결(번역 미완)",
        "complete_stalled" => "완결(번역 중단)",
        "ongoing" => "연재중",
        "ongoing_stalled" => "연재중(번역 중단)",
        "hiatus" => "연재 중단(번역 미완)",
        "hiatus_done" => "연재 중단",
        _ => "연재중",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum Rating {
    S,
    A,
    B,
    C,
    D,
    F,
    None,
}

impl Rating {
    pub fn as_str(&self) -> &'static str {
        match self {
            Rating::S => "S",
            Rating::A => "A",
            Rating::B => "B",
            Rating::C => "C",
            Rating::D => "D",
            Rating::F => "F",
            Rating::None => "None",
        }
    }

    pub fn tier_value(&self) -> Option<&'static str> {
        match self {
            Rating::None => None,
            other => Some(other.as_str()),
        }
    }

    pub fn from_tier_str(s: &str) -> Option<Self> {
        match s {
            "S" => Some(Rating::S),
            "A" => Some(Rating::A),
            "B" => Some(Rating::B),
            "C" => Some(Rating::C),
            "D" => Some(Rating::D),
            "F" => Some(Rating::F),
            _ => None,
        }
    }
}

impl std::str::FromStr for Rating {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "S" => Ok(Rating::S),
            "A" => Ok(Rating::A),
            "B" => Ok(Rating::B),
            "C" => Ok(Rating::C),
            "D" => Ok(Rating::D),
            "F" => Ok(Rating::F),
            "None" => Ok(Rating::None),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct UserRow {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub created_at: String,
    pub is_admin: i64,
    pub email_verified: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub created_at: DateTime<Utc>,
    pub is_admin: bool,
    pub email_verified: bool,
}

impl UserRow {
    pub fn into_user(self) -> AppResult<User> {
        Ok(User {
            id: parse_uuid(&self.id)?,
            email: self.email,
            password_hash: self.password_hash,
            created_at: parse_datetime(&self.created_at)?,
            is_admin: self.is_admin != 0,
            email_verified: self.email_verified != 0,
        })
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct SeriesRow {
    pub id: String,
    pub title: String,
    pub author: Option<String>,
    pub publisher: Option<String>,
    pub aladin_series_id: String,
    pub first_published_at: Option<String>,
    pub latest_published_at: Option<String>,
    pub created_at: String,
    #[sqlx(default)]
    pub cover_url: Option<String>,
    #[sqlx(default)]
    pub publish_status: String,
    #[sqlx(default)]
    pub source_label: Option<String>,
    #[sqlx(default)]
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Series {
    pub id: Uuid,
    pub title: String,
    pub author: Option<String>,
    pub publisher: Option<String>,
    pub aladin_series_id: String,
    pub first_published_at: Option<DateTime<Utc>>,
    pub latest_published_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub publish_status: String,
    pub cover_url: Option<String>,
    /// Custom attribution label (manual series). Empty/None → hide source line.
    pub source_label: Option<String>,
    pub source_url: Option<String>,
}

impl SeriesRow {
    pub fn into_series(self) -> AppResult<Series> {
        let publish_status = normalize_publish_status(&self.publish_status)
            .unwrap_or("ongoing")
            .to_string();
        Ok(Series {
            id: parse_uuid(&self.id)?,
            title: self.title,
            author: self.author,
            publisher: self.publisher,
            aladin_series_id: self.aladin_series_id,
            first_published_at: self
                .first_published_at
                .as_deref()
                .map(parse_datetime)
                .transpose()?,
            latest_published_at: self
                .latest_published_at
                .as_deref()
                .map(parse_datetime)
                .transpose()?,
            created_at: parse_datetime(&self.created_at)?,
            publish_status,
            cover_url: self
                .cover_url
                .filter(|s| !s.trim().is_empty()),
            source_label: self
                .source_label
                .filter(|s| !s.trim().is_empty()),
            source_url: self
                .source_url
                .filter(|s| !s.trim().is_empty()),
        })
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct VolumeRow {
    pub id: String,
    pub series_id: String,
    pub volume_number: i64,
    pub title: String,
    pub cover_url: Option<String>,
    pub published_at: Option<String>,
    pub aladin_item_id: String,
    pub isbn13: Option<String>,
    #[sqlx(default)]
    pub is_unreleased: i64,
    /// Operator override for the short volume label (e.g. 단편집). Empty → auto.
    #[sqlx(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Volume {
    pub id: Uuid,
    pub series_id: Uuid,
    pub volume_number: i64,
    pub title: String,
    pub cover_url: Option<String>,
    pub published_at: Option<NaiveDate>,
    pub aladin_item_id: String,
    pub isbn13: Option<String>,
    /// True when filled from a JP edition without a Korean release.
    pub is_unreleased: bool,
    pub label: Option<String>,
}

impl VolumeRow {
    pub fn into_volume(self) -> AppResult<Volume> {
        Ok(Volume {
            id: parse_uuid(&self.id)?,
            series_id: parse_uuid(&self.series_id)?,
            volume_number: self.volume_number,
            title: self.title,
            cover_url: self.cover_url,
            published_at: self
                .published_at
                .as_deref()
                .map(parse_date)
                .transpose()?,
            aladin_item_id: self.aladin_item_id,
            isbn13: self.isbn13,
            is_unreleased: self.is_unreleased != 0,
            label: self
                .label
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        })
    }
}

pub fn parse_uuid(value: &str) -> AppResult<Uuid> {
    Uuid::parse_str(value).map_err(|e| AppError::Internal(format!("invalid uuid: {e}")))
}

pub fn parse_datetime(value: &str) -> AppResult<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Ok(dt.with_timezone(&Utc));
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
        return Ok(DateTime::from_naive_utc_and_offset(naive, Utc));
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%SZ") {
        return Ok(DateTime::from_naive_utc_and_offset(naive, Utc));
    }
    // volumes.published_at(YYYY-MM-DD)을 series에 복사한 경우
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return Ok(DateTime::from_naive_utc_and_offset(
            date.and_hms_opt(0, 0, 0).unwrap(),
            Utc,
        ));
    }
    Err(AppError::Internal(format!("invalid datetime: {value}")))
}

pub fn parse_date(value: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|e| AppError::Internal(format!("invalid date: {e}")))
}

pub fn format_datetime(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

pub fn format_date(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub accept_terms: bool,
    #[serde(default)]
    pub accept_privacy: bool,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub token_type: &'static str,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    pub token_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct VerifyEmailRequest {
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct ResendVerificationRequest {
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

#[derive(Debug, Serialize)]
pub struct ForgotPasswordResponse {
    pub message: String,
    /// Present only when `EMAIL_DEV_MODE` is on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    pub token: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct DeleteAccountRequest {
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub is_admin: bool,
    pub email_verified: bool,
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            email: user.email,
            is_admin: user.is_admin,
            email_verified: user.email_verified,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct VolumeWithRead {
    pub id: Uuid,
    pub volume_number: i64,
    pub title: String,
    pub cover_url: Option<String>,
    pub published_at: Option<NaiveDate>,
    pub is_read: bool,
    pub is_unreleased: bool,
    /// Operator-set short label; null → client auto-formats from title.
    pub label: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SeriesDetailResponse {
    pub id: Uuid,
    pub title: String,
    pub author: Option<String>,
    pub publisher: Option<String>,
    pub aladin_series_id: String,
    pub is_manual: bool,
    pub publish_status: String,
    pub first_published_at: Option<DateTime<Utc>>,
    pub latest_published_at: Option<DateTime<Utc>>,
    /// Stored series representative cover (may be null → UI falls back to latest volume)
    pub cover_url: Option<String>,
    pub latest_cover_url: Option<String>,
    /// Custom source attribution (mainly for manual series).
    #[serde(default)]
    pub source_label: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    pub rating: Rating,
    pub volumes: Vec<VolumeWithRead>,
    pub total_volumes: i64,
    pub read_volumes: i64,
    pub progress_percent: i32,
    /// Search nicknames / abbreviations (auto + manual).
    #[serde(default)]
    pub search_aliases: Vec<SeriesSearchAlias>,
    /// Other works grouped for combined search (e.g. main + spin-off).
    #[serde(default)]
    pub search_bundle_peers: Vec<SearchBundlePeer>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SeriesSearchAlias {
    pub id: Uuid,
    pub alias: String,
    /// auto | user | admin
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchBundlePeer {
    pub id: Uuid,
    pub title: String,
}

#[derive(Debug, Deserialize)]
pub struct AddSearchAliasRequest {
    pub alias: String,
}

#[derive(Debug, Deserialize)]
pub struct BatchSearchAliasRequest {
    pub alias: String,
    pub series_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct BatchSearchBundleRequest {
    pub series_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct SaveReadsRequest {
    pub reads: Vec<ReadEntry>,
}

#[derive(Debug, Deserialize)]
pub struct ReadEntry {
    pub volume_id: Uuid,
    pub is_read: bool,
}

#[derive(Debug, Deserialize)]
pub struct SaveRatingRequest {
    pub rating: Rating,
}

#[derive(Debug, Serialize)]
pub struct SeriesListItem {
    pub rank: i64,
    pub id: Uuid,
    pub title: String,
    pub latest_cover_url: Option<String>,
    pub total_volumes: i64,
    pub read_volumes: i64,
    pub progress_percent: i32,
    pub publish_status: String,
}

#[derive(Debug, Serialize)]
pub struct SeriesListResponse {
    pub items: Vec<SeriesListItem>,
    pub page: i64,
    pub limit: i64,
    pub total: i64,
}

#[derive(Debug, Serialize)]
pub struct SearchResultItem {
    pub id: Uuid,
    pub title: String,
    pub latest_cover_url: Option<String>,
    pub score: i64,
}

#[derive(Debug, Serialize)]
pub struct ImportSearchResult {
    pub aladin_series_id: String,
    pub title: String,
    pub author: Option<String>,
    pub publisher: Option<String>,
    pub cover_url: Option<String>,
    pub volume_count: usize,
    pub already_imported: bool,
    pub series_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct ImportRequest {
    pub aladin_series_id: Option<String>,
    /// Single Aladin ItemId (or product URL). Prefer `seed_item_ids` for multiple.
    pub seed_item_id: Option<String>,
    /// Explicit ItemIds / product URLs to LookUp (fills gaps ItemSearch misses).
    #[serde(default)]
    pub seed_item_ids: Vec<String>,
    pub title: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ImportResponse {
    pub series_id: Uuid,
    pub title: String,
    pub volume_count: i64,
}

#[derive(Debug, Deserialize)]
pub struct ManualVolumeInput {
    /// Existing volume id (update); omit to create a new volume
    pub id: Option<Uuid>,
    pub volume_number: Option<i64>,
    pub title: Option<String>,
    pub published_at: Option<String>,
    pub cover_url: Option<String>,
    /// Short display label override (단편집, SS집, …). Empty clears to auto.
    pub label: Option<String>,
    /// JP-only fill without Korean release (미정발)
    #[serde(default)]
    pub is_unreleased: bool,
}

#[derive(Debug, Deserialize)]
pub struct ManualSeriesRequest {
    pub title: String,
    pub author: Option<String>,
    pub publisher: Option<String>,
    /// Series representative cover (stored on series; preferred over volume covers in UI)
    pub cover_url: Option<String>,
    /// Attribution text shown on detail (manual series). Empty clears.
    #[serde(default)]
    pub source_label: Option<String>,
    /// Attribution link URL (manual series). Empty clears.
    #[serde(default)]
    pub source_url: Option<String>,
    /// Allow create even if an exact title already exists
    #[serde(default)]
    pub force: bool,
    /// If volumes is empty, create 1..=volume_count placeholder volumes
    pub volume_count: Option<i64>,
    #[serde(default)]
    pub volumes: Vec<ManualVolumeInput>,
}

#[derive(Debug, Deserialize)]
pub struct SetSeriesPublishStatusRequest {
    pub publish_status: String,
}

#[derive(Debug, Deserialize)]
pub struct VolumeOrderRequest {
    pub volume_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct MoveVolumesRequest {
    pub volume_ids: Vec<Uuid>,
    pub target_series_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct MergeSeriesRequest {
    pub source_series_id: Uuid,
    pub target_series_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct SplitVolumesRequest {
    pub volume_ids: Vec<Uuid>,
    pub title: String,
    pub author: Option<String>,
    pub publisher: Option<String>,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Serialize)]
pub struct NewReleaseRefreshResult {
    /// New-release items fetched from Aladin ItemList.
    pub scanned_items: i64,
    /// Unique catalog series matched from those items.
    pub matched_series: i64,
    pub refreshed: i64,
    pub failed: i64,
    /// Catalog-missing candidates upserted as pending suggestions.
    pub suggested: i64,
}

#[derive(Debug, Serialize)]
pub struct RefreshStartResponse {
    pub started: bool,
    pub already_running: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct NewReleaseSuggestionItem {
    pub id: Uuid,
    pub suggestion_key: String,
    pub aladin_series_id: Option<String>,
    pub sample_item_id: String,
    pub title: String,
    pub author: Option<String>,
    pub publisher: Option<String>,
    pub cover_url: Option<String>,
    pub pub_date: Option<String>,
    pub status: String,
    pub first_seen_at: String,
    pub last_seen_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CatalogRequestCreate {
    /// add | edit | other | search_improve
    pub request_type: String,
    pub series_id: Option<Uuid>,
    /// For search_improve: target series that the query should match (multi).
    #[serde(default)]
    pub series_ids: Option<Vec<Uuid>>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub publisher: Option<String>,
    pub aladin_series_id: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CatalogRequestReview {
    /// approved | rejected
    pub status: String,
    pub admin_note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CatalogRequestItem {
    pub id: Uuid,
    pub user_id: Uuid,
    pub user_email: Option<String>,
    pub request_type: String,
    pub series_id: Option<Uuid>,
    pub series_title: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub publisher: Option<String>,
    pub aladin_series_id: Option<String>,
    pub note: Option<String>,
    pub related_series_ids: Vec<Uuid>,
    pub related_series: Vec<CatalogRequestRelatedSeries>,
    pub status: String,
    pub admin_note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct CatalogRequestRelatedSeries {
    pub id: Uuid,
    pub title: String,
}

#[derive(Debug, Serialize)]
pub struct AdminStatusResponse {
    pub quota_date: String,
    pub quota_used: i64,
    pub quota_soft: i64,
    pub quota_hard: i64,
    pub quota_remaining: i64,
    pub server_time_kst: String,
    pub recent_quota: Vec<AdminQuotaDay>,
    pub last_refresh_at: Option<String>,
    pub last_refresh_note: Option<String>,
    /// True while a manual/scheduled new-release refresh is in progress.
    pub refresh_running: bool,
    pub last_scheduled_refresh_date: Option<String>,
    pub last_backup_at: Option<String>,
    pub backup_count: i64,
    pub backup_retain_days: i64,
    pub pending_requests: i64,
    pub pending_suggestions: i64,
    pub user_count: i64,
    pub manual_series_count: i64,
}

#[derive(Debug, Serialize)]
pub struct AdminQuotaDay {
    pub date: String,
    pub used: i64,
}

#[derive(Debug, Serialize)]
pub struct AdminManualSeriesItem {
    pub id: Uuid,
    pub title: String,
    pub author: Option<String>,
    pub publisher: Option<String>,
    pub volume_count: i64,
    pub latest_cover_url: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct TierlistEntry {
    pub series_id: Uuid,
    pub title: String,
    pub cover_url: Option<String>,
    pub position: i64,
}

#[derive(Debug, Serialize)]
pub struct TierlistTier {
    pub tier: String,
    pub entries: Vec<TierlistEntry>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TierlistGatekeeper {
    pub tier: String,
    pub above_series_id: Option<Uuid>,
    pub below_series_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct TierlistResponse {
    pub tiers: Vec<TierlistTier>,
    /// S~F 밖에 표시되는 미평가(None) 시리즈. 티어리스트 저장·이미지 내보내기 대상이 아님.
    pub none_entries: Vec<TierlistEntry>,
    pub gatekeepers: Vec<TierlistGatekeeper>,
}

/// Live showcase snapshot for the first-login tour (reads + tierlist of a fixed account).
#[derive(Debug, Serialize)]
pub struct TourDemoResponse {
    pub display_email: String,
    pub demo_user_email: String,
    pub tearmoon: SeriesDetailResponse,
    pub tierlist: TierlistResponse,
}

#[derive(Debug, Deserialize)]
pub struct SaveTierlistRequest {
    pub tiers: Vec<TierlistTierInput>,
    #[serde(default)]
    pub gatekeepers: Vec<TierlistGatekeeper>,
}

#[derive(Debug, Deserialize)]
pub struct TierlistTierInput {
    pub tier: String,
    pub series_ids: Vec<Uuid>,
}
