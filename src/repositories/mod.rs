use chrono::{DateTime, NaiveDate, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::{
        Rating, Series, SeriesRow, User, UserRow, Volume, VolumeRow, format_date, format_datetime,
        parse_uuid,
    },
};

/// Series row columns for `SeriesRow` queries.
const SERIES_COLS: &str = "id, title, author, publisher, aladin_series_id, \
     first_published_at, latest_published_at, created_at, cover_url, \
     COALESCE(publish_status, 'ongoing') AS publish_status";

/// Display cover: series.cover_url first, else latest volume that has a cover.
const SERIES_DISPLAY_COVER_SQL: &str = r#"
COALESCE(
    NULLIF(TRIM(s.cover_url), ''),
    (
        SELECT v.cover_url
        FROM volumes v
        WHERE v.series_id = s.id
          AND v.cover_url IS NOT NULL
          AND TRIM(v.cover_url) != ''
        ORDER BY v.volume_number DESC, v.published_at DESC
        LIMIT 1
    )
)"#;

pub async fn create_user(
    pool: &SqlitePool,
    email: &str,
    password_hash: &str,
    is_admin: bool,
    email_verified: bool,
    verify_token: Option<&str>,
    verify_token_expires_at: Option<&str>,
    terms_accepted_at: Option<&str>,
    privacy_accepted_at: Option<&str>,
    legal_version: Option<&str>,
) -> AppResult<User> {
    let id = Uuid::new_v4();
    let result = sqlx::query(
        r#"
        INSERT INTO users (
            id, email, password_hash, is_admin, email_verified,
            verify_token, verify_token_expires_at,
            terms_accepted_at, privacy_accepted_at, legal_version
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(id.to_string())
    .bind(email)
    .bind(password_hash)
    .bind(if is_admin { 1 } else { 0 })
    .bind(if email_verified { 1 } else { 0 })
    .bind(verify_token)
    .bind(verify_token_expires_at)
    .bind(terms_accepted_at)
    .bind(privacy_accepted_at)
    .bind(legal_version)
    .execute(pool)
    .await;

    match result {
        Ok(_) => find_by_id(pool, id)
            .await?
            .ok_or_else(|| AppError::Internal("failed to load created user".into())),
        Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
            Err(AppError::Conflict("이미 가입된 이메일입니다".into()))
        }
        Err(err) => Err(err.into()),
    }
}

pub async fn find_by_email(pool: &SqlitePool, email: &str) -> AppResult<Option<User>> {
    let row = sqlx::query_as::<_, UserRow>(
        r#"
        SELECT id, email, password_hash, created_at, is_admin, email_verified
        FROM users
        WHERE email = ? COLLATE NOCASE
        "#,
    )
    .bind(email.trim())
    .fetch_optional(pool)
    .await?;

    row.map(|r| r.into_user()).transpose()
}

pub async fn find_by_id(pool: &SqlitePool, id: Uuid) -> AppResult<Option<User>> {
    let row = sqlx::query_as::<_, UserRow>(
        r#"
        SELECT id, email, password_hash, created_at, is_admin, email_verified
        FROM users
        WHERE id = ?
        "#,
    )
    .bind(id.to_string())
    .fetch_optional(pool)
    .await?;

    row.map(|r| r.into_user()).transpose()
}

pub async fn set_verify_token(
    pool: &SqlitePool,
    user_id: Uuid,
    token: &str,
    expires_at: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE users SET
            verify_token = ?,
            verify_token_expires_at = ?,
            email_verified = 0
        WHERE id = ?
        "#,
    )
    .bind(token)
    .bind(expires_at)
    .bind(user_id.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

/// Update an unverified account on re-registration (password, consent, verify token).
pub async fn refresh_unverified_registration(
    pool: &SqlitePool,
    user_id: Uuid,
    password_hash: &str,
    is_admin: bool,
    email_verified: bool,
    verify_token: Option<&str>,
    verify_token_expires_at: Option<&str>,
    terms_accepted_at: Option<&str>,
    privacy_accepted_at: Option<&str>,
    legal_version: Option<&str>,
) -> AppResult<User> {
    let result = sqlx::query(
        r#"
        UPDATE users SET
            password_hash = ?,
            is_admin = ?,
            email_verified = ?,
            verify_token = ?,
            verify_token_expires_at = ?,
            terms_accepted_at = ?,
            privacy_accepted_at = ?,
            legal_version = ?
        WHERE id = ? AND email_verified = 0
        "#,
    )
    .bind(password_hash)
    .bind(if is_admin { 1 } else { 0 })
    .bind(if email_verified { 1 } else { 0 })
    .bind(verify_token)
    .bind(verify_token_expires_at)
    .bind(terms_accepted_at)
    .bind(privacy_accepted_at)
    .bind(legal_version)
    .bind(user_id.to_string())
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::Conflict("이미 가입된 이메일입니다".into()));
    }
    find_by_id(pool, user_id)
        .await?
        .ok_or_else(|| AppError::Internal("failed to load updated user".into()))
}

pub async fn verify_email_by_token(pool: &SqlitePool, token: &str) -> AppResult<Option<User>> {
    let row = sqlx::query_as::<_, UserRow>(
        r#"
        SELECT id, email, password_hash, created_at, is_admin, email_verified
        FROM users
        WHERE verify_token = ?
          AND verify_token_expires_at IS NOT NULL
          AND verify_token_expires_at >= strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        "#,
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };
    let user = row.into_user()?;
    sqlx::query(
        r#"
        UPDATE users SET
            email_verified = 1,
            verify_token = NULL,
            verify_token_expires_at = NULL
        WHERE id = ?
        "#,
    )
    .bind(user.id.to_string())
    .execute(pool)
    .await?;
    find_by_id(pool, user.id).await
}

pub async fn list_users(pool: &SqlitePool) -> AppResult<Vec<User>> {
    let rows = sqlx::query_as::<_, UserRow>(
        r#"
        SELECT id, email, password_hash, created_at, is_admin, email_verified
        FROM users
        ORDER BY created_at ASC
        "#,
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(|r| r.into_user()).collect()
}

pub async fn count_admins(pool: &SqlitePool) -> AppResult<i64> {
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM users WHERE is_admin = 1")
            .fetch_one(pool)
            .await?;
    Ok(count.0)
}

pub async fn delete_user(pool: &SqlitePool, user_id: Uuid) -> AppResult<()> {
    // Cascades: reads, ratings, tierlist, catalog_requests
    let result = sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(user_id.to_string())
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("user not found".into()));
    }
    Ok(())
}

pub async fn ensure_admin_by_email(pool: &SqlitePool, email: &str) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE users SET is_admin = 1, email_verified = 1
        WHERE lower(email) = lower(?)
        "#,
    )
    .bind(email.trim())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn find_series_by_id(pool: &SqlitePool, id: Uuid) -> AppResult<Option<Series>> {
    let row = sqlx::query_as::<_, SeriesRow>(&format!(
        "SELECT {SERIES_COLS} FROM series WHERE id = ?"
    ))
    .bind(id.to_string())
    .fetch_optional(pool)
    .await?;

    row.map(|r| r.into_series()).transpose()
}

pub async fn count_manual_series(pool: &SqlitePool) -> AppResult<i64> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM series WHERE aladin_series_id LIKE 'manual:%'",
    )
    .fetch_one(pool)
    .await?;
    Ok(count.0)
}

