use std::time::Duration as StdDuration;

use chrono::NaiveDate;
use serde_json::json;
use tokio::time::sleep;

use crate::{
    error::{AppError, AppResult},
    repositories,
    services::{backup, quota},
    state::AppState,
};

const LAST_STATUS_REPORT_DATE_KEY: &str = "last_status_report_date";
const SEND_ATTEMPTS: u32 = 3;
const SEND_RETRY_DELAY: StdDuration = StdDuration::from_secs(30);

/// Log whether the daily report is armed. The report itself runs after the
/// scheduled new-release refresh, not on its own clock.
pub fn log_startup(state: &AppState) {
    if state.config.discord_status_webhook_url.is_none() {
        tracing::info!("Discord status report disabled (DISCORD_STATUS_WEBHOOK_URL unset)");
        return;
    }
    tracing::info!("Discord status report armed (after scheduled new-release refresh)");
}

/// Post today's Discord status once the scheduled new-release refresh has finished.
///
/// No-op when `DISCORD_STATUS_WEBHOOK_URL` is unset or today's report already
/// succeeded. A few retries cover a transient webhook failure; the success date
/// is stored so a later restart does not send again.
pub async fn send_after_scheduled_refresh(state: &AppState) {
    if state.config.discord_status_webhook_url.is_none() {
        tracing::info!("Discord status report skipped (DISCORD_STATUS_WEBHOOK_URL unset)");
        return;
    }

    let today = quota::seoul_today_date();
    if load_last_run_date(state).await == Some(today) {
        tracing::info!(
            today = %today,
            "Discord status report already sent today; skipping"
        );
        return;
    }

    tracing::info!(
        today = %today,
        server_time = %quota::seoul_now_display(),
        "sending Discord status report after new-release refresh"
    );

    for attempt in 1..=SEND_ATTEMPTS {
        match send_status_report(state).await {
            Ok(()) => {
                let _ = repositories::set_app_meta(
                    &state.pool,
                    LAST_STATUS_REPORT_DATE_KEY,
                    &today.format("%Y-%m-%d").to_string(),
                )
                .await;
                tracing::info!("Discord status report sent");
                return;
            }
            Err(err) => {
                tracing::error!(
                    error = %err,
                    attempt,
                    server_time = %quota::seoul_now_display(),
                    "Discord status report failed"
                );
                if attempt < SEND_ATTEMPTS {
                    sleep(SEND_RETRY_DELAY).await;
                }
            }
        }
    }
}

pub async fn send_status_report(state: &AppState) -> AppResult<()> {
    let webhook = state
        .config
        .discord_status_webhook_url
        .as_deref()
        .ok_or_else(|| AppError::Internal("DISCORD_STATUS_WEBHOOK_URL unset".into()))?;

    let pending = repositories::list_catalog_requests_admin(&state.pool, Some("pending"))
        .await?
        .len() as i64;
    let users = repositories::list_users(&state.pool).await?;
    let manual_series_count = repositories::count_manual_series(&state.pool).await?;
    let backups = backup::list_backups(state).await.unwrap_or_default();
    let last_refresh_at = repositories::get_app_meta(&state.pool, "last_refresh_at")
        .await?
        .unwrap_or_else(|| "—".into());
    let last_refresh_note = repositories::get_app_meta(&state.pool, "last_refresh_note")
        .await?
        .unwrap_or_else(|| "—".into());
    let last_backup_at = repositories::get_app_meta(&state.pool, "last_backup_at")
        .await?
        .unwrap_or_else(|| "—".into());
    let last_scheduled_refresh = repositories::get_app_meta(
        &state.pool,
        "last_scheduled_refresh_date",
    )
    .await?
    .unwrap_or_else(|| "—".into());
    let yes24_q = crate::services::yes24_limit::status_today(state).await?;

    let color = if pending > 0 {
        0xd4893a_u32 // amber when requests waiting
    } else {
        0x1f7a74_u32 // teal OK
    };

    let body = json!({
        "username": "linvlib",
        "embeds": [{
            "title": "일일 서버 상태",
            "description": format!("서버 시각 **{}**", quota::seoul_now_display()),
            "color": color,
            "fields": [
                {
                    "name": "갱신",
                    "value": format!(
                        "마지막: `{at}`\n스케줄 일자: `{sched}`\n메모: {note}",
                        at = last_refresh_at,
                        sched = last_scheduled_refresh,
                        note = truncate(&last_refresh_note, 180)
                    ),
                    "inline": false
                },
                {
                    "name": "예스24 API",
                    "value": format!(
                        "오늘({date}) **{used}** / {soft} (한도 {hard})\n초당 제한: {rps}회",
                        date = yes24_q.usage_date,
                        used = yes24_q.used,
                        soft = yes24_q.soft_limit,
                        hard = yes24_q.hard_limit,
                        rps = crate::services::yes24_limit::RPS_LIMIT
                    ),
                    "inline": true
                },
                {
                    "name": "백업",
                    "value": format!(
                        "마지막: `{at}`\n보관: **{count}**개 (최대 {days}일)",
                        at = last_backup_at,
                        count = backups.len(),
                        days = state.config.backup_retain_days
                    ),
                    "inline": true
                },
                {
                    "name": "카탈로그",
                    "value": format!(
                        "대기 요청 **{pending}**\n사용자 **{users}**\n직접 등록 **{manual}**",
                        pending = pending,
                        users = users.len(),
                        manual = manual_series_count
                    ),
                    "inline": true
                }
            ],
            "footer": { "text": state.config.app_base_url.clone() }
        }]
    });

    let response = state
        .http
        .post(webhook)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "Discord webhook HTTP {status}: {}",
            truncate(&text, 300)
        )));
    }

    Ok(())
}

async fn load_last_run_date(state: &AppState) -> Option<NaiveDate> {
    match repositories::get_app_meta(&state.pool, LAST_STATUS_REPORT_DATE_KEY).await {
        Ok(Some(raw)) => match NaiveDate::parse_from_str(&raw, "%Y-%m-%d") {
            Ok(date) => Some(date),
            Err(_) => {
                tracing::warn!(
                    value = %raw,
                    "invalid last_status_report_date; treating as unset"
                );
                None
            }
        },
        Ok(None) => None,
        Err(err) => {
            tracing::warn!(
                error = %err,
                "failed to load last_status_report_date; treating as unset"
            );
            None
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{cut}…")
}
