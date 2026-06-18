//! 全系统数据全量导出接口（仅超级管理员）。
//!
//! `GET /api/admin/system/export` —— 把 SQLite 全部用户表逐行以 NDJSON 流式导出。
//! 每行格式：`{"table":"<name>","data":<row>}\n`；首行为 `_meta`。
//! ⚠️ 原样导出，含全部凭证哈希，产物属高敏感物。

use std::convert::Infallible;

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use chrono::Utc;

use crate::auth::AdminAuthUser;
use crate::response::AppError;
use crate::routes::admin::rbac::require_super_admin;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/export", get(export_all))
}

/// GET /api/admin/system/export
async fn export_all(
    admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<Response<Body>, AppError> {
    require_super_admin(&state, &admin.admin_id).await?; // 仅超管
    tracing::warn!(admin_id = %admin.admin_id, "全系统数据全量导出被调用");

    // 表清单写入首行元数据，使导出产物自描述（空表也能被消费端感知）。
    let tables = state
        .run_store_task("admin.export.tables", |store| store.dump_table_names())
        .await??;

    // 真流式 NDJSON：spawn_blocking 内持单连接遍历整库，逐行经 channel 推出。
    // channel 容量 8 提供背压；客户端断连时 blocking_send 返回 Err → 提前停止。
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, Infallible>>(8);
    let store = state.store().clone();
    tokio::task::spawn_blocking(move || {
        let meta = serde_json::json!({
            "table": "_meta",
            "data": { "exportedAt": Utc::now().to_rfc3339(), "tables": tables },
        });
        if tx
            .blocking_send(Ok(Bytes::from(meta.to_string() + "\n")))
            .is_err()
        {
            return; // 客户端在拿到首行前已断连
        }

        let res = store.stream_full_dump(|table, row| {
            let line = serde_json::json!({ "table": table, "data": row }).to_string();
            tx.blocking_send(Ok(Bytes::from(line + "\n"))).is_ok()
        });
        if let Err(err) = res {
            let line = serde_json::json!({ "table": "_error", "data": err.to_string() }).to_string();
            let _ = tx.blocking_send(Ok(Bytes::from(line + "\n")));
        }
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-ndjson")
        .header(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment; filename=\"wordforge-system-export.ndjson\""),
        )
        .body(Body::from_stream(stream))
        .map_err(|e| AppError::internal(&e.to_string()))
}
