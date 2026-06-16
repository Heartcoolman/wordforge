pub mod amas;
pub mod analytics;
pub mod auth;
pub mod broadcast;
pub mod clients;
pub mod feedback;
pub mod monitoring;
pub mod notifications;
pub mod probe;
pub mod probe_telemetry;
pub mod rbac;
pub mod resource_packs;
pub mod runtime_settings;
pub mod settings;
pub mod settings_sections;
pub mod updates;
pub mod wordbooks;

use axum::extract::{Path, Query, State};
use axum::routing::{get, patch, post};
use axum::Router;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::{hash_password, hash_token, AdminAuthUser};
use crate::blocking;
use crate::extractors::JsonBody;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::operations::users::{User, UserFacets, UserListFilter, UserStats};

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
    /// m025:推荐来源,NULL 表示无(admin 创建 / 老用户)
    referrer_source: Option<String>,
    /// m024:仅 list?includeStats=true 时填充
    #[serde(skip_serializing_if = "Option::is_none")]
    stats: Option<UserStats>,
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
            referrer_source: u.referrer_source.clone(),
            stats: None,
        }
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        // 注意：/auth 路由已移至 build_router 中单独挂载（附加专用速率限制）
        .nest("/analytics", analytics::router())
        .nest("/monitoring", monitoring::router())
        .nest("/notifications", notifications::router())
        .nest("/broadcast", broadcast::router())
        .route("/broadcast-update", post(broadcast::broadcast_update))
        .nest("/settings", settings::router())
        .nest("/wordbook-center", super::wordbook_center::admin_router())
        .nest("/wordbooks", wordbooks::router())
        .nest("/amas", amas::admin_router())
        .nest("/updates", updates::router())
        .nest("/clients", clients::router())
        .nest("/feedback", feedback::router())
        .nest("/probe", probe::router())
        .nest("/probe-telemetry", probe_telemetry::router())
        .nest("/resource-packs", resource_packs::router())
        .nest("/telemetry", clients::telemetry_router())
        .route("/users", get(list_users).post(admin_create_user))
        // 用户管理页顶部筛选 chip 计数(全部/活跃/7天未登录/禁用/管理员)
        .route("/users/facets", get(list_user_facets))
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
        // m024:角色变更
        .route("/users/:id/role", patch(admin_patch_user_role))
        .route("/users/bulk-role", post(admin_users_bulk_role))
        // m024:用户档案 Drawer 数据源
        .route("/users/:id/devices", get(admin_user_devices))
        .route("/users/:id/audit-log", get(admin_user_audit_log))
        // m025:用户档案深化数据源
        .route("/users/:id/extras", get(admin_user_extras))
        .route("/users/:id/activity-log", get(admin_user_activity_log))
        // m026:批量操作 + 设备封禁
        .route(
            "/users/bulk-reset-password",
            post(admin_users_bulk_reset_password),
        )
        .route("/users/bulk-delete", post(admin_users_bulk_delete))
        .route(
            "/users/:id/devices/:device_id/ban",
            post(admin_user_device_ban),
        )
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
    /// m024:'user' / 'staff' / 'admin'
    role: Option<String>,
    /// m024:'active' / 'inactive' / 'suspended'
    status: Option<String>,
    /// m024:最近 N 天未登录(含从未登录)
    inactive_days: Option<u32>,
    /// m024:true 时 list 返回每个 user 的答题聚合(recordCount/accuracy/last20)
    #[serde(default)]
    include_stats: bool,
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

    let filter = UserListFilter {
        search,
        banned: q.banned,
        role: q
            .role
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        status: q
            .status
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        inactive_days: q.inactive_days,
    };

    let (users, total, stats_map) =
        admin_list_users(&state, page, per_page, filter, q.include_stats).await?;

    let safe_users: Vec<AdminUserView> = users
        .iter()
        .map(|u| {
            let mut v = AdminUserView::from(u);
            if let Some(map) = stats_map.as_ref() {
                v.stats = map.get(&u.id).cloned();
            }
            v
        })
        .collect();
    Ok(crate::response::paginated(
        safe_users, total, page, per_page,
    ))
}

