use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use chrono::Utc;
use serde::Deserialize;

use crate::auth::AdminAuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::keys;

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

async fn broadcast_message(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Json(req): Json<BroadcastRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let users = state.store().list_users(usize::MAX, 0)?;
    let mut sent = 0usize;

    for user in &users {
        let notification_id = uuid::Uuid::new_v4().to_string();
        let notification = serde_json::json!({
            "id": notification_id,
            "userId": user.id,
            "type": "broadcast",
            "title": req.title,
            "message": req.message,
            "read": false,
            "createdAt": Utc::now().to_rfc3339(),
        });

        let key = keys::notification_key(&user.id, &notification_id);
        state.store().notifications.insert(
            key.as_bytes(),
            serde_json::to_vec(&notification).map_err(|e| AppError::internal(&e.to_string()))?,
        ).map_err(|e| AppError::internal(&e.to_string()))?;
        sent += 1;
    }

    Ok(ok(serde_json::json!({"sent": sent})))
}
