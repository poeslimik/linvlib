use chrono::{Duration, Utc};
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, header::ContentType},
    transport::smtp::authentication::Credentials,
};
use rand::{Rng, distributions::Alphanumeric};

use crate::{
    error::{AppError, AppResult},
    models::{ForgotPasswordResponse, RegisterResponse, User},
    repositories::users,
    state::AppState,
};

pub const LEGAL_VERSION: &str = "2026-07-25";
const RESET_TOKEN_HOURS: i64 = 2;

pub fn new_verify_token() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(48)
        .map(char::from)
        .collect()
}

pub fn verify_expiry_iso() -> String {
    (Utc::now() + Duration::hours(48))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string()
}

pub fn reset_expiry_iso() -> String {
    (Utc::now() + Duration::hours(RESET_TOKEN_HOURS))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string()
}

pub async fn register_user(
    state: &AppState,
    email: &str,
    password: &str,
    accept_terms: bool,
    accept_privacy: bool,
) -> AppResult<RegisterResponse> {
    if !accept_terms || !accept_privacy {
        return Err(AppError::BadRequest(
            "약관 및 개인정보 처리방침에 동의해 주세요".into(),
        ));
    }

    let email = email.trim().to_lowercase();
    if email.is_empty() || password.len() < 8 {
        return Err(AppError::BadRequest(
            "email is required and password must be at least 8 characters".into(),
        ));
    }

    let is_admin = state.config.is_admin_email(&email);
    let email_verified = is_admin;
    let accepted_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let password_hash = crate::auth::hash_password(password)?;

    // Unverified account: refresh credentials/consent and resend verification instead of conflict.
    if let Some(existing) = users::find_by_email(&state.pool, &email).await? {
        if existing.email_verified {
            return Err(AppError::Conflict("이미 가입된 이메일입니다".into()));
        }

        if is_admin {
            let user = users::refresh_unverified_registration(
                &state.pool,
                existing.id,
                &password_hash,
                true,
                true,
                None,
                None,
                Some(&accepted_at),
                Some(&accepted_at),
                Some(LEGAL_VERSION),
            )
            .await?;
            let access_token = crate::auth::create_token(
                user.id,
                &state.config.jwt_secret,
                state.config.jwt_expiry_hours,
            )?;
            return Ok(RegisterResponse {
                message: "관리자 계정이 생성되었습니다.".into(),
                access_token: Some(access_token),
                token_type: "Bearer",
                verification_token: None,
            });
        }

        let verify_token = new_verify_token();
        let expires = verify_expiry_iso();
        let user = users::refresh_unverified_registration(
            &state.pool,
            existing.id,
            &password_hash,
            false,
            false,
            Some(&verify_token),
            Some(&expires),
            Some(&accepted_at),
            Some(&accepted_at),
            Some(LEGAL_VERSION),
        )
        .await?;
        send_verification_email(state, &user, &verify_token).await?;

        return Ok(RegisterResponse {
            message: "이메일 인증이 완료되지 않은 계정입니다. 인증 메일을 다시 보냈습니다.".into(),
            access_token: None,
            token_type: "Bearer",
            verification_token: if state.config.email_dev_mode {
                Some(verify_token)
            } else {
                None
            },
        });
    }

    let token = if email_verified {
        None
    } else {
        Some(new_verify_token())
    };
    let expires = token.as_ref().map(|_| verify_expiry_iso());

    let user = users::create_user(
        &state.pool,
        &email,
        &password_hash,
        is_admin,
        email_verified,
        token.as_deref(),
        expires.as_deref(),
        Some(&accepted_at),
        Some(&accepted_at),
        Some(LEGAL_VERSION),
    )
    .await?;

    if is_admin {
        let access_token = crate::auth::create_token(
            user.id,
            &state.config.jwt_secret,
            state.config.jwt_expiry_hours,
        )?;
        return Ok(RegisterResponse {
            message: "관리자 계정이 생성되었습니다.".into(),
            access_token: Some(access_token),
            token_type: "Bearer",
            verification_token: None,
        });
    }

    let verify_token = token.expect("token for unverified user");
    send_verification_email(state, &user, &verify_token).await?;

    Ok(RegisterResponse {
        message: "가입되었습니다. 이메일로 보낸 인증 링크를 확인해 주세요.".into(),
        access_token: None,
        token_type: "Bearer",
        verification_token: if state.config.email_dev_mode {
            Some(verify_token)
        } else {
            None
        },
    })
}

pub async fn resend_verification(state: &AppState, email: &str) -> AppResult<RegisterResponse> {
    let email = email.trim().to_lowercase();
    let user = users::find_by_email(&state.pool, &email)
        .await?
        .ok_or_else(|| AppError::NotFound("user not found".into()))?;

    if user.email_verified {
        return Ok(RegisterResponse {
            message: "이미 인증된 계정입니다. 로그인하세요.".into(),
            access_token: None,
            token_type: "Bearer",
            verification_token: None,
        });
    }

    let token = new_verify_token();
    let expires = verify_expiry_iso();
    users::set_verify_token(&state.pool, user.id, &token, &expires).await?;
    send_verification_email(state, &user, &token).await?;

    Ok(RegisterResponse {
        message: "인증 메일을 다시 보냈습니다.".into(),
        access_token: None,
        token_type: "Bearer",
        verification_token: if state.config.email_dev_mode {
            Some(token)
        } else {
            None
        },
    })
}

