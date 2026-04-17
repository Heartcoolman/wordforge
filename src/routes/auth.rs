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
fn issue_token_pair(user_id: &str, state: &AppState) -> Result<(String, String), AppError> {
    // 清理超出限制的旧会话
    if let Err(e) = state
        .store()
        .cleanup_oldest_user_sessions(user_id, MAX_SESSIONS_PER_USER)
    {
        tracing::warn!(user_id, error = %e, "清理多余会话失败");
    }

    let access_token = sign_jwt_for_user(
        user_id,
        &state.config().jwt_secret,
        state.config().jwt_expires_in_hours,
    )?;

    let refresh_token = sign_refresh_token_for_user(
        user_id,
        &state.config().refresh_jwt_secret,
        state.config().refresh_token_expires_in_hours,
    )?;

    // Persist the access token session
    let token_hash = hash_token(&access_token);
    state.store().create_session(&Session {
        token_hash,
        user_id: user_id.to_string(),
        token_type: "user".to_string(),
        created_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(state.config().jwt_expires_in_hours as i64),
        revoked: false,
    })?;

    // Persist the refresh token session (longer expiry)
    let refresh_hash = hash_token(&refresh_token);
    state.store().create_session(&Session {
        token_hash: refresh_hash,
        user_id: user_id.to_string(),
        token_type: "refresh".to_string(),
        created_at: Utc::now(),
        expires_at: Utc::now()
            + Duration::hours(state.config().refresh_token_expires_in_hours as i64),
        revoked: false,
    })?;

    Ok((access_token, refresh_token))
}

async fn register(
    State(state): State<AppState>,
    JsonBody(req): JsonBody<RegisterRequest>,
) -> Result<Response, AppError> {
    let system_settings = state.store().get_system_settings()?;
    if !system_settings.registration_enabled {
        return Err(AppError::forbidden("注册功能已关闭"));
    }
    if system_settings.maintenance_mode {
        return Err(AppError::forbidden("系统正在维护中"));
    }

    let email = req.email.trim().to_lowercase();
    if !is_valid_email(&email) {
        return Err(AppError::bad_request("AUTH_INVALID_EMAIL", "邮箱格式无效"));
    }
    let username = req.username.trim();
    if let Err(msg) = validate_username(username) {
        return Err(AppError::bad_request("AUTH_INVALID_USERNAME", msg));
    }
    if let Err(msg) = validate_password(&req.password) {
        return Err(AppError::bad_request("AUTH_WEAK_PASSWORD", msg));
    }

    if state.store().get_user_by_email(&email)?.is_some() {
        return Err(AppError::conflict("AUTH_EMAIL_EXISTS", "该邮箱已被注册"));
    }

    if state.store().count_users()? >= system_settings.max_users as usize {
        return Err(AppError::forbidden("用户注册数量已达上限"));
    }

    let now = Utc::now();
    let user = User {
        id: uuid::Uuid::new_v4().to_string(),
        email: email.clone(),
        username: username.to_string(),
        password_hash: hash_password(&req.password)?,
        is_banned: false,
        created_at: now,
        updated_at: now,
        failed_login_count: 0,
        locked_until: None,
    };

    state.store().create_user(&user)?;

    let (access_token, refresh_token) = issue_token_pair(&user.id, &state)?;

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
    if state.store().get_system_settings()?.maintenance_mode {
        return Err(AppError::forbidden("系统正在维护中"));
    }

    let (user, stored_hash) = match state.store().get_user_by_email(&req.email)? {
        Some(user) => {
            let hash = user.password_hash.clone();
            (Some(user), hash)
        }
        None => (None, generate_dummy_argon2_hash()),
    };

    // Check ban and lockout status BEFORE password verification to prevent timing attacks
    if let Some(ref u) = user {
        if u.is_banned {
            return Err(AppError::forbidden("用户已被封禁"));
        }

        if state.store().is_account_locked(&u.id)? {
            return Err(AppError::too_many_requests(
                "账户因多次登录失败已被临时锁定，请稍后再试",
            ));
        }
    }

    let verified = verify_password(&req.password, &stored_hash)?;
    if !verified || user.is_none() {
        if let Some(ref u) = user {
            let _ = state.store().record_failed_login(&u.id);
        }
        return Err(AppError::unauthorized("邮箱或密码错误"));
    }

    let user = user.unwrap();

    if let Err(e) = state.store().reset_login_attempts(&user.id) {
        tracing::warn!(user_id = %user.id, error = %e, "Failed to reset login attempts");
    }

    let (access_token, refresh_token) = issue_token_pair(&user.id, &state)?;

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
    let session = state
        .store()
        .get_session(&old_hash)?
        .ok_or_else(|| AppError::unauthorized("刷新会话不存在或已过期"))?;

    if session.user_id != claims.sub {
        return Err(AppError::unauthorized("刷新会话不匹配"));
    }

    // 原子性删除旧的 refresh 会话，防止 token 重放攻击
    let was_deleted = state.store().delete_session_if_exists(&old_hash)?;
    if !was_deleted {
        // token 已被使用（可能是重放攻击），拒绝请求
        return Err(AppError::unauthorized("刷新令牌已被使用"));
    }

    // 在签发新 token 前检查用户状态（封禁检查）
    let user = state
        .store()
        .get_user_by_id(&claims.sub)?
        .ok_or_else(|| AppError::unauthorized("用户不存在"))?;

    if user.is_banned {
        return Err(AppError::forbidden("用户已被封禁"));
    }

    // Issue a new token pair
    let (access_token, refresh_token) = issue_token_pair(&claims.sub, &state)?;

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
    state.store().delete_user_sessions(&auth_user.user_id)?;

    let mut response = ok(serde_json::json!({"loggedOut": true})).into_response();
    clear_auth_cookies(&mut response, state.config().cookie_secure)?;
    Ok(response)
}

