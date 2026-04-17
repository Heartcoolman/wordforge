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
    let initialized = state
        .run_store_task("admin_auth.status.any_admin_exists", |store| {
            store.any_admin_exists()
        })
        .await??;
    Ok(ok(AuthStatusResponse { initialized }))
}

async fn setup(
    State(state): State<AppState>,
    JsonBody(req): JsonBody<SetupRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if !is_valid_email(&req.email) {
        return Err(AppError::bad_request("ADMIN_INVALID_EMAIL", "邮箱格式无效"));
    }
    if let Err(msg) = validate_password(&req.password) {
        return Err(AppError::bad_request("ADMIN_WEAK_PASSWORD", msg));
    }

    let password = req.password;
    let password_hash =
        crate::blocking::run_blocking("admin_auth.setup.hash_password", move || {
            hash_password(&password)
        })
        .await??;
    let admin = Admin {
        id: uuid::Uuid::new_v4().to_string(),
        email: req.email.trim().to_lowercase(),
        password_hash,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        failed_login_count: 0,
        locked_until: None,
    };

    // 使用 create_first_admin 在事务内部原子性检查是否已有 admin，防止 TOCTOU
    let admin_for_create = admin.clone();
    state
        .run_store_task("admin_auth.setup.create_first_admin", move |store| {
            store.create_first_admin(&admin_for_create)
        })
        .await
        .map_err(AppError::from)?
        .map_err(|e| {
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

    let session = Session {
        token_hash: hash_token(&token),
        user_id: admin.id.clone(),
        token_type: "admin".to_string(),
        created_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(state.config().admin_jwt_expires_in_hours as i64),
        revoked: false,
    };
    state
        .run_store_task("admin_auth.setup.create_admin_session", move |store| {
            store.create_admin_session(&session)
        })
        .await??;

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
    let email = req.email.trim().to_lowercase();
    let (admin, is_locked) = state
        .run_store_task(
            "admin_auth.login.lookup",
            move |store| -> Result<_, AppError> {
                let admin = store.get_admin_by_email(&email)?;
                let is_locked = if let Some(ref admin) = admin {
                    store.is_admin_account_locked(&admin.id)?
                } else {
                    false
                };
                Ok((admin, is_locked))
            },
        )
        .await??;

    // 检查账户是否因多次登录失败而被锁定
    if is_locked {
        return Err(AppError::too_many_requests(
            "账户因多次登录失败已被临时锁定，请稍后再试",
        ));
    }

    let stored_hash = admin
        .as_ref()
        .map(|admin| admin.password_hash.clone())
        .unwrap_or_else(generate_dummy_argon2_hash);
    let password = req.password;
    let verified = crate::blocking::run_blocking("admin_auth.login.verify_password", move || {
        verify_password(&password, &stored_hash)
    })
    .await??;
    if !verified || admin.is_none() {
        // 记录登录失败，可能触发锁定
        if let Some(ref a) = admin {
            let admin_id = a.id.clone();
            match state
                .run_store_task("admin_auth.login.record_failed_login", move |store| {
                    store.record_admin_failed_login(&admin_id)
                })
                .await
            {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => tracing::error!(
                    admin_id = %a.id,
                    error = %e,
                    "记录管理员登录失败次数时出错"
                ),
                Err(e) => tracing::error!(
                    admin_id = %a.id,
                    error = %e,
                    "记录管理员登录失败任务失败"
                ),
            }
        }
        return Err(AppError::unauthorized("邮箱或密码错误"));
    }

    let admin = admin.unwrap();

    // 登录成功，重置失败计数
    let admin_id = admin.id.clone();
    match state
        .run_store_task("admin_auth.login.reset_attempts", move |store| {
            store.reset_admin_login_attempts(&admin_id)
        })
        .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => tracing::error!(
            admin_id = %admin.id,
            error = %e,
            "重置管理员登录失败计数时出错"
        ),
        Err(e) => tracing::error!(
            admin_id = %admin.id,
            error = %e,
            "重置管理员登录失败计数任务失败"
        ),
    }

    let token = sign_jwt_for_admin(
        &admin.id,
        &state.config().admin_jwt_secret,
        state.config().admin_jwt_expires_in_hours,
    )?;

    let session = Session {
        token_hash: hash_token(&token),
        user_id: admin.id.clone(),
        token_type: "admin".to_string(),
        created_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(state.config().admin_jwt_expires_in_hours as i64),
        revoked: false,
    };
    state
        .run_store_task("admin_auth.login.create_admin_session", move |store| {
            store.create_admin_session(&session)
        })
        .await??;

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
        .run_store_task("admin_auth.verify.get_admin_by_id", move |store| {
            store.get_admin_by_id(&admin.admin_id)
        })
        .await??
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
    state
        .run_store_task("admin_auth.logout.delete_admin_session", move |store| {
            store.delete_admin_session(&token_hash)
        })
        .await??;
    Ok(ok(serde_json::json!({"loggedOut": true})))
}