pub async fn verify_email(state: &AppState, token: &str) -> AppResult<(User, String)> {
    let token = token.trim();
    if token.is_empty() {
        return Err(AppError::BadRequest("token is required".into()));
    }
    let user = users::verify_email_by_token(&state.pool, token)
        .await?
        .ok_or_else(|| AppError::BadRequest("유효하지 않거나 만료된 인증 링크입니다".into()))?;
    let access_token = crate::auth::create_token(
        user.id,
        &state.config.jwt_secret,
        state.config.jwt_expiry_hours,
    )?;
    Ok((user, access_token))
}

/// Always returns the same message so callers cannot probe whether an email exists.
pub async fn request_password_reset(
    state: &AppState,
    email: &str,
) -> AppResult<ForgotPasswordResponse> {
    let email = email.trim().to_lowercase();
    let generic = ForgotPasswordResponse {
        message: "가입된 이메일이면 비밀번호 재설정 안내를 보냈습니다. 메일함을 확인해 주세요."
            .into(),
        reset_token: None,
    };
    if email.is_empty() {
        return Err(AppError::BadRequest("email is required".into()));
    }

    let Some(user) = users::find_by_email(&state.pool, &email).await? else {
        return Ok(generic);
    };
    if !user.email_verified {
        // Unverified accounts should use the verification flow, not password reset.
        return Ok(generic);
    }

    let token = new_verify_token();
    let expires = reset_expiry_iso();
    users::set_reset_token(&state.pool, user.id, &token, &expires).await?;
    send_password_reset_email(state, &user, &token).await?;

    Ok(ForgotPasswordResponse {
        message: generic.message,
        reset_token: if state.config.email_dev_mode {
            Some(token)
        } else {
            None
        },
    })
}

pub async fn reset_password(
    state: &AppState,
    token: &str,
    password: &str,
) -> AppResult<(User, String)> {
    let token = token.trim();
    if token.is_empty() {
        return Err(AppError::BadRequest("token is required".into()));
    }
    if password.len() < 8 {
        return Err(AppError::BadRequest(
            "password must be at least 8 characters".into(),
        ));
    }

    let password_hash = crate::auth::hash_password(password)?;
    let user = users::reset_password_by_token(&state.pool, token, &password_hash)
        .await?
        .ok_or_else(|| {
            AppError::BadRequest("유효하지 않거나 만료된 재설정 링크입니다".into())
        })?;
    let access_token = crate::auth::create_token(
        user.id,
        &state.config.jwt_secret,
        state.config.jwt_expiry_hours,
    )?;
    Ok((user, access_token))
}

async fn send_verification_email(state: &AppState, user: &User, token: &str) -> AppResult<()> {
    let link = format!(
        "{}/verify?token={}",
        state.config.app_base_url,
        urlencoding_encode(token)
    );
    send_plain_email(
        state,
        user,
        "[linvlib] 이메일 인증",
        &format!(
            "linvlib 가입을 환영합니다.\n\n아래 링크를 열어 이메일을 인증해 주세요 (48시간 유효):\n{link}\n"
        ),
        "verification",
        &link,
    )
    .await
}

async fn send_password_reset_email(state: &AppState, user: &User, token: &str) -> AppResult<()> {
    let link = format!(
        "{}/reset-password?token={}",
        state.config.app_base_url,
        urlencoding_encode(token)
    );
    send_plain_email(
        state,
        user,
        "[linvlib] 비밀번호 재설정",
        &format!(
            "비밀번호 재설정을 요청하셨습니다.\n\n아래 링크에서 새 비밀번호를 설정해 주세요 ({RESET_TOKEN_HOURS}시간 유효):\n{link}\n\n요청하지 않았다면 이 메일을 무시하세요.\n"
        ),
        "password reset",
        &link,
    )
    .await
}

async fn send_plain_email(
    state: &AppState,
    user: &User,
    subject: &str,
    body: &str,
    kind: &str,
    link: &str,
) -> AppResult<()> {
    if state.config.smtp_host.is_none() {
        tracing::warn!(
            email = %user.email,
            %link,
            kind,
            "SMTP not configured; auth link logged (EMAIL_DEV_MODE)"
        );
        return Ok(());
    }

    let from = state
        .config
        .smtp_from
        .clone()
        .or_else(|| state.config.smtp_username.clone())
        .ok_or_else(|| AppError::Internal("SMTP_FROM or SMTP_USERNAME required".into()))?;

    let email = Message::builder()
        .from(
            from.parse::<Mailbox>()
                .map_err(|e| AppError::Internal(format!("invalid SMTP_FROM: {e}")))?,
        )
        .to(user
            .email
            .parse::<Mailbox>()
            .map_err(|e| AppError::Internal(format!("invalid recipient: {e}")))?)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string())
        .map_err(|e| AppError::Internal(format!("email build error: {e}")))?;

    let host = state.config.smtp_host.as_deref().unwrap();
    let mut builder = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
        .map_err(|e| AppError::Internal(format!("smtp relay: {e}")))?
        .port(state.config.smtp_port);

    if let (Some(user), Some(pass)) = (
        state.config.smtp_username.as_deref(),
        state.config.smtp_password.as_deref(),
    ) {
        builder = builder.credentials(Credentials::new(user.to_string(), pass.to_string()));
    }

    let mailer = builder.build();
    mailer
        .send(email)
        .await
        .map_err(|e| AppError::Internal(format!("smtp send failed: {e}")))?;
    Ok(())
}

fn urlencoding_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
