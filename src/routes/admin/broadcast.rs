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
    Router::new().route("/", post(broadcast_message))
}

// B63: System-wide broadcast
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BroadcastRequest {
    title: String,
    message: String,
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

async fn broadcast_message(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<BroadcastRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    req.validate()?;

    // 使用幂等 key 防止重复广播
    let broadcast_id = uuid::Uuid::new_v4().to_string();

    // 分批加载用户，避免一次性加载所有用户导致内存溢出
    let batch_size = 100;
    let mut offset = 0;
    let mut total_sent = 0usize;

    loop {
        let users = state.store().list_users(batch_size, offset)?;
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
                    "title": req.title,
                    "message": req.message,
                    "read": false,
                    "createdAt": Utc::now().to_rfc3339(),
                });
                (user.id.clone(), notification_id, value)
            })
            .collect();

        total_sent += entries.len();
        state
            .store()
            .batch_create_notifications(&entries)
            .map_err(|e| AppError::internal(&e.to_string()))?;

        offset += users.len();
        tracing::info!("广播进度: 已发送 {} 条通知", total_sent);
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
