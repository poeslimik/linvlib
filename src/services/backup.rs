use std::path::PathBuf;
use std::time::Duration as StdDuration;

use chrono::{DateTime, FixedOffset, NaiveDate, Utc};
use regex::Regex;
use tokio::fs;
use tokio::time::sleep;

use crate::{
    error::{AppError, AppResult},
    repositories,
    services::quota,
    state::AppState,
};

const LAST_BACKUP_DATE_KEY: &str = "last_backup_date";
const BACKUP_NAME_RE: &str = r"^linvlib-\d{8}-\d{6}\.db$";

#[derive(Debug, Clone, serde::Serialize)]
pub struct BackupInfo {
    pub name: String,
    pub size_bytes: u64,
    pub modified_at: String,
}

fn seoul_tz() -> FixedOffset {
    FixedOffset::east_opt(9 * 3600).expect("kst")
}

pub fn seoul_now() -> DateTime<FixedOffset> {
    Utc::now().with_timezone(&seoul_tz())
}

pub fn backup_dir(state: &AppState) -> PathBuf {
    PathBuf::from(&state.config.backup_dir)
}

pub fn is_safe_backup_name(name: &str) -> bool {
    Regex::new(BACKUP_NAME_RE)
        .expect("backup name regex")
        .is_match(name)
}

pub fn backup_path(state: &AppState, name: &str) -> AppResult<PathBuf> {
    if !is_safe_backup_name(name) {
        return Err(AppError::BadRequest("invalid backup name".into()));
    }
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(AppError::BadRequest("invalid backup name".into()));
    }
    Ok(backup_dir(state).join(name))
}

pub async fn ensure_backup_dir(state: &AppState) -> AppResult<()> {
    fs::create_dir_all(backup_dir(state))
        .await
        .map_err(|e| AppError::Internal(format!("create backup dir: {e}")))?;
    Ok(())
}

pub async fn list_backups(state: &AppState) -> AppResult<Vec<BackupInfo>> {
    ensure_backup_dir(state).await?;
    let mut entries = fs::read_dir(backup_dir(state))
        .await
        .map_err(|e| AppError::Internal(format!("read backup dir: {e}")))?;
    let mut out = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| AppError::Internal(format!("read backup entry: {e}")))?
    {
        let name = entry.file_name().to_string_lossy().to_string();
        if !is_safe_backup_name(&name) {
            continue;
        }
        let meta = entry
            .metadata()
            .await
            .map_err(|e| AppError::Internal(format!("backup metadata: {e}")))?;
        if !meta.is_file() {
            continue;
        }
        let modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let modified_at: DateTime<Utc> = modified.into();
        out.push(BackupInfo {
            name,
            size_bytes: meta.len(),
            modified_at: modified_at
                .with_timezone(&seoul_tz())
                .format("%Y-%m-%d %H:%M:%S KST")
                .to_string(),
        });
    }
    out.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(out)
}

/// Online-consistent SQLite snapshot via `VACUUM INTO` (no service stop).
pub async fn create_backup(state: &AppState) -> AppResult<BackupInfo> {
    ensure_backup_dir(state).await?;
    let stamp = seoul_now().format("%Y%m%d-%H%M%S");
    let name = format!("linvlib-{stamp}.db");
    let dest = backup_dir(state).join(&name);
    let dest_str = dest
        .to_str()
        .ok_or_else(|| AppError::Internal("backup path is not utf-8".into()))?
        .replace('\\', "/");

    // VACUUM INTO needs a path SQLite can write; use absolute when possible.
    let abs = if dest.is_absolute() {
        dest_str.clone()
    } else {
        std::env::current_dir()
            .map_err(|e| AppError::Internal(format!("cwd: {e}")))?
            .join(&dest)
            .to_str()
            .ok_or_else(|| AppError::Internal("backup abs path is not utf-8".into()))?
            .replace('\\', "/")
    };

    sqlx::query(&format!("VACUUM INTO '{}'", abs.replace('\'', "''")))
        .execute(&state.pool)
        .await
        .map_err(|e| AppError::Internal(format!("VACUUM INTO failed: {e}")))?;

    let meta = fs::metadata(&dest)
        .await
        .map_err(|e| AppError::Internal(format!("backup missing after create: {e}")))?;

    let _ = repositories::set_app_meta(
        &state.pool,
        "last_backup_at",
        &seoul_now().format("%Y-%m-%dT%H:%M:%S%z").to_string(),
    )
    .await;

    prune_old_backups(state).await?;

    Ok(BackupInfo {
        name,
        size_bytes: meta.len(),
        modified_at: seoul_now().format("%Y-%m-%d %H:%M:%S KST").to_string(),
    })
}