pub async fn list_manual_series(
    pool: &SqlitePool,
) -> AppResult<Vec<crate::models::AdminManualSeriesItem>> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            i64,
            Option<String>,
        ),
    >(&format!(
        r#"
        SELECT
            s.id,
            s.title,
            s.author,
            s.publisher,
            s.created_at,
            (SELECT COUNT(*) FROM volumes v WHERE v.series_id = s.id) AS volume_count,
            {SERIES_DISPLAY_COVER_SQL} AS latest_cover_url
        FROM series s
        WHERE s.aladin_series_id LIKE 'manual:%'
        ORDER BY s.title COLLATE NOCASE ASC
        "#
    ))
    .fetch_all(pool)
    .await?;

    let mut items = Vec::with_capacity(rows.len());
    for (id, title, author, publisher, created_at, volume_count, latest_cover_url) in rows {
        items.push(crate::models::AdminManualSeriesItem {
            id: parse_uuid(&id)?,
            title,
            author,
            publisher,
            volume_count,
            latest_cover_url,
            created_at,
        });
    }
    Ok(items)
}

pub async fn find_series_by_title_exact(
    pool: &SqlitePool,
    title: &str,
) -> AppResult<Option<Series>> {
    let row = sqlx::query_as::<_, SeriesRow>(&format!(
        r#"
        SELECT {SERIES_COLS}
        FROM series
        WHERE title = ? COLLATE NOCASE
        ORDER BY created_at ASC
        LIMIT 1
        "#
    ))
    .bind(title.trim())
    .fetch_optional(pool)
    .await?;

    row.map(|r| r.into_series()).transpose()
}

pub async fn find_series_by_aladin_id(
    pool: &SqlitePool,
    aladin_series_id: &str,
) -> AppResult<Option<Series>> {
    let row = sqlx::query_as::<_, SeriesRow>(&format!(
        "SELECT {SERIES_COLS} FROM series WHERE aladin_series_id = ?"
    ))
    .bind(aladin_series_id)
    .fetch_optional(pool)
    .await?;
    if let Some(row) = row {
        return row.into_series().map(Some);
    }

    let row = sqlx::query_as::<_, SeriesRow>(
        r#"
        SELECT
            s.id,
            s.title,
            s.author,
            s.publisher,
            s.aladin_series_id,
            s.first_published_at,
            s.latest_published_at,
            s.created_at,
            s.cover_url,
            COALESCE(s.publish_status, 'ongoing') AS publish_status
        FROM series s
        INNER JOIN series_aladin_aliases a ON a.series_id = s.id
        WHERE a.aladin_series_id = ?
        "#,
    )
    .bind(aladin_series_id)
    .fetch_optional(pool)
    .await?;

    row.map(|r| r.into_series()).transpose()
}

pub async fn list_series_aladin_aliases(
    pool: &SqlitePool,
    series_id: Uuid,
) -> AppResult<Vec<String>> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT aladin_series_id FROM series_aladin_aliases WHERE series_id = ?")
            .bind(series_id.to_string())
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

