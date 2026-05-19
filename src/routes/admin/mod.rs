pub mod amas;
pub mod analytics;
pub mod auth;
pub mod broadcast;
pub mod clients;
pub mod feedback;
pub mod monitoring;
pub mod settings;
pub mod updates;

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};

use crate::auth::AdminAuthUser;
use crate::extractors::JsonBody;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::operations::users::User;

/// Safe admin view of a user (excludes password_hash).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminUserView {
    id: String,
    email: String,
    username: String,
    is_banned: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    failed_login_count: u32,
    locked_until: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<&User> for AdminUserView {
    fn from(u: &User) -> Self {
        Self {
            id: u.id.clone(),
            email: u.email.clone(),
            username: u.username.clone(),
            is_banned: u.is_banned,
            created_at: u.created_at,
            updated_at: u.updated_at,
            failed_login_count: u.failed_login_count,
            locked_until: u.locked_until,
        }
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        // 注意：/auth 路由已移至 build_router 中单独挂载（附加专用速率限制）
        .nest("/analytics", analytics::router())
        .nest("/monitoring", monitoring::router())
        .nest("/broadcast", broadcast::router())
        .route("/broadcast-update", post(broadcast::broadcast_update))
        .nest("/settings", settings::router())
        .nest("/wordbook-center", super::wordbook_center::admin_router())
        .nest("/amas", amas::admin_router())
        .nest("/updates", updates::router())
        .nest("/clients", clients::router())
        .nest("/feedback", feedback::router())
        .nest("/telemetry", clients::telemetry_router())
        .route("/users", get(list_users))
        .route("/users/:id/ban", post(ban_user))
        .route("/users/:id/unban", post(unban_user))
        .route("/stats", get(admin_stats))
        .route("/users/:id/reset-password", post(admin_reset_user_password))
        .route("/users/:id/set-password", post(admin_set_user_password))
}

/// 导出 admin 认证路由（用于在外层添加专用速率限制）
pub fn auth_router() -> Router<AppState> {
    auth::router()
}

/// 导出 admin 认证公开路由（不受速率限制）
pub fn auth_public_router() -> Router<AppState> {
    auth::public_router()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListUsersQuery {
    page: Option<u64>,
    per_page: Option<u64>,
    search: Option<String>,
    banned: Option<bool>,
}

async fn list_users(
    _admin: AdminAuthUser,
    Query(q): Query<ListUsersQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let page = q.page.unwrap_or(1).clamp(1, u64::MAX);
    let per_page = q
        .per_page
        .unwrap_or(state.config().pagination.default_page_size)
        .clamp(1, state.config().pagination.max_page_size);
    let search = q
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_ascii_lowercase);

    let (users, total) = state
        .admin_service()
        .list_users(page, per_page, search, q.banned)
        .await?;

    let safe_users: Vec<AdminUserView> = users.iter().map(AdminUserView::from).collect();
    Ok(crate::response::paginated(
        safe_users, total, page, per_page,
    ))
}

async fn ban_user(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let revoked = state.admin_service().ban_user(id.clone()).await?;
    tracing::info!(
        admin_id = %admin.admin_id,
        action = "ban_user",
        target_user_id = %id,
        sessions_revoked = revoked,
        "管理员封禁用户"
    );
    Ok(ok(
        serde_json::json!({"banned": true, "userId": id, "sessionsRevoked": revoked}),
    ))
}

async fn unban_user(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    state.admin_service().unban_user(id.clone()).await?;
    tracing::info!(
        admin_id = %admin.admin_id,
        action = "unban_user",
        target_user_id = %id,
        "管理员解封用户"
    );
    Ok(ok(serde_json::json!({"banned": false, "userId": id})))
}

async fn admin_stats(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    Ok(ok(state.admin_service().stats().await?))
}

async fn admin_reset_user_password(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let reset = state
        .admin_service()
        .create_password_reset(id.clone())
        .await?;

    tracing::info!(
        admin_id = %admin.admin_id,
        action = "reset_user_password",
        target_user_id = %id,
        "管理员生成密码重置密钥"
    );

    Ok(ok(serde_json::json!({
        "resetCreated": true,
        "resetKey": reset.reset_key,
        "expiresInHours": reset.expires_in_hours,
        "message": "密码重置令牌已创建，请通过安全渠道通知用户",
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdminSetPasswordRequest {
    new_password: String,
}

async fn admin_set_user_password(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<AdminSetPasswordRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if let Err(msg) = crate::validation::validate_password(&req.new_password) {
        return Err(AppError::bad_request("AUTH_WEAK_PASSWORD", msg));
    }

    let revoked = state
        .admin_service()
        .set_user_password(id.clone(), req.new_password)
        .await?;

    tracing::info!(
        admin_id = %admin.admin_id,
        action = "set_user_password",
        target_user_id = %id,
        sessions_revoked = revoked,
        "管理员直接重置用户密码"
    );

    Ok(ok(serde_json::json!({
        "passwordReset": true,
        "userId": id,
        "sessionsRevoked": revoked,
    })))
}
