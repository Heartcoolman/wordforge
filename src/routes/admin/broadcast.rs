use axum::extract::State;
use axum::routing::post;
use axum::Router;

use crate::extractors::JsonBody;
use chrono::Utc;
use serde::Deserialize;

use crate::auth::AdminAuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(broadcast_message))
        // m027:受众预估命中端点 — 不持久化,纯查询,供 DevicesPage 广播 section 实时显示
        .route("/preview", post(broadcast_preview))
}

/// m027:广播受众预估命中。POST 接 `BroadcastAudience`,返 `{matched, total}`。
/// audience 为 None 时 matched = total(全员)。
async fn broadcast_preview(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<PreviewRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let (matched, total) = state
        .run_store_task(
            "admin.broadcast.preview",
            move |store| -> Result<_, AppError> {
                let total = store.count_users()? as i64;
                let matched = if let Some(aud) = req.audience {
                    match store.list_user_ids_for_audience(
                        &aud.platforms,
                        aud.version_min.as_deref(),
                        aud.last_active_days,
                        &aud.user_ids,
                    )? {
                        Some(ids) => ids.len() as i64,
                        None => total,
                    }
                } else {
                    total
                };
                Ok((matched, total))
            },
        )
        .await??;
    Ok(ok(serde_json::json!({ "matched": matched, "total": total })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewRequest {
    #[serde(default)]
    audience: Option<BroadcastAudience>,
}

// B63: System-wide broadcast
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BroadcastRequest {
    title: String,
    message: String,
    /// m027:可选受众条件。任意字段为空数组/None 都视为"该维度不过滤";
    /// 整个 audience 为 None 时走老的全员广播路径。
    #[serde(default)]
    audience: Option<BroadcastAudience>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BroadcastAudience {
    #[serde(default)]
    platforms: Vec<String>,
    #[serde(default)]
    version_min: Option<String>,
    #[serde(default)]
    last_active_days: Option<i64>,
    #[serde(default)]
    user_ids: Vec<String>,
}

impl BroadcastRequest {
    fn validate(&self) -> Result<(), AppError> {
        if self.title.is_empty() || self.title.len() > 200 {
            return Err(AppError::bad_request(
                "INVALID_TITLE",
                "标题长度需在1到200个字符之间",
            ));
        }
        if self.message.is_empty() || self.message.len() > 10000 {
            return Err(AppError::bad_request(
                "INVALID_MESSAGE",
                "消息内容长度需在1到10000个字符之间",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BroadcastUpdateRequest {
    version: Option<String>,
    message: Option<String>,
}

pub(crate) async fn broadcast_update(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<BroadcastUpdateRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    state.broadcast_update(req.version.as_deref(), req.message.as_deref());
    Ok(ok(serde_json::json!({ "broadcasted": true })))
}

async fn broadcast_message(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<BroadcastRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    req.validate()?;

    // 使用幂等 key 防止重复广播
    let broadcast_id = uuid::Uuid::new_v4().to_string();

    let title = req.title.clone();
    let message = req.message.clone();
    let mut total_sent = 0usize;

    // m027:audience 受众 filter。None 走老的全员路径,Some 把命中 user_id 一次性拉出。
    let target_user_ids: Option<Vec<String>> = if let Some(aud) = req.audience {
        state
            .run_store_task(
                "admin.broadcast.resolve_audience",
                move |store| -> Result<_, AppError> {
                    Ok(store.list_user_ids_for_audience(
                        &aud.platforms,
                        aud.version_min.as_deref(),
                        aud.last_active_days,
                        &aud.user_ids,
                    )?)
                },
            )
            .await??
    } else {
        None
    };

    if let Some(user_ids) = target_user_ids {
        // audience 路径:user_id list 分块写通知
        for chunk in user_ids.chunks(100) {
            let entries: Vec<(String, String, serde_json::Value)> = chunk
                .iter()
                .map(|uid| {
                    let notification_id = format!("{}_{}", broadcast_id, uid);
                    let value = serde_json::json!({
                        "id": notification_id,
                        "userId": uid,
                        "type": "broadcast",
                        "title": title,
                        "message": message,
                        "read": false,
                        "createdAt": Utc::now().to_rfc3339(),
                    });
                    (uid.clone(), notification_id, value)
                })
                .collect();
            total_sent += entries.len();
            state
                .run_store_task("admin.broadcast.persist", move |store| {
                    store
                        .batch_create_notifications(&entries)
                        .map_err(|e| AppError::internal(&e.to_string()))
                })
                .await??;
        }
        tracing::info!(
            audience = "filtered",
            matched = total_sent,
            "受众过滤广播完成"
        );
    } else {
        // 全员路径(老行为):分批加载 users → batch_create_notifications
        let batch_size = 100;
        let mut offset = 0;
        loop {
            let users = state
                .run_store_task("admin.broadcast.load_users", move |store| {
                    store.list_users(batch_size, offset)
                })
                .await??;
            if users.is_empty() {
                break;
            }

            let entries: Vec<(String, String, serde_json::Value)> = users
                .iter()
                .map(|user| {
                    let notification_id = format!("{}_{}", broadcast_id, user.id);
                    let value = serde_json::json!({
                        "id": notification_id,
                        "userId": user.id,
                        "type": "broadcast",
                        "title": title,
                        "message": message,
                        "read": false,
                        "createdAt": Utc::now().to_rfc3339(),
                    });
                    (user.id.clone(), notification_id, value)
                })
                .collect();

            total_sent += entries.len();
            state
                .run_store_task("admin.broadcast.persist", move |store| {
                    store
                        .batch_create_notifications(&entries)
                        .map_err(|e| AppError::internal(&e.to_string()))
                })
                .await??;

            offset += users.len();
            tracing::info!("广播进度: 已发送 {} 条通知", total_sent);
        }
    }

    tracing::info!(
        admin_id = %admin.admin_id,
        action = "broadcast",
        broadcast_id = %broadcast_id,
        total_sent = total_sent,
        "管理员发送系统广播"
    );

    Ok(ok(
        serde_json::json!({"sent": total_sent, "broadcastId": broadcast_id}),
    ))
}
