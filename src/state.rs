use std::sync::Arc;

use reqwest::Client;
use sqlx::SqlitePool;

use crate::config::Config;
use crate::services::rate_limit::AuthRateLimiter;
use crate::services::title_rules::TitleRules;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Arc<Config>,
    pub http: Client,
    pub title_rules: Arc<TitleRules>,
    pub auth_rate_limiter: Arc<AuthRateLimiter>,
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
        }
    }
}