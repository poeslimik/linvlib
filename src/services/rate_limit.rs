//! In-memory sliding-window rate limits for auth endpoints.

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::ConnectInfo;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

const LOGIN_LIMIT: usize = 20;
const LOGIN_WINDOW: Duration = Duration::from_secs(15 * 60);

const REGISTER_LIMIT: usize = 10;
const REGISTER_WINDOW: Duration = Duration::from_secs(60 * 60);

const RESEND_LIMIT: usize = 5;
const RESEND_WINDOW: Duration = Duration::from_secs(60 * 60);

const VERIFY_LIMIT: usize = 30;
const VERIFY_WINDOW: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Default)]
pub struct AuthRateLimiter {
    inner: Mutex<HashMap<String, VecDeque<Instant>>>,
}

impl AuthRateLimiter {
    pub fn check_login(&self, ip: &str, email: &str) -> AppResult<()> {
        self.check(&format!("login:ip:{ip}"), LOGIN_LIMIT, LOGIN_WINDOW)?;
        let email = email.trim().to_lowercase();
        if !email.is_empty() {
            self.check(&format!("login:email:{email}"), LOGIN_LIMIT, LOGIN_WINDOW)?;
        }
        Ok(())
    }

    pub fn check_register(&self, ip: &str) -> AppResult<()> {
        self.check(&format!("register:ip:{ip}"), REGISTER_LIMIT, REGISTER_WINDOW)
    }

    pub fn check_resend(&self, ip: &str, email: &str) -> AppResult<()> {
        self.check(&format!("resend:ip:{ip}"), RESEND_LIMIT, RESEND_WINDOW)?;
        let email = email.trim().to_lowercase();
        if !email.is_empty() {
            self.check(&format!("resend:email:{email}"), RESEND_LIMIT, RESEND_WINDOW)?;
        }
        Ok(())
    }

    pub fn check_verify(&self, ip: &str) -> AppResult<()> {
        self.check(&format!("verify:ip:{ip}"), VERIFY_LIMIT, VERIFY_WINDOW)
    }

    fn check(&self, key: &str, limit: usize, window: Duration) -> AppResult<()> {
        let now = Instant::now();
        let mut map = self
            .inner
            .lock()
            .map_err(|_| AppError::Internal("rate limiter poisoned".into()))?;

        let entries = map.entry(key.to_string()).or_default();
        while entries.front().is_some_and(|t| now.duration_since(*t) >= window) {
            entries.pop_front();
        }

        if entries.len() >= limit {
            let retry_after_secs = entries
                .front()
                .map(|oldest| {
                    window
                        .saturating_sub(now.duration_since(*oldest))
                        .as_secs()
                        .max(1)
                })
                .unwrap_or(1);
            return Err(AppError::RateLimited { retry_after_secs });
        }

        entries.push_back(now);

        // Opportunistic cleanup of stale keys.
        if map.len() > 4_096 {
            map.retain(|_, q| q.back().is_some_and(|t| now.duration_since(*t) < window));
        }

        Ok(())
    }
}

/// Best-effort client IP (proxy headers, then socket peer).
pub struct ClientIp(pub String);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(ip) = forwarded_ip(parts) {
            return Ok(ClientIp(ip));
        }
        if let Some(ConnectInfo(addr)) = parts.extensions.get::<ConnectInfo<SocketAddr>>() {
            return Ok(ClientIp(addr.ip().to_string()));
        }
        Ok(ClientIp("unknown".into()))
    }
}

fn forwarded_ip(parts: &Parts) -> Option<String> {
    if let Some(value) = parts.headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        let ip = value.trim();
        if !ip.is_empty() {
            return Some(ip.to_string());
        }
    }
    if let Some(value) = parts
        .headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
    {
        // First hop is the original client when set by a trusted reverse proxy.
        let ip = value.split(',').next()?.trim();
        if !ip.is_empty() {
            return Some(ip.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_after_limit() {
        let limiter = AuthRateLimiter::default();
        for _ in 0..LOGIN_LIMIT {
            limiter.check("t:key", LOGIN_LIMIT, LOGIN_WINDOW).unwrap();
        }
        let err = limiter.check("t:key", LOGIN_LIMIT, LOGIN_WINDOW).unwrap_err();
        match err {
            AppError::RateLimited { retry_after_secs } => assert!(retry_after_secs >= 1),
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
