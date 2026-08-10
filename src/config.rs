use std::env;

use crate::error::AppError;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub jwt_expiry_hours: i64,
    pub aladin_ttb_key: String,
    pub bind_addr: String,
    /// Soft-coded title/series rules (default: `config/title_rules.toml`).
    pub title_rules_path: String,
    /// Bootstrap admin email (always admin + auto-verified on register).
    pub admin_email: String,
    /// Public base URL for verification links (e.g. https://linvlib.example.com).
    pub app_base_url: String,
    /// Daily Aladin API query budget (hard cap ~5000).
    pub aladin_daily_quota: i64,
    /// Soft stop threshold; leave headroom under the hard cap.
    pub aladin_soft_quota: i64,
    /// Directory for SQLite snapshots (relative to process CWD unless absolute).
    pub backup_dir: String,
    /// How many days of backup files to keep.
    pub backup_retain_days: i64,
    /// Discord Incoming Webhook URL for daily status reports (optional).
    pub discord_status_webhook_url: Option<String>,
    /// When true (or SMTP unset), register/resend may return verification_token.
    pub email_dev_mode: bool,
    pub smtp_host: Option<String>,
    pub smtp_port: u16,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    pub smtp_from: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self, AppError> {
        dotenvy::dotenv().ok();

        let smtp_host = env::var("SMTP_HOST").ok().filter(|s| !s.trim().is_empty());
        let email_dev_mode = env::var("EMAIL_DEV_MODE")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(smtp_host.is_none());

        Ok(Self {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite:linvlib.db".to_string()),
            jwt_secret: env::var("JWT_SECRET")
                .map_err(|_| AppError::Internal("JWT_SECRET is not set".into()))?,
            jwt_expiry_hours: env::var("JWT_EXPIRY_HOURS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(168),
            aladin_ttb_key: env::var("ALADIN_TTB_KEY")
                .map_err(|_| AppError::Internal("ALADIN_TTB_KEY is not set".into()))?,
            bind_addr: env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_string()),
            title_rules_path: env::var("TITLE_RULES_PATH")
                .unwrap_or_else(|_| "config/title_rules.toml".to_string()),
            admin_email: env::var("ADMIN_EMAIL")
                .unwrap_or_default()
                .trim()
                .to_lowercase(),
            app_base_url: env::var("APP_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:3000".to_string())
                .trim_end_matches('/')
                .to_string(),
            aladin_daily_quota: env::var("ALADIN_DAILY_QUOTA")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5000),
            aladin_soft_quota: env::var("ALADIN_SOFT_QUOTA")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(4800),
            backup_dir: env::var("BACKUP_DIR").unwrap_or_else(|_| "backups".to_string()),
            backup_retain_days: env::var("BACKUP_RETAIN_DAYS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(14),
            discord_status_webhook_url: env::var("DISCORD_STATUS_WEBHOOK_URL")
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            email_dev_mode,
            smtp_host,
            smtp_port: env::var("SMTP_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(587),
            smtp_username: env::var("SMTP_USERNAME").ok().filter(|s| !s.is_empty()),
            smtp_password: env::var("SMTP_PASSWORD").ok().filter(|s| !s.is_empty()),
            smtp_from: env::var("SMTP_FROM").ok().filter(|s| !s.is_empty()),
        })
    }

    pub fn is_admin_email(&self, email: &str) -> bool {
        !self.admin_email.is_empty() && email.trim().eq_ignore_ascii_case(&self.admin_email)
    }
}