pub async fn delete_backup(state: &AppState, name: &str) -> AppResult<()> {
    let path = backup_path(state, name)?;
    if !path.exists() {
        return Err(AppError::NotFound(format!("backup {name}")));
    }
    fs::remove_file(&path)
        .await
        .map_err(|e| AppError::Internal(format!("delete backup: {e}")))?;
    Ok(())
}

pub async fn prune_old_backups(state: &AppState) -> AppResult<usize> {
    ensure_backup_dir(state).await?;
    let retain_days = state.config.backup_retain_days.max(1) as i64;
    let cutoff = seoul_now() - chrono::Duration::days(retain_days);
    let list = list_backups(state).await?;
    let mut removed = 0usize;
    for item in list {
        // Prefer date embedded in filename (KST stamp).
        let too_old = item
            .name
            .strip_prefix("linvlib-")
            .and_then(|rest| rest.strip_suffix(".db"))
            .and_then(|stamp| {
                let (d, _t) = stamp.split_once('-')?;
                NaiveDate::parse_from_str(d, "%Y%m%d").ok()
            })
            .map(|d| d < cutoff.date_naive())
            .unwrap_or(false);
        if too_old {
            if delete_backup(state, &item.name).await.is_ok() {
                removed += 1;
            }
        }
    }
    if removed > 0 {
        tracing::info!(removed, retain_days, "pruned old DB backups");
    }
    Ok(removed)
}

/// Daily KST midnight backup; retains `backup_retain_days` (default 14).
pub fn spawn_daily_backup(state: AppState) {
    tokio::spawn(async move {
        let mut last_run_date = load_last_backup_date(&state).await;
        tracing::info!(
            today = %quota::seoul_today(),
            server_time = %quota::seoul_now_display(),
            last_backup_date = ?last_run_date.map(|d| d.to_string()),
            retain_days = state.config.backup_retain_days,
            dir = %state.config.backup_dir,
            "DB backup scheduler started (KST midnight, checks every 60s)"
        );

        loop {
            sleep(StdDuration::from_secs(60)).await;
            let today = quota::seoul_today_date();
            let due = match last_run_date {
                Some(prev) => prev < today,
                None => true,
            };
            if !due {
                continue;
            }

            // Catch up later the same calendar day if the process was down at 00:00.
            tracing::info!(
                today = %today,
                server_time = %quota::seoul_now_display(),
                "starting scheduled DB backup"
            );

            match create_backup(&state).await {
                Ok(info) => {
                    last_run_date = Some(today);
                    let _ = repositories::set_app_meta(
                        &state.pool,
                        LAST_BACKUP_DATE_KEY,
                        &today.format("%Y-%m-%d").to_string(),
                    )
                    .await;
                    tracing::info!(
                        name = %info.name,
                        size_bytes = info.size_bytes,
                        "scheduled DB backup finished"
                    );
                }
                Err(err) => {
                    tracing::error!(
                        error = %err,
                        server_time = %quota::seoul_now_display(),
                        "scheduled DB backup failed; will retry"
                    );
                }
            }
        }
    });
}

async fn load_last_backup_date(state: &AppState) -> Option<NaiveDate> {
    match repositories::get_app_meta(&state.pool, LAST_BACKUP_DATE_KEY).await {
        Ok(Some(raw)) => NaiveDate::parse_from_str(&raw, "%Y-%m-%d").ok(),
        Ok(None) => {
            // First boot: mark today so we wait until the next KST midnight
            // instead of backing up immediately on every deploy.
            let today = quota::seoul_today_date();
            let _ = repositories::set_app_meta(
                &state.pool,
                LAST_BACKUP_DATE_KEY,
                &today.format("%Y-%m-%d").to_string(),
            )
            .await;
            Some(today)
        }
        Err(_) => Some(quota::seoul_today_date()),
    }
}
