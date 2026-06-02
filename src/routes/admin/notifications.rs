//! D1 admin 应用内通知/告警收件箱。
//! 数据源是 m037 system_alerts(AMAS 软拦截告警的可写载体),m041 加 read_at/acked_by
//! 作已读/确认状态。**绝不复用 end-user notifications 表(按 user_id 键控,维度不同)**。
//! 收件箱无需分页,取最近 N 条 + 未读计数;标记已读经 POST /:id/read 持久化。

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
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    /// 仅返回未读告警。
    #[serde(default)]
    unread: bool,
}

/// GET /api/admin/notifications?unread=bool —— 收件箱列表 + 未读计数。
/// data: { items: SystemAlert[], unreadCount: number }。
async fn list_notifications(
    _admin: AdminAuthUser,
    Query(q): Query<ListQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let unread_only = q.unread;
    let (items, unread_count) = state
        .run_store_task("admin.notifications.list", move |store| {
            let items = store.list_admin_alerts(unread_only, INBOX_LIMIT)?;
            let unread_count = store.count_unread_system_alerts()?;
            Ok::<_, crate::store::StoreError>((items, unread_count))
        })
        .await??;

    Ok(ok(json!({ "items": items, "unreadCount": unread_count })))
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