/// GET /api/admin/users/facets —— 用户管理页顶部筛选 chip 的各类计数。
async fn list_user_facets(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let facets: UserFacets = state
        .run_store_task("admin.users.facets", |store| -> Result<_, AppError> {
            Ok(store.admin_user_facets()?)
        })
        .await??;
    Ok(ok(facets))
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

// m024:POST /api/admin/users —— 管理员创建用户(不走公开注册流程,
// 跳过 registration_enabled / max_users / 维护模式 等公共限制,但仍校验
// email / username / password 强度 + role 枚举 + email 唯一,并写审计)。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdminCreateUserRequest {
    email: String,
    username: String,
    password: String,
    /// 'user' / 'staff' / 'admin'。默认 'user'。
    #[serde(default)]
    role: Option<String>,
}

fn normalize_role(raw: Option<&str>) -> Result<String, AppError> {
    let v = raw.unwrap_or("user").trim();
    match v {
        "user" | "staff" | "admin" => Ok(v.to_string()),
        _ => Err(AppError::bad_request(
            "USER_INVALID_ROLE",
            "role 必须是 'user' / 'staff' / 'admin' 之一",
        )),
    }
}

async fn admin_create_user(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<AdminCreateUserRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let email = req.email.trim().to_lowercase();
    if !crate::validation::is_valid_email(&email) {
        return Err(AppError::bad_request("AUTH_INVALID_EMAIL", "邮箱格式无效"));
    }
    let username = req.username.trim().to_string();
    if let Err(msg) = crate::validation::validate_username(&username) {
        return Err(AppError::bad_request("AUTH_INVALID_USERNAME", msg));
    }
    if let Err(msg) = crate::validation::validate_password(&req.password) {
        return Err(AppError::bad_request("AUTH_WEAK_PASSWORD", msg));
    }
    let role = normalize_role(req.role.as_deref())?;
    let password = req.password;
    let email_for_lookup = email.clone();
    let role_for_task = role.clone();
    let user = state
        .run_store_task(
            "admin.create_user",
            move |store| -> Result<User, AppError> {
                if store.get_user_by_email(&email_for_lookup)?.is_some() {
                    return Err(AppError::conflict("AUTH_EMAIL_EXISTS", "该邮箱已被注册"));
                }
                let password_hash = hash_password(&password)?;
                let now = Utc::now();
                let user = User {
                    id: uuid::Uuid::new_v4().to_string(),
                    email: email_for_lookup,
                    username,
                    password_hash,
                    is_banned: false,
                    created_at: now,
                    updated_at: now,
                    failed_login_count: 0,
                    locked_until: None,
                    role: role_for_task,
                    status: "active".to_string(),
                    last_login_at: None,
                    // m025:admin 通道创建,无 referral source
                    referrer_source: None,
                };
                store.create_user(&user)?;
                Ok(user)
            },
        )
        .await??;

    tracing::info!(
        admin_id = %admin.admin_id,
        action = "create_user",
        target_user_id = %user.id,
        role = %user.role,
        "管理员创建用户"
    );
    write_user_admin_audit(
        &state,
        &admin.admin_id,
        "user.create",
        &user.id,
        serde_json::json!({ "role": user.role }),
    );

    Ok(crate::response::created(AdminUserView::from(&user)))
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
    if let Err(e) =
        state
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
    filter: UserListFilter,
    include_stats: bool,
) -> Result<
    (
        Vec<User>,
        u64,
        Option<std::collections::HashMap<String, UserStats>>,
    ),
    AppError,
