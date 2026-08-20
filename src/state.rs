use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use reqwest::Client;
use sqlx::SqlitePool;

use crate::config::Config;
use crate::services::rate_limit::AuthRateLimiter;
use crate::services::title_rules::TitleRules;

/// Prevents overlapping Aladin new-release refresh jobs (manual + scheduled).
#[derive(Default)]
pub struct RefreshGate {
    running: AtomicBool,
}

impl RefreshGate {
    pub fn try_begin(&self) -> bool {
        self.running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub fn end(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<Config>,
    pub http: Client,
    pub title_rules: Arc<TitleRules>,
    pub auth_rate_limiter: Arc<AuthRateLimiter>,
    pub refresh_gate: Arc<RefreshGate>,
}

impl AppState {
    pub fn new(pool: SqlitePool, config: Config) -> Self {
        let title_rules = TitleRules::load_or_embedded(&config.title_rules_path);
        Self {
            pool,
            config: Arc::new(config),
            http: Client::new(),
            title_rules: Arc::new(title_rules),
            auth_rate_limiter: Arc::new(AuthRateLimiter::default()),
            refresh_gate: Arc::new(RefreshGate::default()),
        }
    }
}