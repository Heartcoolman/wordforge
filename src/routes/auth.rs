use axum::extract::State;
use axum::http::{header::SET_COOKIE, HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;

use crate::extractors::JsonBody;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::{
    extract_refresh_token_from_headers, hash_password, hash_token, sign_jwt_for_user,
    sign_refresh_token_for_user, verify_jwt, verify_password, AuthUser,
};
use crate::response::{created, ok, AppError};
use crate::state::AppState;
use crate::store::keys;
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PasswordResetEntry {
    user_id: String,
    expires_at: chrono::DateTime<chrono::Utc>,
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
        expires_at: Utc::now() + Duration::hours(state.config().refresh_token_expires_in_hours as i64),
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
        return Err(AppError::forbidden("Registration is currently disabled"));
    }
    if system_settings.maintenance_mode {
        return Err(AppError::forbidden("Service is under maintenance"));
    }

    let email = req.email.trim().to_lowercase();
    if !is_valid_email(&email) {
        return Err(AppError::bad_request(
            "AUTH_INVALID_EMAIL",
            "Invalid email format",
        ));
    }
    let username = req.username.trim();
    if let Err(msg) = validate_username(username) {
        return Err(AppError::bad_request("AUTH_INVALID_USERNAME", msg));
    }
    if let Err(msg) = validate_password(&req.password) {
        return Err(AppError::bad_request("AUTH_WEAK_PASSWORD", msg));
    }

    if state.store().get_user_by_email(&email)?.is_some() {
        return Err(AppError::conflict(
            "AUTH_EMAIL_EXISTS",
            "Email already registered",
        ));
    }

    if state.store().count_users()? >= system_settings.max_users as usize {
        return Err(AppError::forbidden("User registration limit reached"));
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

    let payload = AuthResponse {
        access_token: access_token.clone(),
        user: UserProfile::from(&user),
    };

    let mut response = created(payload).into_response();
    set_token_cookie(&mut response, &access_token)?;
    set_refresh_token_cookie(&mut response, &refresh_token)?;
    Ok(response)
}

async fn login(
    State(state): State<AppState>,
    JsonBody(req): JsonBody<LoginRequest>,
) -> Result<Response, AppError> {
    if state.store().get_system_settings()?.maintenance_mode {
        return Err(AppError::forbidden("Service is under maintenance"));
    }

    let user = state
        .store()
        .get_user_by_email(&req.email)?
        .ok_or_else(|| AppError::unauthorized("Invalid email or password"))?;

    if user.is_banned {
        return Err(AppError::forbidden("User is banned"));
    }

    // 检查账户是否因多次登录失败而被锁定
    if state.store().is_account_locked(&user.id)? {
        return Err(AppError::too_many_requests(
            "Account temporarily locked due to too many failed login attempts. Please try again later.",
        ));
    }

    let verified = verify_password(&req.password, &user.password_hash)?;
    if !verified {
        // 记录登录失败，可能触发锁定
        let _ = state.store().record_failed_login(&user.id);
        return Err(AppError::unauthorized("Invalid email or password"));
    }

    // 登录成功，重置失败计数
    let _ = state.store().reset_login_attempts(&user.id);

    let (access_token, refresh_token) = issue_token_pair(&user.id, &state)?;

    let payload = AuthResponse {
        access_token: access_token.clone(),
        user: UserProfile::from(&user),
    };

    let mut response = ok(payload).into_response();
    set_token_cookie(&mut response, &access_token)?;
    set_refresh_token_cookie(&mut response, &refresh_token)?;
    Ok(response)
}

async fn refresh(State(state): State<AppState>, headers: HeaderMap) -> Result<Response, AppError> {
    // Extract the refresh token from Authorization header or cookie
    let old_token = extract_refresh_token_from_headers(&headers)?;

    // Verify the JWT is valid and has token_type == "refresh"
    let claims = verify_jwt(&old_token, &state.config().refresh_jwt_secret)?;
    if claims.token_type != "refresh" {
        return Err(AppError::unauthorized(
            "Invalid token type: expected refresh token",
        ));
    }

    // Verify the refresh session exists in the store
    let old_hash = hash_token(&old_token);
    let session = state
        .store()
        .get_session(&old_hash)?
        .ok_or_else(|| AppError::unauthorized("Refresh session not found or expired"))?;

    if session.user_id != claims.sub {
        return Err(AppError::unauthorized("Refresh session mismatch"));
    }

    // 原子性删除旧的 refresh 会话，防止 token 重放攻击
    let was_deleted = state.store().delete_session_if_exists(&old_hash)?;
    if !was_deleted {
        // token 已被使用（可能是重放攻击），拒绝请求
        return Err(AppError::unauthorized("Refresh token already consumed"));
    }

    // 在签发新 token 前检查用户状态（封禁检查）
    let user = state
        .store()
        .get_user_by_id(&claims.sub)?
        .ok_or_else(|| AppError::unauthorized("User not found"))?;

    if user.is_banned {
        return Err(AppError::forbidden("User is banned"));
    }

    // Issue a new token pair
    let (access_token, refresh_token) = issue_token_pair(&claims.sub, &state)?;

    let mut response = ok(AuthResponse {
        access_token: access_token.clone(),
        user: UserProfile::from(&user),
    })
    .into_response();
    set_token_cookie(&mut response, &access_token)?;
    set_refresh_token_cookie(&mut response, &refresh_token)?;
    Ok(response)
}

async fn logout(auth_user: AuthUser, State(state): State<AppState>) -> Result<Response, AppError> {
    state.store().delete_user_sessions(&auth_user.user_id)?;

    let mut response = ok(serde_json::json!({"loggedOut": true})).into_response();
    clear_auth_cookies(&mut response)?;
    Ok(response)
}

async fn forgot_password(
    State(state): State<AppState>,
    JsonBody(req): JsonBody<ForgotPasswordRequest>,
) -> Result<impl IntoResponse, AppError> {
    if let Some(user) = state.store().get_user_by_email(&req.email)? {
        let raw_token = uuid::Uuid::new_v4().simple().to_string();
        let token_hash = hash_token(&raw_token);

        let entry = PasswordResetEntry {
            user_id: user.id.clone(),
            expires_at: Utc::now() + Duration::hours(1),
        };

        state
            .store()
            .password_reset_tokens
            .insert(
                keys::password_reset_key(&token_hash)?.as_bytes(),
                serde_json::to_vec(&entry).map_err(|e| AppError::internal(&e.to_string()))?,
            )
            .map_err(|e| AppError::internal(&e.to_string()))?;

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
        "message": "If the email exists, a password reset link will be sent.",
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
    let key = keys::password_reset_key(&token_hash)?;

    // 原子删除 token，防止 TOCTOU 竞态条件：
    // 先 remove() 再检查返回值，确保同一 token 只能使用一次
    let raw = state
        .store()
        .password_reset_tokens
        .remove(key.as_bytes())
        .map_err(|e| AppError::internal(&e.to_string()))?
        .ok_or_else(|| AppError::bad_request("AUTH_INVALID_RESET_TOKEN", "Invalid reset token"))?;

    let entry: PasswordResetEntry = serde_json::from_slice(&raw)
        .map_err(|e| AppError::internal(&format!("reset token decode error: {e}")))?;

    if entry.expires_at <= Utc::now() {
        return Err(AppError::bad_request(
            "AUTH_EXPIRED_RESET_TOKEN",
            "Reset token expired",
        ));
    }

    let mut user = state
        .store()
        .get_user_by_id(&entry.user_id)?
        .ok_or_else(|| AppError::bad_request("AUTH_INVALID_RESET_TOKEN", "Invalid reset token"))?;

    user.password_hash = hash_password(&req.new_password)?;
    user.updated_at = Utc::now();
    state.store().update_user(&user)?;

    let _ = state.store().delete_user_sessions(&user.id);

    Ok(ok(serde_json::json!({})))
}

fn set_token_cookie(response: &mut Response, token: &str) -> Result<(), AppError> {
    let cookie = format!("token={token}; Path=/; SameSite=Strict; HttpOnly; Secure");
    append_set_cookie(response, &cookie, "token cookie set failed")?;
    Ok(())
}

fn set_refresh_token_cookie(response: &mut Response, refresh_token: &str) -> Result<(), AppError> {
    let cookie =
        format!("refresh_token={refresh_token}; Path=/; SameSite=Strict; HttpOnly; Secure");
    append_set_cookie(response, &cookie, "refresh token cookie set failed")?;
    Ok(())
}

fn clear_auth_cookies(response: &mut Response) -> Result<(), AppError> {
    append_set_cookie(
        response,
        "token=; Path=/; Max-Age=0; SameSite=Strict; HttpOnly; Secure",
        "token cookie clear failed",
    )?;
    append_set_cookie(
        response,
        "refresh_token=; Path=/; Max-Age=0; SameSite=Strict; HttpOnly; Secure",
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