pub async fn add_series_aladin_alias(
    pool: &SqlitePool,
    series_id: Uuid,
    aladin_series_id: &str,
) -> AppResult<()> {
    let key = aladin_series_id.trim();
    if key.is_empty() || key.starts_with("manual:") {
        return Ok(());
    }
    if let Some(current) = find_series_by_id(pool, series_id).await? {
        if current.aladin_series_id == key {
            return Ok(());
        }
    }
    sqlx::query(
        r#"
        INSERT INTO series_aladin_aliases (aladin_series_id, series_id)
        VALUES (?, ?)
        ON CONFLICT(aladin_series_id) DO UPDATE SET series_id = excluded.series_id
        "#,
    )
    .bind(key)
    .bind(series_id.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

/// Move Aladin identities from a merged-away series onto the surviving series.
pub async fn absorb_series_aladin_identities(
    pool: &SqlitePool,
    source_id: Uuid,
    target_id: Uuid,
) -> AppResult<()> {
    let source = find_series_by_id(pool, source_id)
        .await?
        .ok_or_else(|| AppError::NotFound("source series not found".into()))?;
    let target = find_series_by_id(pool, target_id)
        .await?
        .ok_or_else(|| AppError::NotFound("target series not found".into()))?;

    let mut incoming = list_series_aladin_aliases(pool, source_id).await?;
    if !source.aladin_series_id.starts_with("manual:") {
        incoming.push(source.aladin_series_id.clone());
    }

    sqlx::query("DELETE FROM series_aladin_aliases WHERE series_id = ?")
        .bind(source_id.to_string())
        .execute(pool)
        .await?;

    let mut target_key = target.aladin_series_id.clone();
    if target_key.starts_with("manual:") {
        if let Some(first) = incoming
            .iter()
            .find(|key| !key.starts_with("manual:"))
            .cloned()
        {
            sqlx::query("UPDATE series SET aladin_series_id = ? WHERE id = ?")
                .bind(format!("manual:{source_id}:merged"))
                .bind(source_id.to_string())
                .execute(pool)
                .await?;
            sqlx::query("UPDATE series SET aladin_series_id = ? WHERE id = ?")
                .bind(&first)
                .bind(target_id.to_string())
                .execute(pool)
                .await?;
            target_key = first.clone();
            incoming.retain(|key| key != &first);
        }
    }

    for key in incoming {
        if key == target_key || key.starts_with("manual:") {
            continue;
        }
        add_series_aladin_alias(pool, target_id, &key).await?;
    }
    Ok(())
}

pub async fn remove_series_aladin_aliases(
    pool: &SqlitePool,
    series_id: Uuid,
    keys: &[String],
) -> AppResult<()> {
    for key in keys {
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        sqlx::query(
            "DELETE FROM series_aladin_aliases WHERE series_id = ? AND aladin_series_id = ?",
        )
        .bind(series_id.to_string())
        .bind(key)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn upsert_series(
    pool: &SqlitePool,
    id: Uuid,
    title: &str,
    author: Option<&str>,
    publisher: Option<&str>,
    aladin_series_id: &str,
    first_published_at: Option<DateTime<Utc>>,
    latest_published_at: Option<DateTime<Utc>>,
) -> AppResult<Series> {
    upsert_series_with_cover(
        pool,
        id,
        title,
        author,
        publisher,
        aladin_series_id,
        first_published_at,
        latest_published_at,
        None,
    )
    .await
}

pub async fn upsert_series_with_cover(
    pool: &SqlitePool,
    id: Uuid,
    title: &str,
    author: Option<&str>,
    publisher: Option<&str>,
    aladin_series_id: &str,
    first_published_at: Option<DateTime<Utc>>,
    latest_published_at: Option<DateTime<Utc>>,
    cover_url: Option<&str>,
) -> AppResult<Series> {
    let cover = cover_url.map(str::trim).filter(|s| !s.is_empty());
    let row = sqlx::query_as::<_, SeriesRow>(&format!(
        r#"
        INSERT INTO series (
            id, title, author, publisher, aladin_series_id,
            first_published_at, latest_published_at, cover_url
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(aladin_series_id) DO UPDATE SET
            first_published_at = COALESCE(
                excluded.first_published_at,
                series.first_published_at
            ),
            latest_published_at = COALESCE(
                excluded.latest_published_at,
                series.latest_published_at
            ),
            -- Keep an existing series cover; only fill when empty
            cover_url = COALESCE(
                NULLIF(TRIM(series.cover_url), ''),
                excluded.cover_url
            )
        RETURNING {SERIES_COLS}
        "#
    ))
    .bind(id.to_string())
    .bind(title)
    .bind(author)
    .bind(publisher)
    .bind(aladin_series_id)
    .bind(first_published_at.map(format_datetime))
    .bind(latest_published_at.map(format_datetime))
    .bind(cover)
    .fetch_one(pool)
    .await?;

    row.into_series()
}

pub async fn upsert_volume(
    pool: &SqlitePool,
    id: Uuid,
    series_id: Uuid,
    volume_number: i64,
    title: &str,
    cover_url: Option<&str>,
    published_at: Option<NaiveDate>,
    aladin_item_id: &str,
    isbn13: Option<&str>,
) -> AppResult<Volume> {
    let published = published_at.map(format_date);

    let existing_item = sqlx::query_as::<_, VolumeRow>(
        r#"
        SELECT id, series_id, volume_number, title, cover_url, published_at, aladin_item_id, isbn13, is_unreleased
        FROM volumes
        WHERE aladin_item_id = ?
        "#,
    )
    .bind(aladin_item_id)
    .fetch_optional(pool)
    .await?;

    if let Some(item_row) = existing_item {
        // 기존 권: 제목·권번호·소속 시리즈 유지. 표지/ISBN만 비어 있으면 채우고, 더 이른 출간일만 반영.
        let row = sqlx::query_as::<_, VolumeRow>(
            r#"
            UPDATE volumes SET
                cover_url = CASE
                    WHEN cover_url IS NULL OR TRIM(cover_url) = '' THEN ?
                    ELSE cover_url
                END,
                published_at = CASE
                    WHEN ? IS NULL THEN published_at
                    WHEN published_at IS NULL OR published_at = '' THEN ?
                    WHEN ? < published_at THEN ?
                    ELSE published_at
                END,
                isbn13 = COALESCE(isbn13, ?)
            WHERE id = ?
            RETURNING id, series_id, volume_number, title, cover_url, published_at, aladin_item_id, isbn13, is_unreleased
            "#,
        )
        .bind(cover_url)
        .bind(&published)
        .bind(&published)
        .bind(&published)
        .bind(&published)
        .bind(isbn13)
        .bind(&item_row.id)
        .fetch_one(pool)
        .await?;

        return row.into_volume();
    }

    let row = sqlx::query_as::<_, VolumeRow>(
        r#"
        INSERT INTO volumes (id, series_id, volume_number, title, cover_url, published_at, aladin_item_id, isbn13, is_unreleased)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0)
        RETURNING id, series_id, volume_number, title, cover_url, published_at, aladin_item_id, isbn13, is_unreleased
        "#,
    )
    .bind(id.to_string())
    .bind(series_id.to_string())
    .bind(volume_number)
    .bind(title)
    .bind(cover_url)
    .bind(&published)
    .bind(aladin_item_id)
    .bind(isbn13)
    .fetch_one(pool)
    .await?;

    row.into_volume()
}

pub async fn find_volume_by_id(pool: &SqlitePool, id: Uuid) -> AppResult<Option<Volume>> {
    let row = sqlx::query_as::<_, VolumeRow>(
        r#"
        SELECT id, series_id, volume_number, title, cover_url, published_at, aladin_item_id, isbn13, is_unreleased
        FROM volumes
        WHERE id = ?
        "#,
    )
    .bind(id.to_string())
    .fetch_optional(pool)
    .await?;
    row.map(|r| r.into_volume()).transpose()
}

pub async fn find_volume_by_aladin_item_id(
    pool: &SqlitePool,
    aladin_item_id: &str,
) -> AppResult<Option<Volume>> {
    let row = sqlx::query_as::<_, VolumeRow>(
        r#"
        SELECT id, series_id, volume_number, title, cover_url, published_at, aladin_item_id, isbn13, is_unreleased
        FROM volumes
        WHERE aladin_item_id = ?
        "#,
    )
    .bind(aladin_item_id)
    .fetch_optional(pool)
    .await?;
    row.map(|r| r.into_volume()).transpose()
}

pub async fn max_volume_number(pool: &SqlitePool, series_id: Uuid) -> AppResult<i64> {
    let row: (Option<i64>,) =
        sqlx::query_as("SELECT MAX(volume_number) FROM volumes WHERE series_id = ?")
            .bind(series_id.to_string())
            .fetch_one(pool)
            .await?;
    Ok(row.0.unwrap_or(0).max(0))
}

pub async fn renumber_volumes_in_order(
    pool: &SqlitePool,
    series_id: Uuid,
    volume_ids: &[Uuid],
) -> AppResult<()> {
    let current = list_volumes(pool, series_id, "asc").await?;
    let current_ids: std::collections::HashSet<Uuid> = current.iter().map(|v| v.id).collect();
    if volume_ids.len() != current.len() || volume_ids.iter().any(|id| !current_ids.contains(id)) {
        return Err(AppError::BadRequest(
            "volume_ids must list every volume in the series exactly once".into(),
        ));
    }

    const STAGE_BASE: i64 = -2_000_000;
    for (idx, id) in volume_ids.iter().enumerate() {
        set_volume_number(pool, *id, STAGE_BASE - idx as i64).await?;
    }
    for (idx, id) in volume_ids.iter().enumerate() {
        set_volume_number(pool, *id, (idx + 1) as i64).await?;
    }
    Ok(())
}

pub async fn move_volume_to_series(
    pool: &SqlitePool,
    volume_id: Uuid,
    from_series_id: Uuid,
    to_series_id: Uuid,
    volume_number: i64,
) -> AppResult<()> {
    let result = sqlx::query(
        r#"
        UPDATE volumes SET series_id = ?, volume_number = ?
        WHERE id = ? AND series_id = ?
        "#,
    )
    .bind(to_series_id.to_string())
    .bind(volume_number)
    .bind(volume_id.to_string())
    .bind(from_series_id.to_string())
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("volume not found".into()));
    }
    Ok(())
}

/// Move ratings from source → target when target has none; drop the rest.
pub async fn merge_series_ratings(
    pool: &SqlitePool,
    source_series_id: Uuid,
    target_series_id: Uuid,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO user_series_ratings (user_id, series_id, rating, updated_at)
        SELECT usr.user_id, ?, usr.rating, usr.updated_at
        FROM user_series_ratings usr
        WHERE usr.series_id = ?
          AND NOT EXISTS (
            SELECT 1 FROM user_series_ratings t
            WHERE t.user_id = usr.user_id AND t.series_id = ?
          )
        "#,
    )
    .bind(target_series_id.to_string())
    .bind(source_series_id.to_string())
    .bind(target_series_id.to_string())
    .execute(pool)
    .await?;

    sqlx::query("DELETE FROM user_series_ratings WHERE series_id = ?")
        .bind(source_series_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

/// Retarget tierlist entries from source → target; drop conflicts on target.
pub async fn merge_series_tierlist_entries(
    pool: &SqlitePool,
    source_series_id: Uuid,
    target_series_id: Uuid,
) -> AppResult<()> {
    sqlx::query(
        r#"
        DELETE FROM user_tierlist_entries
        WHERE series_id = ?
          AND user_id IN (
            SELECT user_id FROM (
              SELECT user_id FROM user_tierlist_entries WHERE series_id = ?
            )
          )
        "#,
    )
    .bind(source_series_id.to_string())
    .bind(target_series_id.to_string())
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        UPDATE user_tierlist_entries SET series_id = ?
        WHERE series_id = ?
        "#,
    )
    .bind(target_series_id.to_string())
    .bind(source_series_id.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_volumes(
    pool: &SqlitePool,
    series_id: Uuid,
    order: &str,
) -> AppResult<Vec<Volume>> {
    let order_clause = if order == "asc" {
        "ORDER BY volume_number ASC"
    } else {
        "ORDER BY volume_number DESC"
    };

    let query = format!(
        r#"
        SELECT id, series_id, volume_number, title, cover_url, published_at, aladin_item_id, isbn13, is_unreleased
        FROM volumes
        WHERE series_id = ?
        {order_clause}
        "#
    );

    let rows = sqlx::query_as::<_, VolumeRow>(&query)
        .bind(series_id.to_string())
        .fetch_all(pool)
        .await?;

    rows.into_iter().map(|r| r.into_volume()).collect()
}

/// 시리즈의 첫/최신 출간일을 권 목록에서 다시 계산한다.
pub async fn refresh_series_publish_dates(
    pool: &SqlitePool,
    series_id: Uuid,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE series SET
            first_published_at = (
                SELECT MIN(published_at) || 'T00:00:00Z' FROM volumes
                WHERE series_id = ? AND published_at IS NOT NULL AND published_at != ''
            ),
            latest_published_at = (
                SELECT MAX(published_at) || 'T00:00:00Z' FROM volumes
                WHERE series_id = ? AND published_at IS NOT NULL AND published_at != ''
            )
        WHERE id = ?
        "#,
    )
    .bind(series_id.to_string())
    .bind(series_id.to_string())
    .bind(series_id.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn count_volumes(pool: &SqlitePool, series_id: Uuid) -> AppResult<i64> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM volumes WHERE series_id = ?")
        .bind(series_id.to_string())
        .fetch_one(pool)
        .await?;
    Ok(count.0)
}

pub async fn update_series_meta(
    pool: &SqlitePool,
    series_id: Uuid,
    title: &str,
    author: Option<&str>,
    publisher: Option<&str>,
    cover_url: Option<&str>,
) -> AppResult<Series> {
    let cover = cover_url.map(str::trim).filter(|s| !s.is_empty());
    let row = sqlx::query_as::<_, SeriesRow>(&format!(
        r#"
        UPDATE series SET
            title = ?,
            author = ?,
            publisher = ?,
            cover_url = ?
        WHERE id = ?
        RETURNING {SERIES_COLS}
        "#
    ))
    .bind(title)
    .bind(author)
    .bind(publisher)
    .bind(cover)
    .bind(series_id.to_string())
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("series not found".into()))?;

    row.into_series()
}

/// Fix polluted series keys (e.g. title ending with "09 (상)") to the canonical group id.
/// If the canonical key is already taken by another row, only the title is updated.
pub async fn canonicalize_series_identity(
    pool: &SqlitePool,
    series_id: Uuid,
    aladin_series_id: &str,
    title: &str,
) -> AppResult<Series> {
    let current = find_series_by_id(pool, series_id)
        .await?
        .ok_or_else(|| AppError::NotFound("series not found".into()))?;

    if current.aladin_series_id == aladin_series_id && current.title == title {
        return Ok(current);
    }

    let conflict = find_series_by_aladin_id(pool, aladin_series_id).await?;
    let can_rekey = conflict
        .as_ref()
        .map(|s| s.id == series_id)
        .unwrap_or(true);

    let row = if can_rekey {
        sqlx::query_as::<_, SeriesRow>(&format!(
            r#"
            UPDATE series SET title = ?, aladin_series_id = ?
            WHERE id = ?
            RETURNING {SERIES_COLS}
            "#
        ))
        .bind(title)
        .bind(aladin_series_id)
        .bind(series_id.to_string())
        .fetch_optional(pool)
        .await?
    } else {
        sqlx::query_as::<_, SeriesRow>(&format!(
            r#"
            UPDATE series SET title = ?
            WHERE id = ?
            RETURNING {SERIES_COLS}
            "#
        ))
        .bind(title)
        .bind(series_id.to_string())
        .fetch_optional(pool)
        .await?
    }
    .ok_or_else(|| AppError::NotFound("series not found".into()))?;

    row.into_series()
}

pub async fn set_series_publish_status(
    pool: &SqlitePool,
    series_id: Uuid,
    publish_status: &str,
) -> AppResult<Series> {
    let status = crate::models::normalize_publish_status(publish_status).ok_or_else(|| {
        AppError::BadRequest(format!(
            "invalid publish_status (expected one of: {})",
            crate::models::PUBLISH_STATUSES.join(", ")
        ))
    })?;
    let row = sqlx::query_as::<_, SeriesRow>(&format!(
        r#"
        UPDATE series
        SET publish_status = ?
        WHERE id = ?
        RETURNING {SERIES_COLS}
        "#
    ))
    .bind(status)
    .bind(series_id.to_string())
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("series not found".into()))?;

    row.into_series()
}

pub async fn delete_series(pool: &SqlitePool, series_id: Uuid) -> AppResult<()> {
    let result = sqlx::query("DELETE FROM series WHERE id = ?")
        .bind(series_id.to_string())
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("series not found".into()));
    }
    Ok(())
}

