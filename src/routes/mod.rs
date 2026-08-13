use axum::{
    Router,
    http::{HeaderValue, header},
    routing::{delete, get, post, put},
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

use crate::{handlers, state::AppState};

pub fn create_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let api = Router::new()
        .route("/auth/register", post(handlers::auth::register))
        .route("/auth/login", post(handlers::auth::login))
        .route("/auth/verify", post(handlers::auth::verify_email))
        .route(
            "/auth/resend-verification",
            post(handlers::auth::resend_verification),
        )
        .route("/auth/forgot-password", post(handlers::auth::forgot_password))
        .route("/auth/reset-password", post(handlers::auth::reset_password))
        .route("/auth/me", get(handlers::auth::me).delete(handlers::auth::delete_account))
        .route(
            "/series",
            get(handlers::series::list_series).post(handlers::series::create_manual_series),
        )
        .route(
            "/series/{id}",
            get(handlers::series::get_series)
                .put(handlers::series::update_manual_series)
                .delete(handlers::series::delete_manual_series),
        )
        .route("/series/{id}/reads", put(handlers::series::save_reads))
        .route("/series/{id}/rating", put(handlers::series::save_rating))
        .route(
            "/series/{id}/publish-status",
            put(handlers::series::set_series_publish_status),
        )
        .route(
            "/series/{id}/aliases",
            post(handlers::series::add_search_alias),
        )
        .route(
            "/series/{id}/aliases/{alias_id}",
            delete(handlers::series::delete_search_alias),
        )
        .route(
            "/series/{id}/volumes/order",
            put(handlers::series::reorder_volumes),
        )
        .route("/search", get(handlers::search::search))
        .route("/imports/search", get(handlers::series::import_search))
        .route("/imports", post(handlers::series::import_series))
        .route(
            "/catalog-requests",
            get(handlers::catalog::list_my_requests).post(handlers::catalog::create_request),
        )
        .route(
            "/admin/catalog-requests",
            get(handlers::catalog::list_admin_requests),
        )
        .route(
            "/admin/catalog-requests/{id}",
            put(handlers::catalog::review_request),
        )
        .route("/admin/status", get(handlers::admin::status))
        .route("/admin/users", get(handlers::admin::list_users))
        .route(
            "/admin/manual-series",
            get(handlers::admin::list_manual_series),
        )
        .route("/admin/refresh", post(handlers::admin::trigger_refresh))
        .route(
            "/admin/new-releases",
            get(handlers::admin::list_new_release_suggestions),
        )
        .route(
            "/admin/new-releases/{id}/import",
            post(handlers::admin::import_new_release_suggestion),
        )
        .route(
            "/admin/new-releases/{id}",
            delete(handlers::admin::dismiss_new_release_suggestion),
        )
        .route(
            "/admin/search-aliases",
            post(handlers::admin::batch_search_aliases),
        )
        .route(
            "/admin/search-bundles",
            post(handlers::admin::create_search_bundle),
        )
        .route("/admin/backups", get(handlers::admin::list_backups).post(handlers::admin::create_backup))
        .route(
            "/admin/backups/{name}",
            delete(handlers::admin::delete_backup),
        )
        .route(
            "/admin/backups/{name}/download",
            get(handlers::admin::download_backup),
        )
        .route("/admin/volumes/move", post(handlers::admin::move_volumes))
        .route("/admin/series/merge", post(handlers::admin::merge_series))
        .route("/admin/volumes/split", post(handlers::admin::split_volumes))
        .route("/tierlist", get(handlers::tierlist::get_tierlist))
        .route("/tierlist", put(handlers::tierlist::save_tierlist))
        .route("/media/proxy", get(handlers::media::proxy_image));

    let static_files = ServeDir::new("static")
        .not_found_service(ServeFile::new("static/index.html"));

    Router::new()
        .route("/health", get(handlers::health::health))
        .nest("/api/v1", api)
        .fallback_service(static_files)
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-cache, must-revalidate"),
        ))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