async fn forgot_password(
    State(state): State<AppState>,
    JsonBody(req): JsonBody<ForgotPasswordRequest>,
) -> Result<impl IntoResponse, AppError> {
    if let Some(user) = state.store().get_user_by_email(&req.email)? {
        let raw_token = uuid::Uuid::new_v4().simple().to_string();
        let token_hash = hash_token(&raw_token);
        let expires_at = (Utc::now() + Duration::hours(1)).to_rfc3339();

        state
            .store()
            .create_password_reset_token(&token_hash, &user.id, &expires_at)?;

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
        .store()
        .take_password_reset_token(&token_hash)?
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
        .store()
        .get_user_by_id(user_id)?
        .ok_or_else(|| AppError::bad_request("AUTH_INVALID_RESET_TOKEN", "重置令牌无效"))?;

    user.password_hash = hash_password(&req.new_password)?;
    user.updated_at = Utc::now();
    state.store().update_user(&user)?;

    let _ = state.store().delete_user_sessions(&user.id);

    Ok(ok(serde_json::json!({})))
}

async fn verify_reset_token(
    State(state): State<AppState>,
    JsonBody(req): JsonBody<VerifyResetTokenRequest>,
) -> Result<impl IntoResponse, AppError> {
    let token_hash = hash_token(&req.token);

    let entry = state
        .store()
        .get_password_reset_token(&token_hash)?
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

fn set_token_cookie(
    response: &mut Response,
    token: &str,
    max_age_secs: u64,
    secure: bool,
) -> Result<(), AppError> {
    let secure_flag = if secure { "; Secure" } else { "" };
    let cookie = format!(
        "token={token}; Path=/; Max-Age={max_age_secs}; SameSite=None; HttpOnly{secure_flag}"
    );
    append_set_cookie(response, &cookie, "token cookie set failed")?;
    Ok(())
}

fn set_refresh_token_cookie(
    response: &mut Response,
    refresh_token: &str,
    max_age_secs: u64,
    secure: bool,
) -> Result<(), AppError> {
    let secure_flag = if secure { "; Secure" } else { "" };
    let cookie = format!(
        "refresh_token={refresh_token}; Path=/; Max-Age={max_age_secs}; SameSite=None; HttpOnly{secure_flag}"
    );
    append_set_cookie(response, &cookie, "refresh token cookie set failed")?;
    Ok(())
}

fn clear_auth_cookies(response: &mut Response, secure: bool) -> Result<(), AppError> {
    let secure_flag = if secure { "; Secure" } else { "" };
    append_set_cookie(
        response,
        &format!("token=; Path=/; Max-Age=0; SameSite=None; HttpOnly{secure_flag}"),
        "token cookie clear failed",
    )?;
    append_set_cookie(
        response,
        &format!("refresh_token=; Path=/; Max-Age=0; SameSite=None; HttpOnly{secure_flag}"),
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
