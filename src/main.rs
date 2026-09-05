use linvlib::{
    config::Config, routes, services::backup, services::email, services::scheduler,
    services::search_keys, services::status_report, state::AppState,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::net::SocketAddr;
use std::str::FromStr;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,linvlib=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::from_env()?;
    let options = SqliteConnectOptions::from_str(&config.database_url)?.create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    sqlx::query("PRAGMA foreign_keys = ON;")
        .execute(&pool)
        .await?;
    sqlx::query("PRAGMA journal_mode = WAL;")
        .execute(&pool)
        .await?;

    sqlx::migrate!().run(&pool).await?;

    // Clear stale "running" flag left by an unclean shutdown.
    let _ = linvlib::repositories::set_app_meta(&pool, "refresh_running", "0").await;

    linvlib::repositories::users::ensure_admin_by_email(&pool, &config.admin_email).await?;

    let state = AppState::new(pool, config.clone());
    match search_keys::rebuild_all_auto_aliases(&state).await {
        Ok(n) => tracing::info!(series = n, "rebuilt auto search aliases"),
        Err(err) => tracing::warn!(error = %err, "failed to rebuild auto search aliases"),
    }
    scheduler::spawn_scheduled_refresh(state.clone());
    backup::spawn_daily_backup(state.clone());
    status_report::spawn_daily_status_report(state.clone());
    email::spawn_unverified_user_cleanup(state.clone());
    let app = routes::create_router(state);

    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    tracing::info!("linvlib server listening on {}", config.bind_addr);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
