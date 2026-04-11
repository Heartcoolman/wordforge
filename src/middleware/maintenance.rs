use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::state::AppState;

pub async fn maintenance_middleware(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    if state.is_maintenance() {
        let path = req.uri().path();
        let exempt = path.starts_with("/api/admin/")
            || path == "/api/status"
            || path.starts_with("/api/realtime/")
            || path == "/api/telemetry"
            || path.starts_with("/health");

        if !exempt {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                axum::Json(serde_json::json!({
                    "code": "MAINTENANCE",
                    "message": "服务器维护中，请稍后重试"
                })),
            )
                .into_response();
        }
    }
    next.run(req).await
}
