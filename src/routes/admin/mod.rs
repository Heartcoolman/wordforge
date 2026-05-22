pub mod amas;
pub mod analytics;
pub mod auth;
pub mod broadcast;
pub mod clients;
pub mod feedback;
pub mod monitoring;
pub mod probe;
pub mod resource_packs;
pub mod settings;
pub mod updates;

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::Router;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::{hash_password, hash_token, AdminAuthUser};
use crate::blocking;
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
        .nest("/probe", probe::router())
        .nest("/resource-packs", resource_packs::router())
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

    let (users, total) = admin_list_users(&state, page, per_page, search, q.banned).await?;

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
    let revoked = admin_ban_user(&state, id.clone()).await?;
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
    admin_unban_user(&state, id.clone()).await?;
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
    Ok(ok(admin_get_stats(&state).await?))
}

async fn admin_reset_user_password(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let reset = admin_create_password_reset(&state, id.clone()).await?;

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

    let revoked = admin_do_set_user_password(&state, id.clone(), req.new_password).await?;

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

// ── Admin 用户管理 helpers（原 AdminService 方法） ──

async fn admin_list_users(
    state: &AppState,
    page: u64,
    per_page: u64,
    search: Option<String>,
    banned: Option<bool>,
) -> Result<(Vec<User>, u64), AppError> {
    let store = state.store().clone();
    let limit = per_page as usize;
    let offset = ((page - 1) * per_page) as usize;
    let has_filter = search.is_some() || banned.is_some();

    blocking::run_blocking("admin.list_users", move || {
        if has_filter {
            let mut all = store.list_users(10_000, 0)?;
            all.retain(|user| {
                let banned_match = banned.map(|v| user.is_banned == v).unwrap_or(true);
                let search_match = search.as_ref().map_or(true, |needle| {
                    user.username.to_ascii_lowercase().contains(needle)
                        || user.email.to_ascii_lowercase().contains(needle)
                });
                banned_match && search_match
            });
            let total = all.len() as u64;
            let page_slice = all.into_iter().skip(offset).take(limit).collect();
            Ok::<_, crate::store::StoreError>((page_slice, total))
        } else {
            let page_slice = store.list_users(limit, offset)?;
            let total = store.count_users()? as u64;
            Ok((page_slice, total))
        }
    })
    .await?
    .map_err(AppError::from)
}

async fn admin_ban_user(state: &AppState, user_id: String) -> Result<u32, AppError> {
    let store = state.store().clone();
    blocking::run_blocking("admin.ban_user", move || -> Result<_, AppError> {
        if store.get_user_by_id(&user_id)?.is_none() {
            return Err(AppError::not_found("用户不存在"));
        }
        store.ban_user(&user_id)?;
        Ok(store.delete_user_sessions(&user_id)?)
    })
    .await?
}

async fn admin_unban_user(state: &AppState, user_id: String) -> Result<(), AppError> {
    let store = state.store().clone();
    blocking::run_blocking("admin.unban_user", move || -> Result<_, AppError> {
        if store.get_user_by_id(&user_id)?.is_none() {
            return Err(AppError::not_found("用户不存在"));
        }
        store.unban_user(&user_id)?;
        Ok(())
    })
    .await?
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminStats {
    users: usize,
    words: u64,
    records: usize,
    trend: AdminStatsTrend,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminStatsTrend {
    users: TrendValue,
    records: TrendValue,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrendValue {
    value: i64,
    label: &'static str,
}

struct AdminStatsCounts {
    users: usize,
    words: u64,
    records: usize,
    users_today: usize,
    users_yesterday: usize,
    records_today: usize,
    records_yesterday: usize,
}

fn calc_trend(today: usize, yesterday: usize) -> i64 {
    if yesterday == 0 {
        return 0;
    }
    ((today as f64 - yesterday as f64) / yesterday as f64 * 100.0).round() as i64
}

async fn admin_get_stats(state: &AppState) -> Result<AdminStats, AppError> {
    let store = state.store().clone();
    let today = Utc::now().date_naive();
    let today_str = today.format("%Y-%m-%d").to_string();
    let yesterday_str = (today - Duration::days(1)).format("%Y-%m-%d").to_string();

    let counts = blocking::run_blocking("admin.stats", move || {
        Ok::<_, crate::store::StoreError>(AdminStatsCounts {
            users: store.count_users()?,
            words: store.count_words()?,
            records: store.count_all_records()?,
            users_today: store.count_users_registered_on_date(&today_str)?,
            users_yesterday: store.count_users_registered_on_date(&yesterday_str)?,
            records_today: store.count_records_on_date(&today_str)?,
            records_yesterday: store.count_records_on_date(&yesterday_str)?,
        })
    })
    .await??;

    Ok(AdminStats {
        users: counts.users,
        words: counts.words,
        records: counts.records,
        trend: AdminStatsTrend {
            users: TrendValue {
                value: calc_trend(counts.users_today, counts.users_yesterday),
                label: "较昨日",
            },
            records: TrendValue {
                value: calc_trend(counts.records_today, counts.records_yesterday),
                label: "较昨日",
            },
        },
    })
}

struct PasswordResetKey {
    reset_key: String,
    expires_in_hours: u8,
}

async fn admin_create_password_reset(
    state: &AppState,
    user_id: String,
) -> Result<PasswordResetKey, AppError> {
    let store = state.store().clone();
    let raw_token = uuid::Uuid::new_v4().simple().to_string();
    let token_hash = hash_token(&raw_token);
    let expires_in_hours = 4u8;
    let expires_at = (Utc::now() + Duration::hours(expires_in_hours as i64)).to_rfc3339();

    blocking::run_blocking(
        "admin.create_password_reset",
        move || -> Result<_, AppError> {
            if store.get_user_by_id(&user_id)?.is_none() {
                return Err(AppError::not_found("用户不存在"));
            }
            store
                .create_password_reset_token(&token_hash, &user_id, &expires_at)
                .map_err(|e| AppError::internal(&e.to_string()))?;
            Ok(())
        },
    )
    .await??;

    Ok(PasswordResetKey {
        reset_key: raw_token,
        expires_in_hours,
    })
}

async fn admin_do_set_user_password(
    state: &AppState,
    user_id: String,
    new_password: String,
) -> Result<u32, AppError> {
    let store = state.store().clone();
    blocking::run_blocking(
        "admin.set_user_password",
        move || -> Result<_, AppError> {
            let mut user = store
                .get_user_by_id(&user_id)?
                .ok_or_else(|| AppError::not_found("用户不存在"))?;

            user.password_hash = hash_password(&new_password)?;
            user.updated_at = Utc::now();
            store.update_user(&user)?;

            Ok(store.delete_user_sessions(&user_id)?)
        },
    )
    .await?
}
