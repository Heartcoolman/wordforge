//! D1 admin 应用内通知/告警收件箱。
//! 数据源是 m037 system_alerts(AMAS 软拦截告警的可写载体),m041 加 read_at/acked_by
//! 作已读/确认状态。**绝不复用 end-user notifications 表(按 user_id 键控,维度不同)**。
//! 取最近 N 条(每页 INBOX_LIMIT) + 未读计数;支持 ?offset= 翻页覆盖全部未读(防截断项无法标记已读);
//! 标记已读经 POST /:id/read 持久化。

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use serde_json::json;

use crate::auth::AdminAuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;

/// 收件箱单次返回的最大条数。
const INBOX_LIMIT: i64 = 100;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_notifications))
        .route("/:id/read", post(mark_read))
        .route("/read-all", post(mark_all_read))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    /// 仅返回未读告警。
    #[serde(default)]
    unread: bool,
    /// 分页偏移(默认 0,负数按 0 处理)。配合每页 INBOX_LIMIT 翻页可覆盖**全部**未读告警 ——
    /// 修复未读超 INBOX_LIMIT 时被截断项无法单条标记已读、计数与列表割裂的缺陷。
    #[serde(default)]
    offset: i64,
}

/// GET /api/admin/notifications?unread=bool&offset=N —— 收件箱列表(分页) + 未读计数。
/// data: { items: SystemAlert[], unreadCount: number, limit: number, offset: number }。
/// 返回 limit/offset 供前端翻页(offset+limit < unreadCount 时仍有下一页)。
async fn list_notifications(
    _admin: AdminAuthUser,
    Query(q): Query<ListQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let unread_only = q.unread;
    let offset = q.offset.max(0);
    let (items, unread_count) = state
        .run_store_task("admin.notifications.list", move |store| {
            let items = store.list_admin_alerts_paged(unread_only, INBOX_LIMIT, offset)?;
            let unread_count = store.count_unread_system_alerts()?;
            Ok::<_, crate::store::StoreError>((items, unread_count))
        })
        .await??;

    Ok(ok(
        json!({ "items": items, "unreadCount": unread_count, "limit": INBOX_LIMIT, "offset": offset }),
    ))
}

/// POST /api/admin/notifications/:id/read —— 标记单条已读(幂等)。
async fn mark_read(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let admin_id = admin.admin_id.clone();
    let hit = state
        .run_store_task("admin.notifications.mark_read", move |store| {
            store.mark_system_alert_read(&id, &admin_id)
        })
        .await??;
    if !hit {
        return Err(AppError::not_found("告警不存在"));
    }
    let unread_count = state
        .run_store_task("admin.notifications.unread_count", |store| {
            store.count_unread_system_alerts()
        })
        .await??;
    Ok(ok(json!({ "read": true, "unreadCount": unread_count })))
}

/// POST /api/admin/notifications/read-all —— 一键全部已读，返回重算后的未读计数。
async fn mark_all_read(
    admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let admin_id = admin.admin_id.clone();
    let (marked, unread_count) = state
        .run_store_task("admin.notifications.read_all", move |store| {
            let marked = store.mark_all_system_alerts_read(&admin_id)?;
            let unread_count = store.count_unread_system_alerts()?;
            Ok::<_, crate::store::StoreError>((marked, unread_count))
        })
        .await??;
    Ok(ok(json!({ "marked": marked, "unreadCount": unread_count })))
}
