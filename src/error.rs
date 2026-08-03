use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("title already exists")]
    TitleConflict { series_id: Uuid, title: String },
    #[error("email not verified")]
    EmailNotVerified,
    #[error("quota exceeded: {0}")]
    QuotaExceeded(String),
    #[error("rate limited")]
    RateLimited { retry_after_secs: u64 },
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Jwt(#[from] jsonwebtoken::errors::Error),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error("password error")]
    PasswordHash,
    #[error("internal error: {0}")]
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".to_string()),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            AppError::TitleConflict { title, .. } => (
                StatusCode::CONFLICT,
                format!("‘{title}’과(와) 같은 제목의 작품이 이미 있습니다"),
            ),
            AppError::EmailNotVerified => (
                StatusCode::FORBIDDEN,
                "이메일 인증이 필요합니다".to_string(),
            ),
            AppError::QuotaExceeded(msg) => (StatusCode::TOO_MANY_REQUESTS, msg.clone()),
            AppError::RateLimited { .. } => (
                StatusCode::TOO_MANY_REQUESTS,
                "요청이 너무 많습니다. 잠시 후 다시 시도해 주세요".to_string(),
            ),
            AppError::Sqlx(err) => {
                tracing::error!(error = %err, "database error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "database error".to_string(),
                )
            }
            AppError::Jwt(_) => (StatusCode::UNAUTHORIZED, "invalid token".to_string()),
            AppError::Reqwest(err) => {
                tracing::error!(error = %err, "external api error");
                (
                    StatusCode::BAD_GATEWAY,
                    "external api error".to_string(),
                )
            }
            AppError::PasswordHash => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "password error".to_string(),
            ),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
        };

        let body = match &self {
            AppError::TitleConflict { series_id, .. } => {
                json!({ "error": message, "existing_series_id": series_id })
            }
            AppError::RateLimited { retry_after_secs } => {
                json!({ "error": message, "retry_after_secs": retry_after_secs })
            }
            _ => json!({ "error": message }),
        };

        let mut response = (status, Json(body)).into_response();
        if let AppError::RateLimited { retry_after_secs } = &self {
            if let Ok(value) = axum::http::HeaderValue::from_str(&retry_after_secs.to_string()) {
                response
                    .headers_mut()
                    .insert(axum::http::header::RETRY_AFTER, value);
            }
        }
        response
    }
}

pub type AppResult<T> = Result<T, AppError>;
