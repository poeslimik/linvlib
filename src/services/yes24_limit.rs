//! Yes24 Open API call limits (Basic plan: 10 RPS / 20_000 per KST day).

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tokio::time::sleep;

use crate::{
    error::{AppError, AppResult},
    repositories,
    services::quota,
    state::AppState,
};

/// Official Basic plan daily cap (account-scoped, resets 00:00 KST).
pub const DAILY_LIMIT: i64 = 20_000;
/// Official Basic plan per-second cap.
pub const RPS_LIMIT: usize = 10;

/// Soft daily reserve below the hard cap so bursts don't hit the provider wall.
const DAILY_SOFT_LIMIT: i64 = 19_500;

#[derive(Debug, Default)]
pub struct Yes24Limiter {
    /// Timestamps of recent acquires (sliding 1s window).
    recent: Mutex<VecDeque<Instant>>,
}

impl Yes24Limiter {
    /// Wait for an RPS slot, then reserve one daily quota unit.
    pub async fn acquire(&self, state: &AppState) -> AppResult<()> {
        self.wait_for_rps_slot().await;

        let today = quota::seoul_today();
        let ok = repositories::try_consume_yes24_quota(
            &state.pool,
            &today,
            1,
            DAILY_SOFT_LIMIT,
        )
        .await?;
        if !ok {
            let used = repositories::get_yes24_quota_used(&state.pool, &today)
                .await
                .unwrap_or(DAILY_SOFT_LIMIT);
            return Err(AppError::QuotaExceeded(format!(
                "Yes24 daily soft limit reached ({used}/{DAILY_SOFT_LIMIT}, hard {DAILY_LIMIT})"
            )));
        }
        Ok(())
    }

    async fn wait_for_rps_slot(&self) {
        loop {
            let wait = {
                let mut q = self
                    .recent
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let now = Instant::now();
                while q
                    .front()
                    .is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(1))
                {
                    q.pop_front();
                }
                if q.len() < RPS_LIMIT {
                    q.push_back(now);
                    None
                } else {
                    let oldest = *q.front().expect("non-empty when at RPS limit");
                    Some(
                        Duration::from_secs(1)
                            .saturating_sub(now.duration_since(oldest))
                            + Duration::from_millis(5),
                    )
                }
            };
            match wait {
                None => return,
                Some(d) => sleep(d).await,
            }
        }
    }
}

/// Snapshot for admin UI / Discord.
#[derive(Debug, Clone)]
pub struct Yes24QuotaStatus {
    pub usage_date: String,
    pub used: i64,
    pub soft_limit: i64,
    pub hard_limit: i64,
}

pub async fn status_today(state: &AppState) -> AppResult<Yes24QuotaStatus> {
    let usage_date = quota::seoul_today();
    let used = repositories::get_yes24_quota_used(&state.pool, &usage_date).await?;
    Ok(Yes24QuotaStatus {
        usage_date,
        used,
        soft_limit: DAILY_SOFT_LIMIT,
        hard_limit: DAILY_LIMIT,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rps_window_allows_burst_up_to_limit() {
        let limiter = Yes24Limiter::default();
        for _ in 0..RPS_LIMIT {
            let wait = {
                let mut q = limiter.recent.lock().unwrap();
                let now = Instant::now();
                while q
                    .front()
                    .is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(1))
                {
                    q.pop_front();
                }
                if q.len() < RPS_LIMIT {
                    q.push_back(now);
                    None
                } else {
                    Some(Duration::from_millis(1))
                }
            };
            assert!(wait.is_none());
        }
        let blocked = {
            let q = limiter.recent.lock().unwrap();
            q.len() >= RPS_LIMIT
        };
        assert!(blocked);
    }
}
