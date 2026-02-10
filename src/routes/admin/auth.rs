use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::{
    extract_token_from_headers, hash_password, hash_token, sign_jwt_for_admin, verify_password,
    AdminAuthUser,
};
use crate::response::{created, ok, AppError};
use crate::state::AppState;
use crate::store::operations::admins::Admin;
use crate::store::operations::sessions::Session;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(auth_status))
        .route("/setup", post(setup))
        .route("/login", post(login))
        .route("/logout", post(logout))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthStatusResponse {
    initialized: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetupRequest {
    email: String,
    password: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminAuthResponse {
    token: String,
    admin: AdminProfile,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminProfile {
    id: String,
    email: String,
}

async fn auth_status(
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let initialized = state.store().any_admin_exists()?;
    Ok(ok(AuthStatusResponse { initialized }))
}

async fn setup(
    State(state): State<AppState>,
    Json(req): Json<SetupRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if state.store().any_admin_exists()? {
        return Err(AppError::conflict(
            "ADMIN_ALREADY_EXISTS",
            "Admin account already exists",
        ));
    }

    if !req.email.contains('@') {
        return Err(AppError::bad_request(
            "ADMIN_INVALID_EMAIL",
            "Invalid email format",
        ));
    }
    if req.password.len() < 8 {
        return Err(AppError::bad_request(
            "ADMIN_WEAK_PASSWORD",
            "Password must be at least 8 characters",
        ));
    }

    let admin = Admin {
        id: uuid::Uuid::new_v4().to_string(),
        email: req.email.trim().to_lowercase(),
        password_hash: hash_password(&req.password)?,
        created_at: Utc::now(),
    };

    state.store().create_admin(&admin)?;

    let token = sign_jwt_for_admin(
        &admin.id,
        &state.config().admin_jwt_secret,
        state.config().jwt_expires_in_hours,
    )?;

    let token_hash = hash_token(&token);
    state.store().create_admin_session(&Session {
        token_hash,
        user_id: admin.id.clone(),
        token_type: "admin".to_string(),
        created_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(state.config().jwt_expires_in_hours as i64),
        revoked: false,
    })?;

    Ok(created(AdminAuthResponse {
        token,
        admin: AdminProfile {
            id: admin.id,
            email: admin.email,
        },
    }))
}

async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let admin = state
        .store()
        .get_admin_by_email(&req.email)?
        .ok_or_else(|| AppError::unauthorized("Invalid email or password"))?;

    let verified = verify_password(&req.password, &admin.password_hash)?;
    if !verified {
        return Err(AppError::unauthorized("Invalid email or password"));
    }

    let token = sign_jwt_for_admin(
        &admin.id,
        &state.config().admin_jwt_secret,
        state.config().jwt_expires_in_hours,
    )?;

    let token_hash = hash_token(&token);
    state.store().create_admin_session(&Session {
        token_hash,
        user_id: admin.id.clone(),
        token_type: "admin".to_string(),
        created_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(state.config().jwt_expires_in_hours as i64),
        revoked: false,
    })?;

    Ok(ok(AdminAuthResponse {
        token,
        admin: AdminProfile {
            id: admin.id,
            email: admin.email,
        },
    }))
}

async fn logout(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let token = extract_token_from_headers(&headers)?;
    let token_hash = hash_token(&token);
    state.store().delete_admin_session(&token_hash)?;
    Ok(ok(serde_json::json!({"loggedOut": true})))
}
