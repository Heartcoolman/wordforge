use axum::extract::State;
use axum::routing::{get, post};
use axum::Router;

use crate::extractors::JsonBody;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::{
    extract_token_from_headers, generate_dummy_argon2_hash, hash_password, hash_token,
    sign_jwt_for_admin, verify_password, AdminAuthUser,
};
use crate::response::{created, ok, AppError};
use crate::state::AppState;
use crate::store::operations::admins::Admin;
use crate::store::operations::sessions::Session;
use crate::validation::{is_valid_email, validate_password};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/setup", post(setup))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/verify", get(verify))
}

/// 不受 auth rate limit 约束的公开路由
pub fn public_router() -> Router<AppState> {
    Router::new().route("/status", get(auth_status))
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
    JsonBody(req): JsonBody<SetupRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if !is_valid_email(&req.email) {
        return Err(AppError::bad_request(
            "ADMIN_INVALID_EMAIL",
            "邮箱格式无效",
        ));
    }
    if let Err(msg) = validate_password(&req.password) {
        return Err(AppError::bad_request("ADMIN_WEAK_PASSWORD", msg));
    }

    let admin = Admin {
        id: uuid::Uuid::new_v4().to_string(),
        email: req.email.trim().to_lowercase(),
        password_hash: hash_password(&req.password)?,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        failed_login_count: 0,
        locked_until: None,
    };

    // 使用 create_first_admin 在事务内部原子性检查是否已有 admin，防止 TOCTOU
    state.store().create_first_admin(&admin).map_err(|e| {
        if matches!(e, crate::store::StoreError::Conflict { .. }) {
            AppError::conflict("ADMIN_ALREADY_EXISTS", "管理员账户已存在")
        } else {
            AppError::from(e)
        }
    })?;

    let token = sign_jwt_for_admin(
        &admin.id,
        &state.config().admin_jwt_secret,
        state.config().admin_jwt_expires_in_hours,
    )?;

    let token_hash = hash_token(&token);
    state.store().create_admin_session(&Session {
        token_hash,
        user_id: admin.id.clone(),
        token_type: "admin".to_string(),
        created_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(state.config().admin_jwt_expires_in_hours as i64),
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
    JsonBody(req): JsonBody<LoginRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let (admin, stored_hash) = match state.store().get_admin_by_email(&req.email)? {
        Some(admin) => {
            let hash = admin.password_hash.clone();
            (Some(admin), hash)
        }
        None => (None, generate_dummy_argon2_hash()),
    };

    // 检查账户是否因多次登录失败而被锁定
    if let Some(ref a) = admin {
        if state.store().is_admin_account_locked(&a.id)? {
            return Err(AppError::too_many_requests(
                "账户因多次登录失败已被临时锁定，请稍后再试",
            ));
        }
    }

    let verified = verify_password(&req.password, &stored_hash)?;
    if !verified || admin.is_none() {
        // 记录登录失败，可能触发锁定
        if let Some(ref a) = admin {
            if let Err(e) = state.store().record_admin_failed_login(&a.id) {
                tracing::error!(
                    admin_id = %a.id,
                    error = %e,
                    "记录管理员登录失败次数时出错"
                );
            }
        }
        return Err(AppError::unauthorized("邮箱或密码错误"));
    }

    let admin = admin.unwrap();

    // 登录成功，重置失败计数
    if let Err(e) = state.store().reset_admin_login_attempts(&admin.id) {
        tracing::error!(
            admin_id = %admin.id,
            error = %e,
            "重置管理员登录失败计数时出错"
        );
    }

    let token = sign_jwt_for_admin(
        &admin.id,
        &state.config().admin_jwt_secret,
        state.config().admin_jwt_expires_in_hours,
    )?;

    let token_hash = hash_token(&token);
    state.store().create_admin_session(&Session {
        token_hash,
        user_id: admin.id.clone(),
        token_type: "admin".to_string(),
        created_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(state.config().admin_jwt_expires_in_hours as i64),
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

/// 验证当前管理员 token 是否有效，返回管理员基本信息
async fn verify(
    admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let admin_record = state
        .store()
        .get_admin_by_id(&admin.admin_id)?
        .ok_or_else(|| AppError::unauthorized("管理员不存在"))?;
    Ok(ok(AdminProfile {
        id: admin_record.id,
        email: admin_record.email,
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
