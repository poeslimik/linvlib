use std::net::IpAddr;

use axum::{
    extract::{Query, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    response::IntoResponse,
};
use serde::Deserialize;

use crate::{
    error::{AppError, AppResult},
    state::AppState,
};

const MAX_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, Deserialize)]
pub struct ProxyQuery {
    url: String,
}

/// Same-origin image proxy so html-to-image can embed covers without CORS failures.
pub async fn proxy_image(
    State(state): State<AppState>,
    Query(query): Query<ProxyQuery>,
) -> AppResult<impl IntoResponse> {
    let url = validate_image_url(&query.url)?;

    let response = state
        .http
        .get(url)
        .header(
            header::USER_AGENT,
            "linvlib/0.1 (+https://linvlib.cloud)",
        )
        .header(header::ACCEPT, "image/*,*/*;q=0.8")
        .timeout(std::time::Duration::from_secs(12))
        .send()
        .await
        .map_err(|err| AppError::Internal(format!("image fetch failed: {err}")))?;

    if !response.status().is_success() {
        return Err(AppError::NotFound("image not found".into()));
    }

    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    if !is_allowed_content_type(&content_type) {
        return Err(AppError::BadRequest("url is not an image".into()));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|err| AppError::Internal(format!("image body failed: {err}")))?;

    if bytes.len() > MAX_BYTES {
        return Err(AppError::BadRequest("image too large".into()));
    }

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type.split(';').next().unwrap_or("image/jpeg"))
            .unwrap_or_else(|_| HeaderValue::from_static("image/jpeg")),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=86400"),
    );
    headers.insert(
        HeaderName::from_static("access-control-allow-origin"),
        HeaderValue::from_static("*"),
    );

    Ok((StatusCode::OK, headers, bytes))
}

fn validate_image_url(raw: &str) -> AppResult<reqwest::Url> {
    let url = reqwest::Url::parse(raw.trim())
        .map_err(|_| AppError::BadRequest("invalid image url".into()))?;

    if url.scheme() != "https" && url.scheme() != "http" {
        return Err(AppError::BadRequest("image url must be http(s)".into()));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(AppError::BadRequest("image url must not include credentials".into()));
    }

    let host = url
        .host_str()
        .ok_or_else(|| AppError::BadRequest("image url missing host".into()))?;

    if is_blocked_host(host) {
        return Err(AppError::BadRequest("image host is not allowed".into()));
    }

    Ok(url)
}

fn is_blocked_host(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return true;
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return !is_public_ip(ip);
    }
    false
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.octets()[0] == 0)
        }
        IpAddr::V6(v6) => !(v6.is_loopback() || v6.is_unspecified() || v6.is_unique_local()),
    }
}

fn is_allowed_content_type(content_type: &str) -> bool {
    let ct = content_type.to_ascii_lowercase();
    ct.starts_with("image/") || ct.starts_with("application/octet-stream")
}
