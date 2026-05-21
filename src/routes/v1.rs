//! B79: V1 compatibility routes — **已冻结，全部返回 410 Gone（M0-C5）**
//!
//! ⚠️  **设计警告（P3#6）**：V1 路由刻意**绕过 AMAS 引擎**——
//!     - 所有端点自 2026-05-21 起停止处理业务逻辑，返回 410 Gone
//!     - 迁移文档：<https://docs.wordforge.app/api/v1-deprecation>
//!     - Sunset 时间：2027-01-01
//!
//!   现行客户端（iOS）一律调用 /api/* 主端点，无 v1 调用。

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;

use crate::middleware::deprecation::{
    make_deprecation_layer, V1_DEPRECATION_DATE, V1_LINK_URL, V1_SUNSET_DATE,
};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    // M0-C4：整个 /api/v1/* 路由器注入 Deprecation + Sunset header（RFC 8594）
    // M0-C5：所有 handler 返回 410 Gone，业务逻辑已冻结
    Router::new()
        .route("/words", get(gone))
        .route("/words/:id", get(gone))
        .route("/records", get(gone).post(gone))
        .route("/study-config", get(gone))
        .route("/learning/session", post(gone))
        .layer(axum::middleware::from_fn(make_deprecation_layer(
            V1_DEPRECATION_DATE,
            V1_SUNSET_DATE,
            Some(V1_LINK_URL),
        )))
}

/// M0-C5：统一 410 Gone 响应，所有 /api/v1/* 端点共用。
async fn gone() -> impl IntoResponse {
    (
        StatusCode::GONE,
        axum::Json(serde_json::json!({
            "error": "GONE",
            "message": "/api/v1/* 端点已停止维护，请迁移至 /api/* 主端点",
            "sunset": V1_SUNSET_DATE,
            "migrationDoc": V1_LINK_URL,
        })),
    )
}
