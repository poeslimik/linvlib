use axum::{
    body::Body,
    http::{Request, StatusCode, header::AUTHORIZATION},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::str::FromStr;
use tower::ServiceExt;

use linvlib::{config::Config, routes, state::AppState};

fn test_config(database_url: String) -> Config {
    Config {
        database_url,
        jwt_secret: "test-secret-key-for-integration-tests".to_string(),
        jwt_expiry_hours: 24,
        aladin_ttb_key: "test-ttb-key".to_string(),
        bind_addr: "127.0.0.1:0".to_string(),
        title_rules_path: "config/title_rules.toml".to_string(),
        admin_email: "admin@example.com".to_string(),
        app_base_url: "http://localhost:3000".to_string(),
        aladin_daily_quota: 5000,
        aladin_soft_quota: 4800,
        backup_dir: "backups-test".to_string(),
        backup_retain_days: 14,
        discord_status_webhook_url: None,
        email_dev_mode: true,
        smtp_host: None,
        smtp_port: 587,
        smtp_username: None,
        smtp_password: None,
        smtp_from: None,
    }
}

async fn setup_app() -> axum::Router {
    let database_url = "sqlite::memory:".to_string();
    let config = test_config(database_url.clone());

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
                .unwrap()
                .create_if_missing(true),
        )
        .await
        .expect("connect test db");

    sqlx::query("PRAGMA foreign_keys = ON;")
        .execute(&pool)
        .await
        .expect("enable foreign keys");

    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("run migrations");

    let state = AppState::new(pool, config);
    routes::create_router(state)
}

async fn read_json(response: axum::response::Response) -> Value {
    let body = response
        .into_body()
        .collect()
        .await
        .expect("read body")
        .to_bytes();
    serde_json::from_slice(&body).expect("parse json")
}

async fn register_and_verify(app: &axum::Router, email: &str, password: &str) -> String {
    let register = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "email": email,
                        "password": password,
                        "accept_terms": true,
                        "accept_privacy": true
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(register.status(), StatusCode::CREATED);
    let register_json = read_json(register).await;
    if let Some(token) = register_json["access_token"].as_str() {
        return token.to_string();
    }
    let verify_token = register_json["verification_token"]
        .as_str()
        .expect("verification_token in email_dev_mode")
        .to_string();
    let verify = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/verify")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "token": verify_token }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(verify.status(), StatusCode::OK);
    read_json(verify).await["access_token"]
        .as_str()
        .expect("token")
        .to_string()
}

#[tokio::test]
async fn health_check_returns_ok() {
    let app = setup_app().await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json = read_json(response).await;
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
async fn auth_register_login_and_me_flow() {
    let app = setup_app().await;
    let token = register_and_verify(&app, "reader@example.com", "password123").await;

    let me = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/me")
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(me.status(), StatusCode::OK);
    let me_json = read_json(me).await;
    assert_eq!(me_json["email"], "reader@example.com");
    assert_eq!(me_json["email_verified"], true);

    let login = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "email": "reader@example.com",
                        "password": "password123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(login.status(), StatusCode::OK);
}

#[tokio::test]
async fn forgot_and_reset_password_flow() {
    let app = setup_app().await;
    let _token = register_and_verify(&app, "reset-me@example.com", "password123").await;

    let forgot = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/forgot-password")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "email": "reset-me@example.com" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(forgot.status(), StatusCode::OK);
    let forgot_json = read_json(forgot).await;
    let reset_token = forgot_json["reset_token"]
        .as_str()
        .expect("EMAIL_DEV_MODE should return reset_token");

    let unknown = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/forgot-password")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "email": "nobody@example.com" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unknown.status(), StatusCode::OK);
    let unknown_json = read_json(unknown).await;
    assert!(unknown_json["reset_token"].is_null());

    let reset = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/reset-password")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "token": reset_token,
                        "password": "newpassword99"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reset.status(), StatusCode::OK);

    let old_login = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "email": "reset-me@example.com",
                        "password": "password123"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(old_login.status(), StatusCode::UNAUTHORIZED);

    let new_login = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "email": "reset-me@example.com",
                        "password": "newpassword99"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(new_login.status(), StatusCode::OK);
}

