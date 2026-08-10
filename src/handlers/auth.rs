use axum::{
    Json,
    extract::State,
    http::StatusCode,
};

use crate::{
    auth::{self, AuthUser},
    error::{AppError, AppResult},
    models::{
        AuthResponse, DeleteAccountRequest, ForgotPasswordRequest, ForgotPasswordResponse,
        LoginRequest, RegisterRequest, RegisterResponse, ResendVerificationRequest,
        ResetPasswordRequest, UserResponse, VerifyEmailRequest,
    },
    repositories::users,
    services::{email, rate_limit::ClientIp},
    state::AppState,
};

pub async fn register(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<RegisterRequest>,
) -> AppResult<(StatusCode, Json<RegisterResponse>)> {
    state.auth_rate_limiter.check_register(&ip)?;
    let response = email::register_user(
        &state,
        &body.email,
        &body.password,
        body.accept_terms,
        body.accept_privacy,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn login(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<LoginRequest>,
) -> AppResult<Json<AuthResponse>> {
    state
        .auth_rate_limiter
        .check_login(&ip, body.email.trim())?;

    let user = users::find_by_email(&state.pool, body.email.trim())
        .await?
        .ok_or(AppError::Unauthorized)?;

    if !auth::verify_password(&body.password, &user.password_hash)? {
        return Err(AppError::Unauthorized);
    }
    if !user.email_verified {
        return Err(AppError::EmailNotVerified);
    }

    let access_token = auth::create_token(
        user.id,
        &state.config.jwt_secret,
        state.config.jwt_expiry_hours,
    )?;

    Ok(Json(AuthResponse {
        access_token,
        token_type: "Bearer",
    }))
}

pub async fn verify_email(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<VerifyEmailRequest>,
) -> AppResult<Json<AuthResponse>> {
    state.auth_rate_limiter.check_verify(&ip)?;
    let (_user, access_token) = email::verify_email(&state, &body.token).await?;
    Ok(Json(AuthResponse {
        access_token,
        token_type: "Bearer",
    }))
}

pub async fn resend_verification(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<ResendVerificationRequest>,
) -> AppResult<Json<RegisterResponse>> {
    state
        .auth_rate_limiter
        .check_resend(&ip, body.email.trim())?;
    let response = email::resend_verification(&state, &body.email).await?;
    Ok(Json(response))
}

pub async fn forgot_password(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<ForgotPasswordRequest>,
) -> AppResult<Json<ForgotPasswordResponse>> {
    state
        .auth_rate_limiter
        .check_forgot(&ip, body.email.trim())?;
    let response = email::request_password_reset(&state, &body.email).await?;
    Ok(Json(response))
}

pub async fn reset_password(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(body): Json<ResetPasswordRequest>,
) -> AppResult<Json<AuthResponse>> {
    state.auth_rate_limiter.check_reset(&ip)?;
    let (_user, access_token) = email::reset_password(&state, &body.token, &body.password).await?;
    Ok(Json(AuthResponse {
        access_token,
        token_type: "Bearer",
    }))
}

pub async fn me(AuthUser(user): AuthUser) -> Json<UserResponse> {
    Json(user.into())
}

pub async fn delete_account(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(body): Json<DeleteAccountRequest>,
) -> AppResult<Json<serde_json::Value>> {
    if body.password.is_empty() {
        return Err(AppError::BadRequest("password is required".into()));
    }
    if !auth::verify_password(&body.password, &user.password_hash)? {
        return Err(AppError::Unauthorized);
    }
    if user.is_admin {
        let admins = users::count_admins(&state.pool).await?;
        if admins <= 1 {
            return Err(AppError::BadRequest(
                "마지막 관리자 계정은 탈퇴할 수 없습니다".into(),
            ));
        }
    }

    users::delete_user(&state.pool, user.id).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
