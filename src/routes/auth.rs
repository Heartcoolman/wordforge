use axum::extract::State;
use axum::http::{header::SET_COOKIE, HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;

use crate::extractors::JsonBody;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::{
    extract_refresh_token_from_headers, generate_dummy_argon2_hash, hash_password, hash_token,
    sign_jwt_for_user, sign_refresh_token_for_user, verify_jwt, verify_password, AuthUser,
};
use crate::response::{created, ok, AppError};
use crate::state::AppState;
use crate::store::operations::sessions::Session;
use crate::store::operations::users::User;
use crate::validation::{is_valid_email, validate_password, validate_username};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
        .route("/forgot-password", post(forgot_password))
        .route("/reset-password", post(reset_password))
        .route("/verify-reset-token", post(verify_reset_token))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterRequest {
    pub email: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgotPasswordRequest {
    pub email: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetPasswordRequest {
    pub token: String,
    pub new_password: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyResetTokenRequest {
    pub token: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    pub id: String,
    pub email: String,
    pub username: String,
    pub is_banned: bool,
}

impl From<&User> for UserProfile {
    fn from(value: &User) -> Self {
        Self {
            id: value.id.clone(),
            email: value.email.clone(),
            username: value.username.clone(),
            is_banned: value.is_banned,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponse {
    pub access_token: String,
    pub user: UserProfile,
}

/// 每用户最大并发会话数
const MAX_SESSIONS_PER_USER: usize = 10;

/// Issue an access + refresh token pair and persist the access session.
async fn issue_token_pair(user_id: &str, state: &AppState) -> Result<(String, String), AppError> {
    let user_id = user_id.to_string();
    let access_token = sign_jwt_for_user(
        &user_id,
        &state.config().jwt_secret,
        state.config().jwt_expires_in_hours,
    )?;

    let refresh_token = sign_refresh_token_for_user(
        &user_id,
        &state.config().refresh_jwt_secret,
        state.config().refresh_token_expires_in_hours,
    )?;

    let access_session = Session {
        token_hash: hash_token(&access_token),
        user_id: user_id.clone(),
        token_type: "user".to_string(),
        created_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(state.config().jwt_expires_in_hours as i64),
        revoked: false,
    };

    let refresh_session = Session {
        token_hash: hash_token(&refresh_token),
        user_id: user_id.clone(),
        token_type: "refresh".to_string(),
        created_at: Utc::now(),
        expires_at: Utc::now()
            + Duration::hours(state.config().refresh_token_expires_in_hours as i64),
        revoked: false,
    };

    let cleanup_user_id = user_id.clone();
    state
        .run_store_task(
            "auth.issue_token_pair",
            move |store| -> Result<(), AppError> {
                if let Err(e) =
                    store.cleanup_oldest_user_sessions(&cleanup_user_id, MAX_SESSIONS_PER_USER)
                {
                    tracing::warn!(user_id = %cleanup_user_id, error = %e, "清理多余会话失败");
                }

                store.create_session(&access_session)?;
                store.create_session(&refresh_session)?;
                Ok(())
            },
        )
        .await??;

    Ok((access_token, refresh_token))
}

async fn register(
    State(state): State<AppState>,
    JsonBody(req): JsonBody<RegisterRequest>,
) -> Result<Response, AppError> {
    let email = req.email.trim().to_lowercase();
    if !is_valid_email(&email) {
        return Err(AppError::bad_request("AUTH_INVALID_EMAIL", "邮箱格式无效"));
    }
    let username = req.username.trim().to_string();
    if let Err(msg) = validate_username(&username) {
        return Err(AppError::bad_request("AUTH_INVALID_USERNAME", msg));
    }
    if let Err(msg) = validate_password(&req.password) {
        return Err(AppError::bad_request("AUTH_WEAK_PASSWORD", msg));
    }

    let password = req.password;
    let email_for_lookup = email.clone();
    let user = state
        .run_store_task(
            "auth.register.create_user",
            move |store| -> Result<User, AppError> {
                let system_settings = store.get_system_settings()?;
                if !system_settings.registration_enabled {
                    return Err(AppError::forbidden("注册功能已关闭"));
                }
                if system_settings.maintenance_mode {
                    return Err(AppError::forbidden("系统正在维护中"));
                }
                if store.get_user_by_email(&email_for_lookup)?.is_some() {
                    return Err(AppError::conflict("AUTH_EMAIL_EXISTS", "该邮箱已被注册"));
                }
                if store.count_users()? >= system_settings.max_users as usize {
                    return Err(AppError::forbidden("用户注册数量已达上限"));
                }
                let password_hash = hash_password(&password)?;
                let now = Utc::now();
                let user = User {
                    id: uuid::Uuid::new_v4().to_string(),
                    email: email_for_lookup,
                    username: username,
                    password_hash,
                    is_banned: false,
                    created_at: now,
                    updated_at: now,
                    failed_login_count: 0,
                    locked_until: None,
                };
                store.create_user(&user)?;
                Ok(user)
            },
        )
        .await??;

    let (access_token, refresh_token) = issue_token_pair(&user.id, &state).await?;

    tracing::info!(
        user_id = %user.id,
        email = %mask_email_for_log(&user.email),
        "用户注册成功"
    );

    let payload = AuthResponse {
        access_token: access_token.clone(),
        user: UserProfile::from(&user),
    };

    let secure = state.config().cookie_secure;
    let mut response = created(payload).into_response();
    set_token_cookie(
        &mut response,
        &access_token,
        state.config().jwt_expires_in_hours * 3600,
        secure,
    )?;
    set_refresh_token_cookie(
        &mut response,
        &refresh_token,
        state.config().refresh_token_expires_in_hours * 3600,
        secure,
    )?;
    Ok(response)
}

async fn login(
    State(state): State<AppState>,
    JsonBody(req): JsonBody<LoginRequest>,
) -> Result<Response, AppError> {
    let email = req.email.trim().to_lowercase();
    let (user, is_locked) = state
        .run_store_task("auth.login.lookup", move |store| -> Result<_, AppError> {
            if store.get_system_settings()?.maintenance_mode {
                return Err(AppError::forbidden("系统正在维护中"));
            }

            let user = store.get_user_by_email(&email)?;
            let is_locked = if let Some(ref user) = user {
                store.is_account_locked(&user.id)?
            } else {
                false
            };
            Ok((user, is_locked))
        })
        .await??;

    // Check ban and lockout status BEFORE password verification to prevent timing attacks
    if let Some(ref u) = user {
        if u.is_banned {
            return Err(AppError::forbidden("用户已被封禁"));
        }
    }
    if is_locked {
        return Err(AppError::too_many_requests(
            "账户因多次登录失败已被临时锁定，请稍后再试",
        ));
    }

    let stored_hash = user
        .as_ref()
        .map(|user| user.password_hash.clone())
        .unwrap_or_else(generate_dummy_argon2_hash);
    let password = req.password;
    let verified = crate::blocking::run_blocking("auth.login.verify_password", move || {
        verify_password(&password, &stored_hash)
    })
    .await??;
    if !verified || user.is_none() {
        if let Some(ref u) = user {
            let user_id = u.id.clone();
            match state
                .run_store_task("auth.login.record_failed_login", move |store| {
                    store.record_failed_login(&user_id)
                })
                .await
            {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => {
                    tracing::warn!(user_id = %u.id, error = %e, "Failed to record login failure")
                }
                Err(e) => tracing::warn!(user_id = %u.id, error = %e, "Login failure task failed"),
            }
        }
        return Err(AppError::unauthorized("邮箱或密码错误"));
    }

    let user = user.unwrap();

    let user_id = user.id.clone();
    match state
        .run_store_task("auth.login.reset_login_attempts", move |store| {
            store.reset_login_attempts(&user_id)
        })
        .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            tracing::warn!(user_id = %user.id, error = %e, "Failed to reset login attempts")
        }
        Err(e) => {
            tracing::warn!(user_id = %user.id, error = %e, "Reset login attempts task failed")
        }
    }

    let (access_token, refresh_token) = issue_token_pair(&user.id, &state).await?;

    tracing::info!(
        user_id = %user.id,
        email = %mask_email_for_log(&user.email),
        "用户登录成功"
    );

    let payload = AuthResponse {
        access_token: access_token.clone(),
        user: UserProfile::from(&user),
    };

    let secure = state.config().cookie_secure;
    let mut response = ok(payload).into_response();
    set_token_cookie(
        &mut response,
        &access_token,
        state.config().jwt_expires_in_hours * 3600,
        secure,
    )?;
    set_refresh_token_cookie(
        &mut response,
        &refresh_token,
        state.config().refresh_token_expires_in_hours * 3600,
        secure,
    )?;
    Ok(response)
}

async fn refresh(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, AppError> {
    // Extract the refresh token from Authorization header or cookie
    let old_token = extract_refresh_token_from_headers(&headers)?;

    // Verify the JWT is valid and has token_type == "refresh"
    let claims = verify_jwt(&old_token, &state.config().refresh_jwt_secret)?;
    if claims.token_type != "refresh" {
        return Err(AppError::unauthorized("令牌类型无效：需要刷新令牌"));
    }

    // Verify the refresh session exists in the store
    let old_hash = hash_token(&old_token);
    let old_hash_for_lookup = old_hash.clone();
    let session = state
        .run_store_task("auth.refresh.get_session", move |store| {
            store.get_session(&old_hash_for_lookup)
        })
        .await??
        .ok_or_else(|| AppError::unauthorized("刷新会话不存在或已过期"))?;

    if session.user_id != claims.sub {
        return Err(AppError::unauthorized("刷新会话不匹配"));
    }

    // 原子性删除旧的 refresh 会话，防止 token 重放攻击
    let old_hash_for_delete = old_hash.clone();
    let was_deleted = state
        .run_store_task("auth.refresh.delete_session", move |store| {
            store.delete_session_if_exists(&old_hash_for_delete)
        })
        .await??;
    if !was_deleted {
        // token 已被使用（可能是重放攻击），拒绝请求
        return Err(AppError::unauthorized("刷新令牌已被使用"));
    }

    // 在签发新 token 前检查用户状态（封禁检查）
    let user = state
        .run_store_task("auth.refresh.get_user", {
            let user_id = claims.sub.clone();
            move |store| store.get_user_by_id(&user_id)
        })
        .await??
        .ok_or_else(|| AppError::unauthorized("用户不存在"))?;

    if user.is_banned {
        return Err(AppError::forbidden("用户已被封禁"));
    }

    // Issue a new token pair
    let (access_token, refresh_token) = issue_token_pair(&claims.sub, &state).await?;

    let secure = state.config().cookie_secure;
    let mut response = ok(AuthResponse {
        access_token: access_token.clone(),
        user: UserProfile::from(&user),
    })
    .into_response();
    set_token_cookie(
        &mut response,
        &access_token,
        state.config().jwt_expires_in_hours * 3600,
        secure,
    )?;
    set_refresh_token_cookie(
        &mut response,
        &refresh_token,
        state.config().refresh_token_expires_in_hours * 3600,
        secure,
    )?;
    Ok(response)
}

async fn logout(auth_user: AuthUser, State(state): State<AppState>) -> Result<Response, AppError> {
    let user_id = auth_user.user_id.clone();
    state
        .run_store_task("auth.logout.delete_user_sessions", move |store| {
            store.delete_user_sessions(&user_id)
        })
        .await??;

    let mut response = ok(serde_json::json!({"loggedOut": true})).into_response();
    clear_auth_cookies(&mut response, state.config().cookie_secure)?;
    Ok(response)
}

async fn forgot_password(
    State(state): State<AppState>,
    JsonBody(req): JsonBody<ForgotPasswordRequest>,
) -> Result<impl IntoResponse, AppError> {
    let email = req.email.trim().to_lowercase();
    if let Some(user) = state
        .run_store_task("auth.forgot_password.get_user_by_email", move |store| {
            store.get_user_by_email(&email)
        })
        .await??
    {
        let raw_token = uuid::Uuid::new_v4().simple().to_string();
        let token_hash = hash_token(&raw_token);
        let expires_at = (Utc::now() + Duration::hours(1)).to_rfc3339();

        state
            .run_store_task("auth.forgot_password.create_token", move |store| {
                store.create_password_reset_token(&token_hash, &user.id, &expires_at)
            })
            .await??;

        // 仅通过日志输出 token，绝不在响应中返回
        tracing::trace!(
            token_prefix = %&raw_token[..8],
            "Password reset token generated (dev diagnostics only)"
        );

        tracing::info!(
            email = %mask_email_for_log(&user.email),
            "Password reset requested; email delivery disabled in trimmed build"
        );
    }

    Ok(ok(serde_json::json!({
        "emailSent": true,
        "message": "如果该邮箱已注册，将会发送密码重置链接",
    })))
}

async fn reset_password(
    State(state): State<AppState>,
    JsonBody(req): JsonBody<ResetPasswordRequest>,
) -> Result<impl IntoResponse, AppError> {
    if let Err(msg) = validate_password(&req.new_password) {
        return Err(AppError::bad_request("AUTH_WEAK_PASSWORD", msg));
    }

    let token_hash = hash_token(&req.token);

    // 原子删除+返回，确保同一 token 只能使用一次
    let entry = state
        .run_store_task("auth.reset_password.take_token", move |store| {
            store.take_password_reset_token(&token_hash)
        })
        .await??
        .ok_or_else(|| AppError::bad_request("AUTH_INVALID_RESET_TOKEN", "重置令牌无效"))?;

    let expires_at =
        chrono::DateTime::parse_from_rfc3339(entry["expires_at"].as_str().unwrap_or_default())
            .map_err(|e| AppError::internal(&format!("reset token expires_at parse error: {e}")))?;

    if expires_at <= Utc::now() {
        return Err(AppError::bad_request(
            "AUTH_EXPIRED_RESET_TOKEN",
            "重置令牌已过期",
        ));
    }

    let user_id = entry["user_id"]
        .as_str()
        .ok_or_else(|| AppError::internal("reset token missing user_id"))?;

    let mut user = state
        .run_store_task("auth.reset_password.get_user", {
            let user_id = user_id.to_string();
            move |store| store.get_user_by_id(&user_id)
        })
        .await??
        .ok_or_else(|| AppError::bad_request("AUTH_INVALID_RESET_TOKEN", "重置令牌无效"))?;

    let new_password = req.new_password;
    user.password_hash =
        crate::blocking::run_blocking("auth.reset_password.hash_password", move || {
            hash_password(&new_password)
        })
        .await??;
    user.updated_at = Utc::now();
    let user_for_update = user.clone();
    let user_id = user.id.clone();
    match state
        .run_store_task(
            "auth.reset_password.update_user",
            move |store| -> Result<(), AppError> {
                store.update_user(&user_for_update)?;
                let _ = store.delete_user_sessions(&user_id);
                Ok(())
            },
        )
        .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => return Err(e),
        Err(e) => return Err(e.into()),
    }

    Ok(ok(serde_json::json!({})))
}

async fn verify_reset_token(
    State(state): State<AppState>,
    JsonBody(req): JsonBody<VerifyResetTokenRequest>,
) -> Result<impl IntoResponse, AppError> {
    let token_hash = hash_token(&req.token);

    let entry = state
        .run_store_task("auth.verify_reset_token.get_token", move |store| {
            store.get_password_reset_token(&token_hash)
        })
        .await??
        .ok_or_else(|| AppError::bad_request("AUTH_INVALID_RESET_TOKEN", "重置令牌无效"))?;

    let expires_at =
        chrono::DateTime::parse_from_rfc3339(entry["expires_at"].as_str().unwrap_or_default())
            .map_err(|e| AppError::internal(&format!("reset token expires_at parse error: {e}")))?;

    if expires_at <= Utc::now() {
        return Err(AppError::bad_request(
            "AUTH_EXPIRED_RESET_TOKEN",
            "重置令牌已过期",
        ));
    }

    Ok(ok(serde_json::json!({"valid": true})))
}

fn cookie_same_site_flags(secure: bool) -> &'static str {
    if secure {
        "SameSite=None; Secure"
    } else {
        "SameSite=Lax"
    }
}

fn set_token_cookie(
    response: &mut Response,
    token: &str,
    max_age_secs: u64,
    secure: bool,
) -> Result<(), AppError> {
    let flags = cookie_same_site_flags(secure);
    let cookie = format!("token={token}; Path=/; Max-Age={max_age_secs}; {flags}; HttpOnly");
    append_set_cookie(response, &cookie, "token cookie set failed")?;
    Ok(())
}

fn set_refresh_token_cookie(
    response: &mut Response,
    refresh_token: &str,
    max_age_secs: u64,
    secure: bool,
) -> Result<(), AppError> {
    let flags = cookie_same_site_flags(secure);
    let cookie =
        format!("refresh_token={refresh_token}; Path=/; Max-Age={max_age_secs}; {flags}; HttpOnly");
    append_set_cookie(response, &cookie, "refresh token cookie set failed")?;
    Ok(())
}

fn clear_auth_cookies(response: &mut Response, secure: bool) -> Result<(), AppError> {
    let flags = cookie_same_site_flags(secure);
    append_set_cookie(
        response,
        &format!("token=; Path=/; Max-Age=0; {flags}; HttpOnly"),
        "token cookie clear failed",
    )?;
    append_set_cookie(
        response,
        &format!("refresh_token=; Path=/; Max-Age=0; {flags}; HttpOnly"),
        "refresh token cookie clear failed",
    )?;
    Ok(())
}

fn append_set_cookie(
    response: &mut Response,
    cookie: &str,
    error_context: &str,
) -> Result<(), AppError> {
    let value = HeaderValue::from_str(cookie)
        .map_err(|e| AppError::internal(&format!("{error_context}: {e}")))?;
    response.headers_mut().append(SET_COOKIE, value);
    Ok(())
}

fn mask_email_for_log(email: &str) -> String {
    let trimmed = email.trim();
    let Some((local, domain)) = trimmed.split_once('@') else {
        return "***".to_string();
    };

    let local_mask = local
        .chars()
        .next()
        .map(|ch| format!("{ch}***"))
        .unwrap_or_else(|| "***".to_string());
    let domain_mask = domain
        .chars()
        .next()
        .map(|ch| format!("{ch}***"))
        .unwrap_or_else(|| "***".to_string());

    format!("{local_mask}@{domain_mask}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_email_for_log_redacts_sensitive_parts() {
        assert_eq!(mask_email_for_log("alice@example.com"), "a***@e***");
        assert_eq!(mask_email_for_log("x@b.com"), "x***@b***");
        assert_eq!(mask_email_for_log("invalid-email"), "***");
    }
}