#[tokio::test]
async fn register_requires_legal_consent() {
    let app = setup_app().await;
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "email": "noconsent@example.com",
                        "password": "password123",
                        "accept_terms": false,
                        "accept_privacy": true
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = read_json(response).await;
    assert!(
        json["error"]
            .as_str()
            .unwrap_or("")
            .contains("동의해 주세요"),
        "unexpected error: {json}"
    );
}

#[tokio::test]
async fn register_unverified_again_resends_verification() {
    let app = setup_app().await;
    let body = json!({
        "email": "pending@example.com",
        "password": "password123",
        "accept_terms": true,
        "accept_privacy": true
    });

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::CREATED);
    let first_json = read_json(first).await;
    let first_token = first_json["verification_token"]
        .as_str()
        .expect("first verification_token")
        .to_string();

    let second = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "email": "pending@example.com",
                        "password": "newpassword99",
                        "accept_terms": true,
                        "accept_privacy": true
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::CREATED);
    let second_json = read_json(second).await;
    assert!(
        second_json["message"]
            .as_str()
            .unwrap_or("")
            .contains("인증 메일을 다시 보냈습니다"),
        "unexpected message: {second_json}"
    );
    let second_token = second_json["verification_token"]
        .as_str()
        .expect("second verification_token")
        .to_string();
    assert_ne!(first_token, second_token);

    // Old token should no longer work; new password + new token should.
    let stale = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/verify")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "token": first_token }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::BAD_REQUEST);

    let verify = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/verify")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "token": second_token }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(verify.status(), StatusCode::OK);

    let login = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "email": "pending@example.com",
                        "password": "newpassword99"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(login.status(), StatusCode::OK);
}

#[tokio::test]
async fn series_progress_rating_and_tierlist_flow() {
    let series_id = uuid::Uuid::new_v4();
    let volume_id = uuid::Uuid::new_v4();

    let database_url = "sqlite::memory:".to_string();

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
                .unwrap()
                .create_if_missing(true),
        )
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys = ON;")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();

    sqlx::query(
        "INSERT INTO series (id, title, author, publisher, aladin_series_id, latest_published_at)
         VALUES (?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))",
    )
    .bind(series_id.to_string())
    .bind("테스트 라노벨")
    .bind("작가")
    .bind("출판사")
    .bind("series:test")
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO volumes (id, series_id, volume_number, title, cover_url, published_at, aladin_item_id)
         VALUES (?, ?, ?, ?, ?, date('now'), ?)",
    )
    .bind(volume_id.to_string())
    .bind(series_id.to_string())
    .bind(1)
    .bind("테스트 라노벨 1권")
    .bind("https://example.com/cover.jpg")
    .bind("item:1")
    .execute(&pool)
    .await
    .unwrap();

    let config = test_config(database_url.clone());
    let app = routes::create_router(AppState::new(pool, config));

    let token = register_and_verify(&app, "fixture-user@example.com", "password123").await;

    let detail = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/series/{series_id}"))
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(detail.status(), StatusCode::OK);

    let save_reads = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/series/{series_id}/reads"))
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "reads": [{ "volume_id": volume_id, "is_read": true }]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(save_reads.status(), StatusCode::OK);

    let save_rating = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/v1/series/{series_id}/rating"))
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "rating": "A" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(save_rating.status(), StatusCode::OK);

    let search = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/search?q=테스트")
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(search.status(), StatusCode::OK);
    let search_json = read_json(search).await;
    assert!(search_json.as_array().unwrap().len() >= 1);

    let tierlist = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/tierlist")
                .header(AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tierlist.status(), StatusCode::OK);
    let tierlist_json = read_json(tierlist).await;
    let a_tier = tierlist_json["tiers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["tier"] == "A")
        .expect("A tier");
    assert_eq!(a_tier["entries"][0]["series_id"], series_id.to_string());
}
