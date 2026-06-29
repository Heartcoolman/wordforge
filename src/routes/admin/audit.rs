//! P1：全局 admin 审计端点 `GET /api/admin/audit-log`。
//!
//! 读取 `update_audit_log` 表（注意表名是 `update_audit_log`，非 `admin_audit_log`），
//! 返回全表的 admin 敏感操作审计，支持按 adminId/action/targetType/targetId/时间窗过滤 + 分页。
//! 区别于已有的「升级历史」（仅 self_update/rollback）与「按目标审计」（单一 target）。

use axum::extract::{Query, State};
use serde::Deserialize;
use serde_json::json;

use crate::auth::AdminAuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::operations::update_audit::AdminAuditFilter;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AuditLogQuery {
    admin_id: Option<String>,
    action: Option<String>,
    target_type: Option<String>,
    target_id: Option<String>,
    /// 起始时间（RFC3339，含）
    since: Option<String>,
    /// 截止时间（RFC3339，含）
    until: Option<String>,
    page: Option<u64>,
    per_page: Option<u64>,
}

/// 把空白/空串归一化为 None，避免空过滤条件被拼进 WHERE。
fn clean(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// GET /api/admin/audit-log —— 全局 admin 审计列表（倒序分页）。
pub(crate) async fn list_global_audit(
    _admin: AdminAuthUser,
    Query(q): Query<AuditLogQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let limit = per_page as i64;
    let offset = ((page - 1) * per_page) as i64;

    let filter = AdminAuditFilter {
        admin_id: clean(q.admin_id),
        action: clean(q.action),
        target_type: clean(q.target_type),
        target_id: clean(q.target_id),
        since: clean(q.since),
        until: clean(q.until),
    };

    let (items, total) = state
        .run_store_task("admin.audit_log.list_global", move |store| {
            store.list_admin_audit_global(&filter, limit, offset)
        })
        .await
        .map_err(|e| AppError::internal(&e.to_string()))?
        .map_err(AppError::from)?;

    Ok(ok(json!({
        "items": items,
        "total": total,
        "page": page,
        "perPage": per_page,
    })))
}