/// Insert a volume with an exact volume_number (manual registration).
pub async fn insert_manual_volume(
    pool: &SqlitePool,
    id: Uuid,
    series_id: Uuid,
    volume_number: i64,
    title: &str,
    cover_url: Option<&str>,
    published_at: Option<NaiveDate>,
    is_unreleased: bool,
) -> AppResult<Volume> {
    let published = published_at.map(format_date);
    let row = sqlx::query_as::<_, VolumeRow>(
        r#"
        INSERT INTO volumes (id, series_id, volume_number, title, cover_url, published_at, aladin_item_id, isbn13, is_unreleased)
        VALUES (?, ?, ?, ?, ?, ?, ?, NULL, ?)
        RETURNING id, series_id, volume_number, title, cover_url, published_at, aladin_item_id, isbn13, is_unreleased
        "#,
    )
    .bind(id.to_string())
    .bind(series_id.to_string())
    .bind(volume_number)
    .bind(title)
    .bind(cover_url)
    .bind(&published)
    .bind(format!("manual-vol:{id}"))
    .bind(if is_unreleased { 1 } else { 0 })
    .fetch_one(pool)
    .await?;

    row.into_volume()
}

pub async fn update_manual_volume(
    pool: &SqlitePool,
    id: Uuid,
    series_id: Uuid,
    volume_number: i64,
    title: &str,
    cover_url: Option<&str>,
    published_at: Option<NaiveDate>,
    is_unreleased: bool,
) -> AppResult<Volume> {
    let published = published_at.map(format_date);
    let row = sqlx::query_as::<_, VolumeRow>(
        r#"
        UPDATE volumes SET
            volume_number = ?,
            title = ?,
            cover_url = ?,
            published_at = ?,
            is_unreleased = ?
        WHERE id = ? AND series_id = ?
        RETURNING id, series_id, volume_number, title, cover_url, published_at, aladin_item_id, isbn13, is_unreleased
        "#,
    )
    .bind(volume_number)
    .bind(title)
    .bind(cover_url)
    .bind(&published)
    .bind(if is_unreleased { 1 } else { 0 })
    .bind(id.to_string())
    .bind(series_id.to_string())
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("volume not found".into()))?;

    row.into_volume()
}

pub async fn delete_volume(pool: &SqlitePool, id: Uuid, series_id: Uuid) -> AppResult<()> {
    let result = sqlx::query("DELETE FROM volumes WHERE id = ? AND series_id = ?")
        .bind(id.to_string())
        .bind(series_id.to_string())
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("volume not found".into()));
    }
    Ok(())
}

