use axum::extract::State;
use axum::http::{header::SET_COOKIE, HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::{
    extract_token_from_headers, hash_password, hash_token, sign_jwt_for_user,
    sign_refresh_token_for_user, verify_jwt, verify_password,
};
use crate::response::{created, ok, AppError};
use crate::state::AppState;
use crate::store::keys;
use crate::store::operations::sessions::Session;
use crate::store::operations::users::User;

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
pub struct RegisterRequest {
    pub email: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
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
    pub token: String,
    pub access_token: String,
    pub refresh_token: String,
    pub user: UserProfile,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PasswordResetEntry {
    user_id: String,
    expires_at: chrono::DateTime<chrono::Utc>,
}

/// Issue an access + refresh token pair and persist the access session.
fn issue_token_pair(
    user_id: &str,
    state: &AppState,
) -> Result<(String, String), AppError> {
    let access_token = sign_jwt_for_user(
        user_id,
        &state.config().jwt_secret,
        state.config().jwt_expires_in_hours,
    )?;

    let refresh_token = sign_refresh_token_for_user(
        user_id,
        &state.config().jwt_secret,
        state.config().jwt_expires_in_hours,
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
    let refresh_hours = (state.config().jwt_expires_in_hours * 7).max(168);
    state.store().create_session(&Session {
        token_hash: refresh_hash,
        user_id: user_id.to_string(),
        token_type: "refresh".to_string(),
        created_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(refresh_hours as i64),
        revoked: false,
    })?;

    Ok((access_token, refresh_token))
}

async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<Response, AppError> {
    if !req.email.contains('@') {
        return Err(AppError::bad_request(
            "AUTH_INVALID_EMAIL",
            "Invalid email format",
        ));
    }
    if req.password.len() < 8 {
        return Err(AppError::bad_request(
            "AUTH_WEAK_PASSWORD",
            "Password must be at least 8 characters",
        ));
    }

    if state.store().get_user_by_email(&req.email)?.is_some() {
        return Err(AppError::conflict(
            "AUTH_EMAIL_EXISTS",
            "Email already registered",
        ));
    }

    let now = Utc::now();
    let user = User {
        id: uuid::Uuid::new_v4().to_string(),
        email: req.email.trim().to_lowercase(),
        username: req.username.trim().to_string(),
        password_hash: hash_password(&req.password)?,
        is_banned: false,
        created_at: now,
        updated_at: now,
    };

    state.store().create_user(&user)?;

    let (access_token, refresh_token) = issue_token_pair(&user.id, &state)?;

    let payload = AuthResponse {
        token: access_token.clone(),
        access_token: access_token.clone(),
        refresh_token,
        user: UserProfile::from(&user),
    };

    let mut response = created(payload).into_response();
    set_token_cookie(&mut response, &access_token)?;
    Ok(response)
}

async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Response, AppError> {
    let user = state
        .store()
        .get_user_by_email(&req.email)?
        .ok_or_else(|| AppError::unauthorized("Invalid email or password"))?;

    if user.is_banned {
        return Err(AppError::forbidden("User is banned"));
    }

    let verified = verify_password(&req.password, &user.password_hash)?;
    if !verified {
        return Err(AppError::unauthorized("Invalid email or password"));
    }

    let (access_token, refresh_token) = issue_token_pair(&user.id, &state)?;

    let payload = AuthResponse {
        token: access_token.clone(),
        access_token: access_token.clone(),
        refresh_token,
        user: UserProfile::from(&user),
    };

    let mut response = ok(payload).into_response();
    set_token_cookie(&mut response, &access_token)?;
    Ok(response)
}

async fn refresh(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    // Extract the refresh token from Authorization header or cookie
    let old_token = extract_token_from_headers(&headers)?;

    // Verify the JWT is valid and has token_type == "refresh"
    let claims = verify_jwt(&old_token, &state.config().jwt_secret)?;
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

    // Revoke the old refresh session
    let _ = state.store().delete_session(&old_hash);

    // Issue a new token pair
    let (access_token, refresh_token) = issue_token_pair(&claims.sub, &state)?;

    let user = state
        .store()
        .get_user_by_id(&claims.sub)?
        .ok_or_else(|| AppError::unauthorized("User not found"))?;

    if user.is_banned {
        return Err(AppError::forbidden("User is banned"));
    }

    Ok(ok(AuthResponse {
        token: access_token.clone(),
        access_token: access_token.clone(),
        refresh_token,
        user: UserProfile::from(&user),
    }))
}

async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let token = extract_token_from_headers(&headers)?;
    let token_hash = hash_token(&token);
    state.store().delete_session(&token_hash)?;
    Ok(ok(serde_json::json!({"loggedOut": true})))
}

async fn forgot_password(
    State(state): State<AppState>,
    Json(req): Json<ForgotPasswordRequest>,
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
                keys::password_reset_key(&token_hash).as_bytes(),
                serde_json::to_vec(&entry).map_err(|e| AppError::internal(&e.to_string()))?,
            )
            .map_err(|e| AppError::internal(&e.to_string()))?;

        tracing::info!(
            email = %user.email,
            reset_token = %raw_token,
            "Password reset requested; email delivery disabled in trimmed build"
        );
    }

    Ok(ok(serde_json::json!({"success": true})))
}

async fn reset_password(
    State(state): State<AppState>,
    Json(req): Json<ResetPasswordRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.new_password.len() < 8 {
        return Err(AppError::bad_request(
            "AUTH_WEAK_PASSWORD",
            "Password must be at least 8 characters",
        ));
    }

    let token_hash = hash_token(&req.token);
    let key = keys::password_reset_key(&token_hash);
    let raw = state
        .store()
        .password_reset_tokens
        .get(key.as_bytes())
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
    state
        .store()
        .password_reset_tokens
        .remove(key.as_bytes())
        .map_err(|e| AppError::internal(&e.to_string()))?;

    Ok(ok(serde_json::json!({"success": true})))
}

fn set_token_cookie(response: &mut Response, token: &str) -> Result<(), AppError> {
    let cookie = format!("token={token}; Path=/; SameSite=Strict; HttpOnly");
    let value = HeaderValue::from_str(&cookie)
        .map_err(|e| AppError::internal(&format!("token cookie set failed: {e}")))?;
    response.headers_mut().append(SET_COOKIE, value);
    Ok(())
}