> {
    let store = state.store().clone();
    let limit = per_page as usize;
    let offset = ((page - 1) * per_page) as usize;

    blocking::run_blocking("admin.list_users", move || {
        let (page_slice, total) = store.list_users_filtered(&filter, limit, offset)?;
        let stats_map = if include_stats && !page_slice.is_empty() {
            let ids: Vec<String> = page_slice.iter().map(|u| u.id.clone()).collect();
            Some(store.list_user_stats(&ids)?)
        } else {
            None
        };
        Ok::<_, crate::store::StoreError>((page_slice, total, stats_map))
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
            // 总记录数 + 正确数 + 平均反应时(ms)
            let (total, correct, avg_rt): (i64, i64, Option<f64>) = conn.query_row(
                "SELECT COUNT(*), \
                        COALESCE(SUM(CASE WHEN is_correct = 1 THEN 1 ELSE 0 END), 0), \
                        AVG(NULLIF(response_time_ms, 0))
                 FROM learning_records WHERE user_id = ?1",
                rusqlite::params![user_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
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
                // m026:平均反应时(ms),设计图 Drawer "答题档案" 第三个 metric
                "avgResponseTimeMs": avg_rt,
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
            // learning_sessions 无 completed_at/words_count 列：用 updated_at 作完成时间
            // (仅 completed 状态有意义)、total_count 作题量，保持前端 JSON 契约不变。
            let mut stmt = conn.prepare(
                "SELECT id, status, created_at, updated_at, total_count, correct_count
                 FROM learning_sessions
                 WHERE user_id = ?1
                 ORDER BY created_at DESC
                 LIMIT ?2",
            )?;
            let rows: Result<Vec<serde_json::Value>, _> = stmt
                .query_map(rusqlite::params![user_id, limit as i64], |r| {
                    let status: String = r.get(1)?;
                    let updated_at: String = r.get(3)?;
                    let total: Option<i64> = r.get(4)?;
                    let correct: Option<i64> = r.get(5)?;
                    let completed_at = (status == "completed").then_some(updated_at);
                    Ok(serde_json::json!({
                        "id": r.get::<_, String>(0)?,
                        "status": status,
                        "createdAt": r.get::<_, String>(2)?,
                        "completedAt": completed_at,
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
    let results =
        admin_bulk_set_banned(&state, &admin.admin_id, &req, true, "user.bulk_ban").await?;
    let succeeded = results.iter().filter(|r| r.success).count();
    Ok(ok(serde_json::json!({
        "total": req.user_ids.len(),
        "succeeded": succeeded,
        "failed": req.user_ids.len() - succeeded,
        "results": results,
    })))
}

/// 批量封禁/解封共用:单 run_blocking + 单事务批量处理,返回保留原 per-user result 结构。
async fn admin_bulk_set_banned(
    state: &AppState,
    admin_id: &str,
    req: &BulkUserIdsRequest,
    banned: bool,
    action: &str,
) -> Result<Vec<BulkResultEntry>, AppError> {
    let store = state.store().clone();
    let ids = req.user_ids.clone();
    let admin_id = admin_id.to_string();
    let action = action.to_string();
    let reason = req.reason.clone();
    let affected = blocking::run_blocking("admin.bulk_set_banned", move || {
        if banned {
            store.batch_ban_users(&ids, &admin_id, &action, reason.as_deref())
        } else {
            store.batch_unban_users(&ids, &admin_id, &action, reason.as_deref())
        }
    })
    .await??;
    let affected: std::collections::HashSet<String> = affected.into_iter().collect();
    Ok(req
        .user_ids
        .iter()
        .map(|uid| {
            let success = affected.contains(uid);
            BulkResultEntry {
                user_id: uid.clone(),
                success,
                error: if success {
                    None
                } else {
                    Some("用户不存在".to_string())
                },
            }
        })
        .collect())
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
    let results =
        admin_bulk_set_banned(&state, &admin.admin_id, &req, false, "user.bulk_unban").await?;
    let succeeded = results.iter().filter(|r| r.success).count();
    Ok(ok(serde_json::json!({
        "total": req.user_ids.len(),
        "succeeded": succeeded,
        "failed": req.user_ids.len() - succeeded,
        "results": results,
    })))
}

// m024:PATCH /api/admin/users/:id/role —— 单条角色变更
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatchRoleRequest {
    role: String,
}

async fn admin_patch_user_role(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<PatchRoleRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let new_role = normalize_role(Some(&req.role))?;
    let store = state.store().clone();
    let user_id = id.clone();
    let role_for_task = new_role.clone();
    let updated: User = blocking::run_blocking(
        "admin.update_user_role",
        move || -> Result<User, AppError> {
            if store.get_user_by_id(&user_id)?.is_none() {
                return Err(AppError::not_found("用户不存在"));
            }
            store.update_user_role(&user_id, &role_for_task)?;
            let u = store
                .get_user_by_id(&user_id)?
                .ok_or_else(|| AppError::not_found("用户不存在"))?;
            Ok(u)
        },
    )
    .await??;

    tracing::info!(
        admin_id = %admin.admin_id,
        action = "patch_user_role",
        target_user_id = %id,
        new_role = %new_role,
        "管理员变更用户角色"
    );
    write_user_admin_audit(
        &state,
        &admin.admin_id,
        "user.set_role",
        &id,
        serde_json::json!({ "role": new_role }),
    );

    Ok(ok(AdminUserView::from(&updated)))
}

// m024:POST /api/admin/users/bulk-role —— 批量角色变更。复用 BulkResultEntry。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BulkRoleRequest {
    user_ids: Vec<String>,
    role: String,
}

async fn admin_users_bulk_role(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<BulkRoleRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if req.user_ids.is_empty() || req.user_ids.len() > 200 {
        return Err(AppError::bad_request(
            "BULK_SIZE",
            "userIds 数量需在 1..=200 之间",
        ));
    }
    let new_role = normalize_role(Some(&req.role))?;
    let mut results = Vec::with_capacity(req.user_ids.len());
    for uid in &req.user_ids {
        let store = state.store().clone();
        let uid_for_task = uid.clone();
        let role_for_task = new_role.clone();
        let r = blocking::run_blocking("admin.bulk_role", move || -> Result<(), AppError> {
            if store.get_user_by_id(&uid_for_task)?.is_none() {
                return Err(AppError::not_found("用户不存在"));
            }
            store.update_user_role(&uid_for_task, &role_for_task)?;
            Ok(())
        })
        .await?;
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
                    "user.bulk_set_role",
                    uid,
                    serde_json::json!({ "role": new_role }),
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
        "role": new_role,
    })))
}

// m024:GET /api/admin/users/:id/devices —— 该用户绑定的 client_devices 列表
// (按 last_seen_at 倒序)。供 Drawer "设备/会话" tab。
#[derive(Debug, Deserialize)]
struct DevicesQuery {
    #[serde(default = "default_devices_limit")]
    limit: u32,
}
fn default_devices_limit() -> u32 {
    20
}

async fn admin_user_devices(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    Query(q): Query<DevicesQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit.clamp(1, 100) as usize;
    let store = state.store().clone();
    let user_id = id.clone();
    let devices = blocking::run_blocking(
        "admin.user_devices",
        move || -> Result<_, crate::store::StoreError> {
            store.list_client_devices_for_user(&user_id, limit)
        },
    )
    .await??;
    // 脱敏:与 /clients/paginated 一致,用投影 DTO 不暴露 last_ip / banned_by / ban_reason
    let devices: Vec<clients::ListedDevice> = devices.into_iter().map(Into::into).collect();
    Ok(ok(serde_json::json!({ "devices": devices })))
}

// m024:GET /api/admin/users/:id/audit-log?limit=50 —— 该用户身上发生的 admin 操作
// 审计(insert_admin_audit 写入,target_type='user', target_id=:id)。
#[derive(Debug, Deserialize)]
struct AuditQuery {
    #[serde(default = "default_audit_limit")]
    limit: u32,
}
fn default_audit_limit() -> u32 {
    50
}

async fn admin_user_audit_log(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    Query(q): Query<AuditQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit.clamp(1, 200) as usize;
    let store = state.store().clone();
    let user_id = id.clone();
    let entries = blocking::run_blocking(
        "admin.user_audit_log",
        move || -> Result<_, crate::store::StoreError> {
            store.list_admin_audit_for_target("user", &user_id, limit)
        },
    )
    .await??;
    Ok(ok(serde_json::json!({ "entries": entries })))
}

// m025:GET /api/admin/users/:id/extras —— 一次性聚合 5 类(preferences /
// elo / habit / latest AMAS strategy / latest device telemetry)+ 3 衍生
// 指标(level / streakDays / accuracy7d / fatigueAlertCount7d)。供 Drawer
// "资料" 与 "答题档案" tab 的深化字段。
async fn admin_user_extras(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let store = state.store().clone();
    let user_id = id.clone();
    let payload = blocking::run_blocking(
        "admin.user_extras",
        move || -> Result<serde_json::Value, crate::store::StoreError> {
            let conn = store.conn()?;
            // 用户存在性
            let exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM users WHERE id = ?1)",
                rusqlite::params![user_id],
                |r| r.get(0),
            )?;
            if !exists {
                return Ok(serde_json::Value::Null);
            }

            // 1) preferences(可能未初始化)
            let preferences: Option<serde_json::Value> = conn
                .query_row(
                    "SELECT theme, language, notification_enabled, sound_enabled
                     FROM user_preferences WHERE user_id = ?1",
                    rusqlite::params![user_id],
                    |r| {
                        Ok(serde_json::json!({
                            "theme": r.get::<_, String>(0)?,
                            "language": r.get::<_, String>(1)?,
                            "notificationEnabled": r.get::<_, i64>(2)? != 0,
                            "soundEnabled": r.get::<_, i64>(3)? != 0,
                        }))
                    },
                )
                .ok();

            // 2) elo + 衍生 level
            let elo: Option<serde_json::Value> = conn
                .query_row(
                    "SELECT rating, sigma, games FROM user_elo WHERE user_id = ?1",
                    rusqlite::params![user_id],
                    |r| {
                        let rating: f64 = r.get(0)?;
                        let sigma: f64 = r.get(1)?;
                        let games: i64 = r.get(2)?;
                        // 简单 level 衍生:每 100 局 1 级,上限 50
                        let level = (games / 100).clamp(0, 50);
                        Ok(serde_json::json!({
                            "rating": rating,
                            "sigma": sigma,
                            "games": games,
                            "level": level,
                        }))
                    },
                )
                .ok();

            // 3) habit(可能未初始化)
            let habit: Option<serde_json::Value> = conn
                .query_row(
                    "SELECT daily_goal_words, daily_goal_minutes, sessions_per_day,
                            median_session_length_mins, temporal_total_sessions
                     FROM habit_profiles WHERE user_id = ?1",
                    rusqlite::params![user_id],
                    |r| {
                        Ok(serde_json::json!({
                            "dailyGoalWords": r.get::<_, i64>(0)?,
                            "dailyGoalMinutes": r.get::<_, i64>(1)?,
                            "sessionsPerDay": r.get::<_, f64>(2)?,
                            "medianSessionMins": r.get::<_, f64>(3)?,
                            "totalSessions": r.get::<_, i64>(4)?,
                        }))
                    },
                )
                .ok();

            // 4) streakDays 衍生:从 today 向前倒数连续答题日
            let recent_days: Vec<String> = {
                let mut stmt = conn.prepare(
                    "SELECT DISTINCT DATE(created_at) AS d
                     FROM learning_records WHERE user_id = ?1
                     ORDER BY d DESC LIMIT 60",
                )?;
                let rows: Vec<String> = stmt
                    .query_map(rusqlite::params![user_id], |r| r.get::<_, String>(0))?
                    .collect::<Result<Vec<_>, _>>()?;
                rows
            };
            let streak_days: i64 = {
                let today = chrono::Utc::now().date_naive();
                let mut count = 0i64;
                for (i, d) in recent_days.iter().enumerate() {
                    let parsed = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok();
                    match parsed {
                        Some(date) if date == today - chrono::Duration::days(i as i64) => {
                            count += 1
                        }
                        _ => break,
                    }
                }
                count
            };

            // 5) latest AMAS strategy(若有)
            let latest_strategy: Option<serde_json::Value> = conn
                .query_row(
                    "SELECT strategy_json, timestamp FROM engine_monitoring_events
                     WHERE user_id = ?1 ORDER BY timestamp DESC LIMIT 1",
                    rusqlite::params![user_id],
                    |r| {
                        let raw: String = r.get(0)?;
                        let ts: String = r.get(1)?;
                        Ok(serde_json::json!({
                            "strategy": serde_json::from_str::<serde_json::Value>(&raw)
                                .unwrap_or(serde_json::Value::Null),
                            "at": ts,
                        }))
                    },
                )
                .ok();

            // 6) latest telemetry device(若有)
            let latest_device: Option<serde_json::Value> = conn
                .query_row(
                    "SELECT os_name, browser_name, browser_version, timezone, language
                     FROM telemetry_summaries
                     WHERE user_id = ?1 ORDER BY server_ts DESC LIMIT 1",
                    rusqlite::params![user_id],
                    |r| {
                        Ok(serde_json::json!({
                            "osName": r.get::<_, Option<String>>(0)?,
                            "browserName": r.get::<_, Option<String>>(1)?,
                            "browserVersion": r.get::<_, Option<String>>(2)?,
                            "timezone": r.get::<_, Option<String>>(3)?,
                            "language": r.get::<_, Option<String>>(4)?,
                        }))
                    },
                )
                .ok();

            // m026:已选词书(JOIN study_configs.selected_wordbook_ids_json × wordbooks)
            let selected_wordbooks: Vec<serde_json::Value> = {
                let ids_json: Option<String> = conn
                    .query_row(
                        "SELECT selected_wordbook_ids_json FROM study_configs WHERE user_id = ?1",
                        rusqlite::params![user_id],
                        |r| r.get(0),
                    )
                    .ok();
                let ids: Vec<String> = ids_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
                    .unwrap_or_default();
                if ids.is_empty() {
                    Vec::new()
                } else {
                    let placeholders = vec!["?"; ids.len()].join(",");
                    let sql =
                        format!("SELECT id, name FROM wordbooks WHERE id IN ({placeholders})");
                    let mut stmt = conn.prepare(&sql)?;
                    let rows: Vec<serde_json::Value> = stmt
                        .query_map(rusqlite::params_from_iter(ids.iter()), |r| {
                            Ok(serde_json::json!({
                                "id": r.get::<_, String>(0)?,
                                "name": r.get::<_, String>(1)?,
                            }))
                        })?
                        .collect::<Result<Vec<_>, _>>()?;
                    rows
                }
            };

            // 7) 7 日窗口指标:accuracy7d / fatigueAlertCount7d
            let (acc7d_records, acc7d_correct): (i64, i64) = conn.query_row(
                "SELECT COUNT(*), COALESCE(SUM(CASE WHEN is_correct=1 THEN 1 ELSE 0 END), 0)
                 FROM learning_records WHERE user_id = ?1
                   AND datetime(created_at) > datetime('now', '-7 days')",
                rusqlite::params![user_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            let fatigue_alert_count_7d: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM engine_monitoring_events
                     WHERE user_id = ?1
                       AND datetime(timestamp) > datetime('now', '-7 days')
                       AND user_state_fatigue > 0.6",
                    rusqlite::params![user_id],
                    |r| r.get(0),
                )
                .unwrap_or(0);

            Ok(serde_json::json!({
                "preferences": preferences,
                "elo": elo,
                "habit": habit,
                "streakDays": streak_days,
                "selectedWordbooks": selected_wordbooks,
                "latestStrategy": latest_strategy,
                "latestDevice": latest_device,
                "metrics7d": {
                    "records": acc7d_records,
                    "correct": acc7d_correct,
                    "accuracy": if acc7d_records > 0 {
                        Some(acc7d_correct as f64 / acc7d_records as f64)
                    } else { None },
                    "fatigueAlertCount": fatigue_alert_count_7d,
                },
            }))
        },
    )
    .await??;

    if payload.is_null() {
        return Err(AppError::not_found("用户不存在"));
    }
    Ok(ok(payload))
}

// m025:GET /api/admin/users/:id/activity-log —— 用户自有活动日志(login /
// session.complete / goal.update / fatigue.alert),区别于 audit-log = admin 操作。
#[derive(Debug, Deserialize)]
struct ActivityQuery {
    #[serde(default = "default_activity_limit")]
    limit: u32,
}
fn default_activity_limit() -> u32 {
    50
}

async fn admin_user_activity_log(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    Query(q): Query<ActivityQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit.clamp(1, 200) as usize;
    let store = state.store().clone();
    let user_id = id.clone();
    let entries = blocking::run_blocking(
        "admin.user_activity_log",
        move || -> Result<_, crate::store::StoreError> {
            store.list_user_activity(&user_id, limit)
        },
    )
    .await??;
    Ok(ok(serde_json::json!({ "entries": entries })))
}

// m026:POST /api/admin/users/bulk-reset-password —— 批量生成密钥(每条独立)。
// 安全考虑:不在响应里回写密钥明文(密钥应由 admin 单条 GET 后转发);
// 这里仅返回成功/失败计数,逐条创建密钥落库。
async fn admin_users_bulk_reset_password(
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
        let r = admin_create_password_reset(&state, uid.clone()).await;
        match r {
            Ok(reset) => {
                results.push(BulkResultEntry {
                    user_id: uid.clone(),
                    success: true,
                    error: None,
                });
                write_user_admin_audit(
                    &state,
                    &admin.admin_id,
                    "user.bulk_reset_password",
                    uid,
                    serde_json::json!({ "expiresInHours": reset.expires_in_hours }),
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

// m026:POST /api/admin/users/bulk-delete —— 批量删除(级联清理 USER_SCOPED_TABLES)。
// 危险操作,需要 reason 字段做强 audit。
async fn admin_users_bulk_delete(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<BulkUserIdsRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if req.user_ids.is_empty() || req.user_ids.len() > 50 {
        return Err(AppError::bad_request("BULK_SIZE", "删除批量上限 50"));
    }
    let reason = req.reason.as_deref().unwrap_or("(未填写)").to_string();
    let mut results = Vec::with_capacity(req.user_ids.len());
    for uid in &req.user_ids {
        let store = state.store().clone();
        let uid_for_task = uid.clone();
        let r = blocking::run_blocking("admin.bulk_delete", move || -> Result<(), AppError> {
            if store.get_user_by_id(&uid_for_task)?.is_none() {
                return Err(AppError::not_found("用户不存在"));
            }
            store.delete_user(&uid_for_task)?;
            Ok(())
        })
        .await?;
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
                    "user.bulk_delete",
                    uid,
                    serde_json::json!({ "reason": &reason }),
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

// m026:POST /api/admin/users/:id/devices/:device_id/ban —— 设备级封禁
// (复用 ban_client_device + 撤销该 user 的全部 sessions 作为兜底)。
async fn admin_user_device_ban(
    admin: AdminAuthUser,
    Path((user_id, device_id)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let store = state.store().clone();
    let admin_id_for_audit = admin.admin_id.clone();
    let device_id_for_task = device_id.clone();
    let user_id_for_task = user_id.clone();
    let banned = blocking::run_blocking(
        "admin.user_device_ban",
        move || -> Result<bool, AppError> {
            // 验设备存在且归属该 user(防止跨 user 误操作)
            let conn = store.conn()?;
            let owner: Option<Option<String>> = conn
                .query_row(
                    "SELECT user_id FROM client_devices WHERE device_id = ?1",
                    rusqlite::params![&device_id_for_task],
                    |r| r.get(0),
                )
                .ok();
            match owner {
                Some(Some(uid)) if uid == user_id_for_task => {}
                Some(_) => {
                    return Err(AppError::bad_request(
                        "DEVICE_USER_MISMATCH",
                        "该设备不归属此用户",
                    ))
                }
                None => return Err(AppError::not_found("设备不存在")),
            }
            let ok = store.ban_client_device(
                &device_id_for_task,
                &admin_id_for_audit,
                Some("admin user-detail panel"),
            )?;
            // 兜底:撤销该 user 所有 session(不区分 device,目前 sessions 表无 device 列)
            store.delete_user_sessions(&user_id_for_task).ok();
            Ok(ok)
        },
    )
    .await??;
    write_user_admin_audit(
        &state,
        &admin.admin_id,
        "user.device_ban",
        &user_id,
        serde_json::json!({ "deviceId": &device_id, "applied": banned }),
    );
    Ok(ok(
        serde_json::json!({ "banned": banned, "deviceId": device_id }),
    ))
}

async fn admin_do_set_user_password(
    state: &AppState,
    user_id: String,
    new_password: String,
) -> Result<u32, AppError> {
    let store = state.store().clone();
    blocking::run_blocking("admin.set_user_password", move || -> Result<_, AppError> {
        let mut user = store
            .get_user_by_id(&user_id)?
            .ok_or_else(|| AppError::not_found("用户不存在"))?;

        user.password_hash = hash_password(&new_password)?;
        user.updated_at = Utc::now();
        store.update_user(&user)?;

        Ok(store.delete_user_sessions(&user_id)?)
    })
    .await?
}
