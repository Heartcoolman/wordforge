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
    /// m022:'user' / 'staff' / 'admin'
    role: String,
    /// m022:'active' / 'inactive' / 'suspended'
    status: String,
    /// m022:NULL 表示从未登录
    last_login_at: Option<chrono::DateTime<chrono::Utc>>,
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
            role: u.role.clone(),
            status: u.status.clone(),
            last_login_at: u.last_login_at,
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
        // m022:用户管理扩展端点
        .route("/users/:id/profile", get(admin_user_profile))
        .route("/users/:id/sessions", get(admin_user_sessions))
        .route("/users/bulk-ban", post(admin_users_bulk_ban))
        .route("/users/bulk-unban", post(admin_users_bulk_unban))
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
    // v1.1-P2.10：admin 敏感操作审计
    write_user_admin_audit(
        &state,
        &admin.admin_id,
        "user.ban",
        &id,
        serde_json::json!({ "sessionsRevoked": revoked }),
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
    // v1.1-P2.10：admin 敏感操作审计
    write_user_admin_audit(
        &state,
        &admin.admin_id,
        "user.unban",
        &id,
        serde_json::Value::Null,
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

    // v1.1-P2.10：admin 敏感操作审计（不写 resetKey 明文）
    write_user_admin_audit(
        &state,
        &admin.admin_id,
        "user.reset_password",
        &id,
        serde_json::json!({ "expiresInHours": reset.expires_in_hours }),
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

    // v1.1-P2.10：admin 敏感操作审计（不写明文密码）
    write_user_admin_audit(
        &state,
        &admin.admin_id,
        "user.set_password",
        &id,
        serde_json::json!({ "sessionsRevoked": revoked }),
    );

    Ok(ok(serde_json::json!({
        "passwordReset": true,
        "userId": id,
        "sessionsRevoked": revoked,
    })))
}

/// v1.1-P2.10：写 admin 用户管理审计。同步入 DB，失败仅 warn，不阻塞主响应。
fn write_user_admin_audit(
    state: &AppState,
    admin_id: &str,
    action: &str,
    user_id: &str,
    metadata: serde_json::Value,
) {
    let meta = if metadata.is_null() {
        None
    } else {
        Some(&metadata)
    };
    if let Err(e) = state
        .store()
        .insert_admin_audit(admin_id, action, Some("user"), Some(user_id), meta)
    {
        tracing::warn!(error=%e, action=%action, "写 admin audit 失败（不影响主流程）");
    }
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

// ─────────────── m022:用户档案 / 会话 / 批量 ban ───────────────

/// GET /api/admin/users/:id/profile —— 用户答题聚合(总记录数 / 准确率 /
/// 词库分布)。供 UserManagementPage Drawer "答题档案" 区块。
async fn admin_user_profile(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = id.clone();
    let profile = blocking::run_blocking(
        "admin.user_profile",
        move || -> Result<_, crate::store::StoreError> {
            let user_exists = state.store().get_user_by_id(&user_id)?.is_some();
            if !user_exists {
                return Ok(None);
            }
            let conn = state.store().conn()?;
            // 总记录数 + 正确数
            let (total, correct): (i64, i64) = conn.query_row(
                "SELECT COUNT(*), COALESCE(SUM(CASE WHEN is_correct = 1 THEN 1 ELSE 0 END), 0)
                 FROM learning_records WHERE user_id = ?1",
                rusqlite::params![user_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            // 词库分布(top 10)
            let mut stmt = conn.prepare(
                "SELECT wb.id, wb.name, COUNT(*) AS cnt
                 FROM learning_records lr
                 JOIN wordbook_words ww ON ww.word_id = lr.word_id
                 JOIN wordbooks wb ON wb.id = ww.wordbook_id
                 WHERE lr.user_id = ?1
                 GROUP BY wb.id, wb.name
                 ORDER BY cnt DESC
                 LIMIT 10",
            )?;
            let distribution: Vec<serde_json::Value> = stmt
                .query_map(rusqlite::params![user_id], |r| {
                    Ok(serde_json::json!({
                        "wordbookId": r.get::<_, String>(0)?,
                        "name": r.get::<_, String>(1)?,
                        "recordCount": r.get::<_, i64>(2)?,
                    }))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let mut session_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM learning_sessions WHERE user_id = ?1",
                rusqlite::params![user_id],
                |r| r.get(0),
            )?;
            // 兜底:如果 session 表不存在或 0,继续
            if session_count < 0 {
                session_count = 0;
            }
            Ok(Some(serde_json::json!({
                "userId": user_id,
                "totalRecords": total,
                "correctRecords": correct,
                "accuracy": if total > 0 { Some(correct as f64 / total as f64) } else { None::<f64> },
                "sessionCount": session_count,
                "wordbookDistribution": distribution,
            })))
        },
    )
    .await??;

    match profile {
        Some(p) => Ok(ok(p)),
        None => Err(AppError::not_found("用户不存在")),
    }
}

/// GET /api/admin/users/:id/sessions?limit=20 —— 用户最近 N 个 learning session。
#[derive(Debug, Deserialize)]
struct SessionsQuery {
    #[serde(default = "default_session_limit")]
    limit: u32,
}
fn default_session_limit() -> u32 {
    20
}

async fn admin_user_sessions(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    Query(q): Query<SessionsQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit.clamp(1, 200);
    let user_id = id;
    let rows = blocking::run_blocking(
        "admin.user_sessions",
        move || -> Result<Vec<serde_json::Value>, crate::store::StoreError> {
            let conn = state.store().conn()?;
            let mut stmt = conn.prepare(
                "SELECT id, status, created_at, completed_at, words_count, correct_count
                 FROM learning_sessions
                 WHERE user_id = ?1
                 ORDER BY created_at DESC
                 LIMIT ?2",
            )?;
            let rows: Result<Vec<serde_json::Value>, _> = stmt
                .query_map(rusqlite::params![user_id, limit as i64], |r| {
                    let total: Option<i64> = r.get(4)?;
                    let correct: Option<i64> = r.get(5)?;
                    Ok(serde_json::json!({
                        "id": r.get::<_, String>(0)?,
                        "status": r.get::<_, String>(1)?,
                        "createdAt": r.get::<_, String>(2)?,
                        "completedAt": r.get::<_, Option<String>>(3)?,
                        "wordsCount": total,
                        "correctCount": correct,
                        "accuracy": match (total, correct) {
                            (Some(t), Some(c)) if t > 0 => Some(c as f64 / t as f64),
                            _ => None,
                        },
                    }))
                })?
                .collect();
            Ok(rows?)
        },
    )
    .await??;

    Ok(ok(serde_json::json!({ "sessions": rows })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BulkUserIdsRequest {
    user_ids: Vec<String>,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BulkResultEntry {
    user_id: String,
    success: bool,
    error: Option<String>,
}

/// POST /api/admin/users/bulk-ban —— 批量封禁。每个用户独立执行,部分失败返回个体 result。
async fn admin_users_bulk_ban(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<BulkUserIdsRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if req.user_ids.is_empty() || req.user_ids.len() > 200 {
        return Err(AppError::bad_request(
            "BULK_SIZE",
            "userIds 数量需在 1..=200 之间",
        ));
    }
    let mut results = Vec::with_capacity(req.user_ids.len());
    for uid in &req.user_ids {
        let r = admin_ban_user(&state, uid.clone()).await;
        match r {
            Ok(revoked) => {
                results.push(BulkResultEntry {
                    user_id: uid.clone(),
                    success: true,
                    error: None,
                });
                write_user_admin_audit(
                    &state,
                    &admin.admin_id,
                    "user.bulk_ban",
                    uid,
                    serde_json::json!({"sessionsRevoked": revoked, "reason": req.reason}),
                );
            }
            Err(e) => results.push(BulkResultEntry {
                user_id: uid.clone(),
                success: false,
                error: Some(e.message.clone()),
            }),
        }
    }
    let succeeded = results.iter().filter(|r| r.success).count();
    Ok(ok(serde_json::json!({
        "total": req.user_ids.len(),
        "succeeded": succeeded,
        "failed": req.user_ids.len() - succeeded,
        "results": results,
    })))
}

/// POST /api/admin/users/bulk-unban
async fn admin_users_bulk_unban(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<BulkUserIdsRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if req.user_ids.is_empty() || req.user_ids.len() > 200 {
        return Err(AppError::bad_request(
            "BULK_SIZE",
            "userIds 数量需在 1..=200 之间",
        ));
    }
    let mut results = Vec::with_capacity(req.user_ids.len());
    for uid in &req.user_ids {
        let r = admin_unban_user(&state, uid.clone()).await;
        match r {
            Ok(()) => {
                results.push(BulkResultEntry {
                    user_id: uid.clone(),
                    success: true,
                    error: None,
                });
                write_user_admin_audit(
                    &state,
                    &admin.admin_id,
                    "user.bulk_unban",
                    uid,
                    serde_json::Value::Null,
                );
            }
            Err(e) => results.push(BulkResultEntry {
                user_id: uid.clone(),
                success: false,
                error: Some(e.message.clone()),
            }),
        }
    }
    let succeeded = results.iter().filter(|r| r.success).count();
    Ok(ok(serde_json::json!({
        "total": req.user_ids.len(),
        "succeeded": succeeded,
        "failed": req.user_ids.len() - succeeded,
        "results": results,
    })))
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
