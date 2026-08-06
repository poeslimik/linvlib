use std::time::Duration as StdDuration;

use chrono::{NaiveDate, Timelike};
use tokio::time::sleep;

use crate::{
    repositories,
    services::{aladin, quota},
    state::AppState,
};

const LAST_SCHEDULED_REFRESH_DATE_KEY: &str = "last_scheduled_refresh_date";
/// KST wall-clock time when the daily scheduled refresh starts.
const RUN_HOUR: u32 = 23;
const RUN_MINUTE: u32 = 30;

/// Poll every minute and run Aladin refresh once per KST day at/after 23:30,
/// consuming whatever soft quota remains that day. Stops at KST midnight so
/// the next day's quota is not spent.
///
/// Last successful run date is persisted in `app_meta` so daytime restarts do not
/// re-trigger. If the process was down at 23:30 but comes back the same evening
/// (still ≥ 23:30), it catches up once.
pub fn spawn_midnight_refresh(state: AppState) {
    tokio::spawn(async move {
        let mut last_run_date = load_last_run_date(&state).await;
        tracing::info!(
            today = %quota::seoul_today(),
            server_time = %quota::seoul_now_display(),
            last_scheduled_refresh_date = ?last_run_date.map(|d| d.to_string()),
            run_at = %format!("{RUN_HOUR:02}:{RUN_MINUTE:02} KST"),
            "Aladin scheduled refresh started (checks every 60s)"
        );

        loop {
            sleep(StdDuration::from_secs(60)).await;

            let (today, past_run_time) = seoul_today_and_past_run_time();
            let already_ran_today = last_run_date == Some(today);
            if !past_run_time || already_ran_today {
                continue;
            }

            let quota_remaining = quota::remaining_soft_quota(&state).await.ok();
            tracing::info!(
                today = %today,
                server_time = %quota::seoul_now_display(),
                ?quota_remaining,
                "KST 23:30 window — starting scheduled Aladin refresh (use remaining soft quota until midnight)"
            );

            match aladin::refresh_all_aladin_bounded(&state, Some(today)).await {
                Ok(res) => {
                    last_run_date = Some(today);
                    let _ = repositories::set_app_meta(
                        &state.pool,
                        LAST_SCHEDULED_REFRESH_DATE_KEY,
                        &today.format("%Y-%m-%d").to_string(),
                    )
                    .await;
                    tracing::info!(
                        total = res.total,
                        refreshed = res.refreshed,
                        failed = res.failed,
                        "scheduled Aladin refresh finished"
                    );
                }
                Err(err) => {
                    // Leave last_run_date unchanged so we retry next minute.
                    tracing::error!(
                        error = %err,
                        server_time = %quota::seoul_now_display(),
                        "scheduled Aladin refresh failed; will retry"
                    );
                }
            }
        }
    });
}

fn seoul_today_and_past_run_time() -> (NaiveDate, bool) {
    let now = chrono::Utc::now().with_timezone(&chrono::FixedOffset::east_opt(9 * 3600).expect("kst"));
    let past = now.hour() > RUN_HOUR || (now.hour() == RUN_HOUR && now.minute() >= RUN_MINUTE);
    (now.date_naive(), past)
}

async fn load_last_run_date(state: &AppState) -> Option<NaiveDate> {
    match repositories::get_app_meta(&state.pool, LAST_SCHEDULED_REFRESH_DATE_KEY).await {
        Ok(Some(raw)) => match NaiveDate::parse_from_str(&raw, "%Y-%m-%d") {
            Ok(date) => Some(date),
            Err(_) => {
                tracing::warn!(
                    value = %raw,
                    "invalid last_scheduled_refresh_date; treating as unset"
                );
                None
            }
        },
        Ok(None) => None,
        Err(err) => {
            tracing::warn!(
                error = %err,
                "failed to load last_scheduled_refresh_date; treating as unset"
            );
            None
        }
    }
}
