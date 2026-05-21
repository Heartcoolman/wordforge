use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::state::AppState;

/// M1-A6：豁免路由（admin / status / realtime / telemetry / v1）不加此中间件层，
/// 因此无需内部路径匹配，硬编码豁免列表已彻底消除。
pub async fn maintenance_middleware(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    if state.is_maintenance() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(serde_json::json!({
                "code": "MAINTENANCE",
                "message": "服务器维护中，请稍后重试"
            })),
        )
            .into_response();
    }
    next.run(req).await
}
