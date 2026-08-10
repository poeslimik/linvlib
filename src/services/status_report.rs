use std::time::Duration as StdDuration;

use chrono::{NaiveDate, Timelike};
use serde_json::json;
use tokio::time::sleep;

use crate::{
    error::{AppError, AppResult},
    repositories,
    services::{backup, quota},
    state::AppState,
};

const LAST_STATUS_REPORT_DATE_KEY: &str = "last_status_report_date";
/// Daily Discord status report wall-clock time (KST).
const RUN_HOUR: u32 = 8;
const RUN_MINUTE: u32 = 0;

/// Poll every minute and post a Discord status embed once per KST day at/after 08:00.
///
/// No-op when `DISCORD_STATUS_WEBHOOK_URL` is unset. Last successful send date is
/// persisted so daytime restarts do not re-send; catch-up runs if the process was
/// down at 08:00 but comes back the same day.
pub fn spawn_daily_status_report(state: AppState) {
    if state.config.discord_status_webhook_url.is_none() {
        tracing::info!("Discord status report disabled (DISCORD_STATUS_WEBHOOK_URL unset)");
        return;
    }

    tokio::spawn(async move {
        let mut last_run_date = load_last_run_date(&state).await;
        tracing::info!(
            today = %quota::seoul_today(),
            server_time = %quota::seoul_now_display(),
            last_status_report_date = ?last_run_date.map(|d| d.to_string()),
            run_at = %format!("{RUN_HOUR:02}:{RUN_MINUTE:02} KST"),
            "Discord daily status report started (checks every 60s)"
        );

        loop {
            sleep(StdDuration::from_secs(60)).await;

            let (today, past_run_time) = seoul_today_and_past_run_time();
            if !past_run_time || last_run_date == Some(today) {
                continue;
            }

            tracing::info!(
                today = %today,
                server_time = %quota::seoul_now_display(),
                "sending Discord daily status report"
            );

            match send_status_report(&state).await {
                Ok(()) => {
                    last_run_date = Some(today);
                    let _ = repositories::set_app_meta(
                        &state.pool,
                        LAST_STATUS_REPORT_DATE_KEY,
                        &today.format("%Y-%m-%d").to_string(),
                    )
                    .await;
                    tracing::info!("Discord daily status report sent");
                }
                Err(err) => {
                    tracing::error!(
                        error = %err,
                        server_time = %quota::seoul_now_display(),
                        "Discord daily status report failed; will retry"
                    );
                }
            }
        }
    });
}

pub async fn send_status_report(state: &AppState) -> AppResult<()> {
    let webhook = state
        .config
        .discord_status_webhook_url
        .as_deref()
        .ok_or_else(|| AppError::Internal("DISCORD_STATUS_WEBHOOK_URL unset".into()))?;

    let (quota_date, quota_used, quota_soft, _quota_hard) = quota::usage_snapshot(state).await?;
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

    let remaining = (quota_soft - quota_used).max(0);
    let color = if remaining <= 200 {
        0xc45c48_u32 // red-ish when quota nearly exhausted
    } else if pending > 0 {
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
                    "name": "알라딘 쿼터",
                    "value": format!(
                        "{used} / {soft} 사용 · 잔여 **{remaining}**\n(날짜 `{date}`)",
                        used = quota_used,
                        soft = quota_soft,
                        remaining = remaining,
                        date = quota_date
                    ),
                    "inline": false
                },
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

fn seoul_today_and_past_run_time() -> (NaiveDate, bool) {
    let now = chrono::Utc::now()
        .with_timezone(&chrono::FixedOffset::east_opt(9 * 3600).expect("kst"));
    let past = now.hour() > RUN_HOUR || (now.hour() == RUN_HOUR && now.minute() >= RUN_MINUTE);
    (now.date_naive(), past)
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