/// Fill empty cover/ISBN and adopt an earlier published_at without changing identity fields.
pub async fn refresh_volume_metadata(
    pool: &SqlitePool,
    volume_id: Uuid,
    cover_url: Option<&str>,
    published_at: Option<NaiveDate>,
    isbn13: Option<&str>,
) -> AppResult<()> {
    let published = published_at.map(format_date);
    sqlx::query(
        r#"
        UPDATE volumes SET
            cover_url = CASE
                WHEN cover_url IS NULL OR TRIM(cover_url) = '' THEN ?
                ELSE cover_url
            END,
            published_at = CASE
                WHEN ? IS NULL THEN published_at
                WHEN published_at IS NULL OR published_at = '' THEN ?
                WHEN ? < published_at THEN ?
                ELSE published_at
            END,
            isbn13 = COALESCE(isbn13, ?)
        WHERE id = ?
        "#,
    )
    .bind(cover_url)
    .bind(&published)
    .bind(&published)
    .bind(&published)
    .bind(&published)
    .bind(isbn13)
    .bind(volume_id.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

/// Move read flags from `drop_id` onto `keep_id`, then delete `drop_id`.
pub async fn merge_and_delete_volume(
    pool: &SqlitePool,
    series_id: Uuid,
    keep_id: Uuid,
    drop_id: Uuid,
) -> AppResult<()> {
    if keep_id == drop_id {
        return Ok(());
    }
    sqlx::query(
        r#"
        INSERT INTO user_volume_reads (user_id, volume_id, is_read, updated_at)
        SELECT user_id, ?, is_read, updated_at
        FROM user_volume_reads
        WHERE volume_id = ?
        ON CONFLICT(user_id, volume_id) DO UPDATE SET
            is_read = CASE
                WHEN excluded.is_read = 1 OR user_volume_reads.is_read = 1 THEN 1
                ELSE 0
            END,
            updated_at = CASE
                WHEN excluded.updated_at > user_volume_reads.updated_at THEN excluded.updated_at
                ELSE user_volume_reads.updated_at
            END
        "#,
    )
    .bind(keep_id.to_string())
    .bind(drop_id.to_string())
    .execute(pool)
    .await?;

    delete_volume(pool, drop_id, series_id).await
}

pub async fn set_volume_number(
    pool: &SqlitePool,
    id: Uuid,
    volume_number: i64,
) -> AppResult<()> {
    sqlx::query("UPDATE volumes SET volume_number = ? WHERE id = ?")
        .bind(volume_number)
        .bind(id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_rating(
    pool: &SqlitePool,
    user_id: Uuid,
    series_id: Uuid,
) -> AppResult<Rating> {
    let rating: Option<(String,)> = sqlx::query_as(
        "SELECT rating FROM user_series_ratings WHERE user_id = ? AND series_id = ?",
    )
    .bind(user_id.to_string())
    .bind(series_id.to_string())
    .fetch_optional(pool)
    .await?;

    Ok(match rating {
        Some((value,)) => value.parse().unwrap_or(Rating::None),
        None => Rating::None,
    })
}

pub async fn upsert_rating(
    pool: &SqlitePool,
    user_id: Uuid,
    series_id: Uuid,
    rating: &Rating,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO user_series_ratings (user_id, series_id, rating, updated_at)
        VALUES (?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
        ON CONFLICT(user_id, series_id) DO UPDATE SET
            rating = excluded.rating,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        "#,
    )
    .bind(user_id.to_string())
    .bind(series_id.to_string())
    .bind(rating.as_str())
    .execute(pool)
    .await?;

    if *rating == Rating::None {
        sqlx::query("DELETE FROM user_tierlist_entries WHERE user_id = ? AND series_id = ?")
            .bind(user_id.to_string())
            .bind(series_id.to_string())
            .execute(pool)
            .await?;
    }

    Ok(())
}

pub async fn get_read_map(
    pool: &SqlitePool,
    user_id: Uuid,
    series_id: Uuid,
) -> AppResult<std::collections::HashMap<Uuid, bool>> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        r#"
        SELECT v.id, uvr.is_read
        FROM volumes v
        LEFT JOIN user_volume_reads uvr
            ON uvr.volume_id = v.id AND uvr.user_id = ?
        WHERE v.series_id = ?
        "#,
    )
    .bind(user_id.to_string())
    .bind(series_id.to_string())
    .fetch_all(pool)
    .await?;

    let mut map = std::collections::HashMap::new();
    for (volume_id, is_read) in rows {
        map.insert(parse_uuid(&volume_id)?, is_read != 0);
    }
    Ok(map)
}

pub async fn save_reads(
    pool: &SqlitePool,
    user_id: Uuid,
    series_id: Uuid,
    reads: &[(Uuid, bool)],
) -> AppResult<()> {
    let mut tx = pool.begin().await?;

    for (volume_id, is_read) in reads {
        let exists: Option<(i64,)> = sqlx::query_as(
            "SELECT 1 FROM volumes WHERE id = ? AND series_id = ?",
        )
        .bind(volume_id.to_string())
        .bind(series_id.to_string())
        .fetch_optional(&mut *tx)
        .await?;

        if exists.is_none() {
            return Err(AppError::BadRequest(format!(
                "volume {} does not belong to series",
                volume_id
            )));
        }

        sqlx::query(
            r#"
            INSERT INTO user_volume_reads (user_id, volume_id, is_read, updated_at)
            VALUES (?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            ON CONFLICT(user_id, volume_id) DO UPDATE SET
                is_read = excluded.is_read,
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            "#,
        )
        .bind(user_id.to_string())
        .bind(volume_id.to_string())
        .bind(if *is_read { 1 } else { 0 })
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

pub async fn count_read_volumes(
    pool: &SqlitePool,
    user_id: Uuid,
    series_id: Uuid,
) -> AppResult<i64> {
    let count: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*)
        FROM user_volume_reads uvr
        JOIN volumes v ON v.id = uvr.volume_id
        WHERE uvr.user_id = ? AND v.series_id = ? AND uvr.is_read = 1
        "#,
    )
    .bind(user_id.to_string())
    .bind(series_id.to_string())
    .fetch_one(pool)
    .await?;
    Ok(count.0)
}

pub async fn latest_cover_for_series(
    pool: &SqlitePool,
    series_id: Uuid,
) -> AppResult<Option<String>> {
    let cover: Option<(Option<String>,)> = sqlx::query_as(&format!(
        r#"
        SELECT {SERIES_DISPLAY_COVER_SQL}
        FROM series s
        WHERE s.id = ?
        "#
    ))
    .bind(series_id.to_string())
    .fetch_optional(pool)
    .await?;

    Ok(cover.and_then(|(url,)| url.filter(|s| !s.trim().is_empty())))
}

pub struct SeriesListRow {
    pub id: Uuid,
    pub title: String,
    pub latest_cover_url: Option<String>,
    pub total_volumes: i64,
    pub read_volumes: i64,
    pub publish_status: String,
}

pub async fn list_series(
    pool: &SqlitePool,
    user_id: Uuid,
    sort: &str,
    page: i64,
    limit: i64,
    q: &str,
    status: &str,
) -> AppResult<(Vec<SeriesListRow>, i64)> {
    let offset = (page - 1).max(0) * limit;
    let q = q.trim();
    let status = match status.trim() {
        "read" | "unread" | "unrated" => status.trim(),
        _ => "all",
    };
    let q_pattern = if q.is_empty() {
        String::new()
    } else {
        format!("%{q}%")
    };

    // Shared filters: title query + read/rating status
    let status_sql = match status {
        // 한 권이라도 읽은 작품 (읽는 중 + 완독)
        "read" => "base.read_volumes > 0",
        "unread" => "base.read_volumes = 0",
        // 읽은 작품 중 평가가 없는 것만
        "unrated" => {
            "base.read_volumes > 0 AND (base.rating IS NULL OR base.rating = 'None')"
        }
        _ => "1=1",
    };

    let title_sql = if q.is_empty() {
        "1=1"
    } else {
        "base.title LIKE ? COLLATE NOCASE"
    };

    let base_cte = format!(
        r#"
        WITH base AS (
            SELECT
                s.id,
                s.title,
                s.latest_published_at,
                COALESCE(NULLIF(TRIM(s.publish_status), ''), 'ongoing') AS publish_status,
                {SERIES_DISPLAY_COVER_SQL} AS cover,
                (SELECT COUNT(*) FROM volumes v WHERE v.series_id = s.id) AS total_volumes,
                (
                    SELECT COUNT(*)
                    FROM user_volume_reads uvr
                    JOIN volumes v ON v.id = uvr.volume_id
                    WHERE uvr.user_id = ? AND v.series_id = s.id AND uvr.is_read = 1
                ) AS read_volumes,
                (
                    SELECT usr.rating
                    FROM user_series_ratings usr
                    WHERE usr.user_id = ? AND usr.series_id = s.id
                ) AS rating
            FROM series s
        )
        "#
    );

    let count_sql = format!(
        "{base_cte} SELECT COUNT(*) FROM base WHERE {title_sql} AND ({status_sql})"
    );
    let mut count_q = sqlx::query_as::<_, (i64,)>(&count_sql)
        .bind(user_id.to_string())
        .bind(user_id.to_string());
    if !q.is_empty() {
        count_q = count_q.bind(&q_pattern);
    }
    let total: (i64,) = count_q.fetch_one(pool).await?;

    let order_sql = match sort {
        "recently_read" => "lr.last_read_at DESC, base.title ASC",
        "popular" => "pop.reader_count DESC, base.title ASC",
        "title" => "base.title COLLATE NOCASE ASC",
        "oldest" => "base.latest_published_at ASC, base.title ASC",
        _ => "base.latest_published_at DESC, base.title ASC",
    };

    let join_sql = match sort {
        "recently_read" => r#"
            LEFT JOIN (
                SELECT v.series_id, MAX(uvr.updated_at) AS last_read_at
                FROM user_volume_reads uvr
                JOIN volumes v ON v.id = uvr.volume_id
                WHERE uvr.user_id = ? AND uvr.is_read = 1
                GROUP BY v.series_id
            ) lr ON lr.series_id = base.id
        "#,
        "popular" => r#"
            LEFT JOIN (
                SELECT v.series_id, COUNT(DISTINCT uvr.user_id) AS reader_count
                FROM user_volume_reads uvr
                JOIN volumes v ON v.id = uvr.volume_id
                WHERE uvr.is_read = 1
                GROUP BY v.series_id
            ) pop ON pop.series_id = base.id
        "#,
        _ => "",
    };

    let list_sql = format!(
        "{base_cte}
         SELECT base.id, base.title, base.cover, base.total_volumes, base.read_volumes, base.publish_status
         FROM base
         {join_sql}
         WHERE {title_sql} AND ({status_sql})
         ORDER BY {order_sql}
         LIMIT ? OFFSET ?"
    );

    let mut list_q =
        sqlx::query_as::<_, (String, String, Option<String>, i64, i64, String)>(&list_sql)
            .bind(user_id.to_string())
            .bind(user_id.to_string());
    if sort == "recently_read" {
        list_q = list_q.bind(user_id.to_string());
    }
    if !q.is_empty() {
        list_q = list_q.bind(&q_pattern);
    }
    list_q = list_q.bind(limit).bind(offset);
    let rows = list_q.fetch_all(pool).await?;

    let items = rows
        .into_iter()
        .filter_map(|(id, title, cover, total_volumes, read_volumes, publish_status)| {
            Uuid::parse_str(&id).ok().map(|id| SeriesListRow {
                id,
                title,
                latest_cover_url: cover,
                total_volumes,
                read_volumes,
                publish_status: crate::models::normalize_publish_status(&publish_status)
                    .unwrap_or("ongoing")
                    .to_string(),
            })
        })
        .collect();

    Ok((items, total.0))
}

pub async fn search_series(pool: &SqlitePool, query: &str) -> AppResult<Vec<(Uuid, String, Option<String>, i64)>> {
    let pattern = format!("%{}%", query);
    let exact = query.to_string();
    let prefix = format!("{query}%");

    let rows = sqlx::query_as::<_, (String, String, Option<String>, i64)>(&format!(
        r#"
        SELECT
            s.id,
            s.title,
            {SERIES_DISPLAY_COVER_SQL} AS latest_cover_url,
            CASE
                WHEN s.title = ? THEN 100
                WHEN s.title LIKE ? ESCAPE '\' THEN 80
                ELSE 50
            END AS score
        FROM series s
        WHERE s.title LIKE ? ESCAPE '\'
        ORDER BY score DESC, s.title COLLATE NOCASE ASC
        LIMIT 50
        "#
    ))
    .bind(&exact)
    .bind(&prefix)
    .bind(&pattern)
    .fetch_all(pool)
    .await?;

    let mut results = Vec::new();
    for (id, title, cover, score) in rows {
        results.push((parse_uuid(&id)?, title, cover, score));
    }

    Ok(results)
}

pub async fn list_tierlist_entries(
    pool: &SqlitePool,
    user_id: Uuid,
) -> AppResult<Vec<(String, Uuid, String, Option<String>, i64)>> {
    // Tierlist covers: prefer latest *read* volume, then series cover, then latest volume.
    let rows = sqlx::query_as::<_, (String, String, String, Option<String>, i64)>(
        r#"
        SELECT ute.tier, s.id, s.title,
            COALESCE(
                (
                    SELECT v.cover_url
                    FROM user_volume_reads uvr
                    JOIN volumes v ON v.id = uvr.volume_id
                    WHERE uvr.user_id = ? AND v.series_id = s.id AND uvr.is_read = 1
                      AND v.cover_url IS NOT NULL
                      AND TRIM(v.cover_url) != ''
                    ORDER BY v.volume_number DESC, v.published_at DESC
                    LIMIT 1
                ),
                NULLIF(TRIM(s.cover_url), ''),
                (
                    SELECT v.cover_url
                    FROM volumes v
                    WHERE v.series_id = s.id
                      AND v.cover_url IS NOT NULL
                      AND TRIM(v.cover_url) != ''
                    ORDER BY v.volume_number DESC, v.published_at DESC
                    LIMIT 1
                )
            ),
            ute.position
        FROM user_tierlist_entries ute
        JOIN series s ON s.id = ute.series_id
        WHERE ute.user_id = ?
        ORDER BY ute.tier ASC, ute.position ASC
        "#,
    )
    .bind(user_id.to_string())
    .bind(user_id.to_string())
    .fetch_all(pool)
    .await?;

    let mut result = Vec::new();
    for (tier, id, title, cover, position) in rows {
        if let Ok(series_id) = Uuid::parse_str(&id) {
            result.push((tier, series_id, title, cover, position));
        }
    }
    Ok(result)
}

pub async fn list_rated_series(
    pool: &SqlitePool,
    user_id: Uuid,
) -> AppResult<Vec<(Uuid, Rating)>> {
    let rows = sqlx::query_as::<_, (String, String)>(
        r#"
        SELECT series_id, rating
        FROM user_series_ratings
        WHERE user_id = ? AND rating != 'None'
        "#,
    )
    .bind(user_id.to_string())
    .fetch_all(pool)
    .await?;

    let mut result = Vec::new();
    for (series_id, rating) in rows {
        if let Ok(id) = Uuid::parse_str(&series_id) {
            if let Ok(rating) = rating.parse::<Rating>() {
                result.push((id, rating));
            }
        }
    }
    Ok(result)
}

/// 읽은 권이 있고 티어리스트(S~F)에 없는 시리즈 — UI의 None 영역용.
pub async fn list_none_tier_candidates(
    pool: &SqlitePool,
    user_id: Uuid,
) -> AppResult<Vec<(Uuid, String, Option<String>, i64)>> {
    // Same cover priority as list_tierlist_entries: latest read → series → latest volume.
    let rows = sqlx::query_as::<_, (String, String, Option<String>)>(
        r#"
        SELECT s.id, s.title,
            COALESCE(
                (
                    SELECT v.cover_url
                    FROM user_volume_reads uvr
                    JOIN volumes v ON v.id = uvr.volume_id
                    WHERE uvr.user_id = ? AND v.series_id = s.id AND uvr.is_read = 1
                      AND v.cover_url IS NOT NULL
                      AND TRIM(v.cover_url) != ''
                    ORDER BY v.volume_number DESC, v.published_at DESC
                    LIMIT 1
                ),
                NULLIF(TRIM(s.cover_url), ''),
                (
                    SELECT v.cover_url
                    FROM volumes v
                    WHERE v.series_id = s.id
                      AND v.cover_url IS NOT NULL
                      AND TRIM(v.cover_url) != ''
                    ORDER BY v.volume_number DESC, v.published_at DESC
                    LIMIT 1
                )
            )
        FROM series s
        WHERE EXISTS (
            SELECT 1
            FROM user_volume_reads uvr
            JOIN volumes v ON v.id = uvr.volume_id
            WHERE uvr.user_id = ? AND v.series_id = s.id AND uvr.is_read = 1
        )
        AND NOT EXISTS (
            SELECT 1
            FROM user_tierlist_entries ute
            WHERE ute.user_id = ? AND ute.series_id = s.id
        )
        ORDER BY s.title COLLATE NOCASE ASC
        "#,
    )
    .bind(user_id.to_string())
    .bind(user_id.to_string())
    .bind(user_id.to_string())
    .fetch_all(pool)
    .await?;

    let mut result = Vec::new();
    for (position, (id, title, cover)) in rows.into_iter().enumerate() {
        if let Ok(series_id) = Uuid::parse_str(&id) {
            result.push((series_id, title, cover, position as i64));
        }
    }
    Ok(result)
}

pub async fn replace_tierlist(
    pool: &SqlitePool,
    user_id: Uuid,
    tiers: &[(String, Vec<Uuid>)],
) -> AppResult<()> {
    let mut tx = pool.begin().await?;

    let previous: Vec<String> = sqlx::query_scalar(
        "SELECT series_id FROM user_tierlist_entries WHERE user_id = ?",
    )
    .bind(user_id.to_string())
    .fetch_all(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM user_tierlist_entries WHERE user_id = ?")
        .bind(user_id.to_string())
        .execute(&mut *tx)
        .await?;

    let mut kept = std::collections::HashSet::new();

    for (tier, series_ids) in tiers {
        for (position, series_id) in series_ids.iter().enumerate() {
            kept.insert(series_id.to_string());

            sqlx::query(
                r#"
                INSERT INTO user_tierlist_entries (user_id, series_id, tier, position, updated_at)
                VALUES (?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
                "#,
            )
            .bind(user_id.to_string())
            .bind(series_id.to_string())
            .bind(tier)
            .bind(position as i64)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                r#"
                INSERT INTO user_series_ratings (user_id, series_id, rating, updated_at)
                VALUES (?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
                ON CONFLICT(user_id, series_id) DO UPDATE SET
                    rating = excluded.rating,
                    updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                "#,
            )
            .bind(user_id.to_string())
            .bind(series_id.to_string())
            .bind(tier)
            .execute(&mut *tx)
            .await?;
        }
    }

    for series_id in previous {
        if kept.contains(&series_id) {
            continue;
        }
        sqlx::query(
            r#"
            INSERT INTO user_series_ratings (user_id, series_id, rating, updated_at)
            VALUES (?, ?, 'None', strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            ON CONFLICT(user_id, series_id) DO UPDATE SET
                rating = 'None',
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
            "#,
        )
        .bind(user_id.to_string())
        .bind(series_id)
        .execute(&mut *tx)
        .await?;
    }

    // Drop gatekeeper refs that no longer sit in the referenced tier.
    sqlx::query(
        r#"
        DELETE FROM user_tierlist_gatekeepers
        WHERE user_id = ?
          AND (
            series_id NOT IN (
              SELECT series_id FROM user_tierlist_entries WHERE user_id = ?
            )
            OR NOT EXISTS (
              SELECT 1 FROM user_tierlist_entries ute
              WHERE ute.user_id = user_tierlist_gatekeepers.user_id
                AND ute.series_id = user_tierlist_gatekeepers.series_id
                AND ute.tier = user_tierlist_gatekeepers.tier
            )
          )
        "#,
    )
    .bind(user_id.to_string())
    .bind(user_id.to_string())
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn list_tierlist_gatekeepers(
    pool: &SqlitePool,
    user_id: Uuid,
) -> AppResult<Vec<(String, String, Uuid)>> {
    let rows = sqlx::query_as::<_, (String, String, String)>(
        r#"
        SELECT tier, side, series_id
        FROM user_tierlist_gatekeepers
        WHERE user_id = ?
        "#,
    )
    .bind(user_id.to_string())
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|(tier, side, series_id)| Ok((tier, side, parse_uuid(&series_id)?)))
        .collect()
}

pub async fn replace_tierlist_gatekeepers(
    pool: &SqlitePool,
    user_id: Uuid,
    rows: &[(String, String, Uuid)],
) -> AppResult<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM user_tierlist_gatekeepers WHERE user_id = ?")
        .bind(user_id.to_string())
        .execute(&mut *tx)
        .await?;

    for (tier, side, series_id) in rows {
        sqlx::query(
            r#"
            INSERT INTO user_tierlist_gatekeepers (user_id, tier, side, series_id, updated_at)
            VALUES (?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            "#,
        )
        .bind(user_id.to_string())
        .bind(tier)
        .bind(side)
        .bind(series_id.to_string())
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

pub async fn merge_series_tierlist_gatekeepers(
    pool: &SqlitePool,
    source_series_id: Uuid,
    target_series_id: Uuid,
) -> AppResult<()> {
    // Drop source rows that would collide with an existing target gatekeeper slot.
    sqlx::query(
        r#"
        DELETE FROM user_tierlist_gatekeepers
        WHERE series_id = ?
          AND EXISTS (
            SELECT 1 FROM user_tierlist_gatekeepers g2
            WHERE g2.user_id = user_tierlist_gatekeepers.user_id
              AND g2.tier = user_tierlist_gatekeepers.tier
              AND g2.side = user_tierlist_gatekeepers.side
              AND g2.series_id = ?
          )
        "#,
    )
    .bind(source_series_id.to_string())
    .bind(target_series_id.to_string())
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        UPDATE user_tierlist_gatekeepers SET series_id = ?
        WHERE series_id = ?
        "#,
    )
    .bind(target_series_id.to_string())
    .bind(source_series_id.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn insert_tierlist_entries(
    pool: &SqlitePool,
    user_id: Uuid,
    entries: &[(Uuid, String, i64)],
) -> AppResult<()> {
    for (series_id, tier, position) in entries {
        sqlx::query(
            r#"
            INSERT INTO user_tierlist_entries (user_id, series_id, tier, position, updated_at)
            VALUES (?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            "#,
        )
        .bind(user_id.to_string())
        .bind(series_id.to_string())
        .bind(tier)
        .bind(position)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn has_tierlist_entries(pool: &SqlitePool, user_id: Uuid) -> AppResult<bool> {
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM user_tierlist_entries WHERE user_id = ?")
            .bind(user_id.to_string())
            .fetch_one(pool)
            .await?;
    Ok(count.0 > 0)
}

pub mod users {
    pub use super::{
        count_admins, create_user, delete_user, ensure_admin_by_email, find_by_email, find_by_id,
        list_users, refresh_unverified_registration, set_verify_token, verify_email_by_token,
    };
}

pub async fn touch_series_refreshed(pool: &SqlitePool, series_id: Uuid) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE series SET last_refreshed_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE id = ?
        "#,
    )
    .bind(series_id.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

/// Aladin series ordered by oldest refresh first.
pub async fn list_aladin_series_for_refresh(
    pool: &SqlitePool,
) -> AppResult<Vec<(Uuid, String, String)>> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        r#"
        SELECT id, title, aladin_series_id
        FROM series
        WHERE aladin_series_id NOT LIKE 'manual:%'
          AND COALESCE(publish_status, 'ongoing') IN ('ongoing', 'complete_partial')
        ORDER BY
            CASE WHEN last_refreshed_at IS NULL OR last_refreshed_at = '' THEN 0 ELSE 1 END,
            last_refreshed_at ASC,
            title COLLATE NOCASE ASC
        "#,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|(id, title, aladin_series_id)| Ok((parse_uuid(&id)?, title, aladin_series_id)))
        .collect()
}

pub async fn get_aladin_quota_used(pool: &SqlitePool, usage_date: &str) -> AppResult<i64> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT query_count FROM aladin_api_usage WHERE usage_date = ?")
            .bind(usage_date)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(c,)| c).unwrap_or(0))
}

pub async fn list_recent_aladin_quota(
    pool: &SqlitePool,
    limit: i64,
) -> AppResult<Vec<(String, i64)>> {
    let rows = sqlx::query_as::<_, (String, i64)>(
        r#"
        SELECT usage_date, query_count
        FROM aladin_api_usage
        ORDER BY usage_date DESC
        LIMIT ?
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Atomically reserve `n` queries for today. Returns Ok(true) if reserved, Ok(false) if over soft limit.
pub async fn try_consume_aladin_quota(
    pool: &SqlitePool,
    usage_date: &str,
    n: i64,
    soft_limit: i64,
) -> AppResult<bool> {
    if n <= 0 {
        return Ok(true);
    }
    let mut tx = pool.begin().await?;
    sqlx::query(
        r#"
        INSERT INTO aladin_api_usage (usage_date, query_count)
        VALUES (?, 0)
        ON CONFLICT(usage_date) DO NOTHING
        "#,
    )
    .bind(usage_date)
    .execute(&mut *tx)
    .await?;

    let used: (i64,) =
        sqlx::query_as("SELECT query_count FROM aladin_api_usage WHERE usage_date = ?")
            .bind(usage_date)
            .fetch_one(&mut *tx)
            .await?;

    if used.0 + n > soft_limit {
        tx.rollback().await?;
        return Ok(false);
    }

    sqlx::query(
        "UPDATE aladin_api_usage SET query_count = query_count + ? WHERE usage_date = ?",
    )
    .bind(n)
    .bind(usage_date)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
}

pub async fn set_app_meta(pool: &SqlitePool, key: &str, value: &str) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO app_meta (key, value) VALUES (?, ?)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value
        "#,
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_app_meta(pool: &SqlitePool, key: &str) -> AppResult<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM app_meta WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(v,)| v))
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CatalogRequestRow {
    pub id: String,
    pub user_id: String,
    pub request_type: String,
    pub series_id: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub publisher: Option<String>,
    pub aladin_series_id: Option<String>,
    pub note: Option<String>,
    pub status: String,
    pub admin_note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub user_email: Option<String>,
    pub series_title: Option<String>,
}

pub async fn insert_catalog_request(
    pool: &SqlitePool,
    id: Uuid,
    user_id: Uuid,
    request_type: &str,
    series_id: Option<Uuid>,
    title: Option<&str>,
    author: Option<&str>,
    publisher: Option<&str>,
    aladin_series_id: Option<&str>,
    note: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO catalog_requests (
            id, user_id, request_type, series_id, title, author, publisher,
            aladin_series_id, note, status
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending')
        "#,
    )
    .bind(id.to_string())
    .bind(user_id.to_string())
    .bind(request_type)
    .bind(series_id.map(|id| id.to_string()))
    .bind(title)
    .bind(author)
    .bind(publisher)
    .bind(aladin_series_id)
    .bind(note)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_catalog_requests_for_user(
    pool: &SqlitePool,
    user_id: Uuid,
) -> AppResult<Vec<CatalogRequestRow>> {
    let rows = sqlx::query_as::<_, CatalogRequestRow>(
        r#"
        SELECT r.id, r.user_id, r.request_type, r.series_id, r.title, r.author, r.publisher,
               r.aladin_series_id, r.note, r.status, r.admin_note, r.created_at, r.updated_at,
               u.email AS user_email, s.title AS series_title
        FROM catalog_requests r
        JOIN users u ON u.id = r.user_id
        LEFT JOIN series s ON s.id = r.series_id
        WHERE r.user_id = ?
        ORDER BY r.created_at DESC
        "#,
    )
    .bind(user_id.to_string())
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn list_catalog_requests_admin(
    pool: &SqlitePool,
    status: Option<&str>,
) -> AppResult<Vec<CatalogRequestRow>> {
    let rows = if let Some(status) = status.filter(|s| !s.is_empty() && *s != "all") {
        sqlx::query_as::<_, CatalogRequestRow>(
            r#"
            SELECT r.id, r.user_id, r.request_type, r.series_id, r.title, r.author, r.publisher,
                   r.aladin_series_id, r.note, r.status, r.admin_note, r.created_at, r.updated_at,
                   u.email AS user_email, s.title AS series_title
            FROM catalog_requests r
            JOIN users u ON u.id = r.user_id
            LEFT JOIN series s ON s.id = r.series_id
            WHERE r.status = ?
            ORDER BY r.created_at ASC
            "#,
        )
        .bind(status)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, CatalogRequestRow>(
            r#"
            SELECT r.id, r.user_id, r.request_type, r.series_id, r.title, r.author, r.publisher,
                   r.aladin_series_id, r.note, r.status, r.admin_note, r.created_at, r.updated_at,
                   u.email AS user_email, s.title AS series_title
            FROM catalog_requests r
            JOIN users u ON u.id = r.user_id
            LEFT JOIN series s ON s.id = r.series_id
            ORDER BY
                CASE r.status WHEN 'pending' THEN 0 WHEN 'approved' THEN 1 ELSE 2 END,
                r.created_at ASC
            "#,
        )
        .fetch_all(pool)
        .await?
    };
    Ok(rows)
}

pub async fn find_catalog_request(
    pool: &SqlitePool,
    id: Uuid,
) -> AppResult<Option<CatalogRequestRow>> {
    let row = sqlx::query_as::<_, CatalogRequestRow>(
        r#"
        SELECT r.id, r.user_id, r.request_type, r.series_id, r.title, r.author, r.publisher,
               r.aladin_series_id, r.note, r.status, r.admin_note, r.created_at, r.updated_at,
               u.email AS user_email, s.title AS series_title
        FROM catalog_requests r
        JOIN users u ON u.id = r.user_id
        LEFT JOIN series s ON s.id = r.series_id
        WHERE r.id = ?
        "#,
    )
    .bind(id.to_string())
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn update_catalog_request_status(
    pool: &SqlitePool,
    id: Uuid,
    status: &str,
    admin_note: Option<&str>,
) -> AppResult<()> {
    let result = sqlx::query(
        r#"
        UPDATE catalog_requests SET
            status = ?,
            admin_note = ?,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
        WHERE id = ?
        "#,
    )
    .bind(status)
    .bind(admin_note)
    .bind(id.to_string())
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("request not found".into()));
    }
    Ok(())
}
