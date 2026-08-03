use chrono::{FixedOffset, NaiveDate, Utc};

use crate::{
    error::{AppError, AppResult},
    repositories,
    state::AppState,
};

fn seoul_offset() -> FixedOffset {
    FixedOffset::east_opt(9 * 3600).expect("kst offset")
}

/// Asia/Seoul calendar date (UTC+9) as YYYY-MM-DD.
pub fn seoul_today() -> String {
    seoul_today_date().format("%Y-%m-%d").to_string()
}

pub fn seoul_today_date() -> NaiveDate {
    Utc::now().with_timezone(&seoul_offset()).date_naive()
}

pub fn seoul_now_display() -> String {
    Utc::now()
        .with_timezone(&seoul_offset())
        .format("%Y-%m-%d %H:%M:%S KST")
        .to_string()
}

pub async fn remaining_soft_quota(state: &AppState) -> AppResult<i64> {
    let day = seoul_today();
    let used = repositories::get_aladin_quota_used(&state.pool, &day).await?;
    Ok((state.config.aladin_soft_quota - used).max(0))
}

pub async fn consume_one(state: &AppState) -> AppResult<()> {
    let day = seoul_today();
    let ok = repositories::try_consume_aladin_quota(
        &state.pool,
        &day,
        1,
        state.config.aladin_soft_quota,
    )
    .await?;
    if !ok {
        return Err(AppError::QuotaExceeded(
            "오늘 알라딘 API 호출 한도에 도달했습니다. 한국 시간 자정 이후 다시 시도해 주세요.".into(),
        ));
    }
    Ok(())
}

pub async fn usage_snapshot(state: &AppState) -> AppResult<(String, i64, i64, i64)> {
    let day = seoul_today();
    let used = repositories::get_aladin_quota_used(&state.pool, &day).await?;
    Ok((
        day,
        used,
        state.config.aladin_soft_quota,
        state.config.aladin_daily_quota,
    ))
}
