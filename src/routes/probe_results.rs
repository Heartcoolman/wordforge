//! 客户端回传探针结果：`POST /api/probe/results`。
//!
//! 校验链：AuthUser → x-device-id → request_id 在 probe_executions 中存在且
//! `device_id` 匹配（防一个用户回传另一个用户的 probe 结果）→ 推进 status →
//! 通过 ProbeService.publish_result 把结果广播给 admin REPL SSE 流。
//!
//! Body limit 320KB（与全局 64KB 不同）：result_json 截断阈 256KB + stderr +
//! 字段元数据，留余量。

use axum::extract::{DefaultBodyLimit, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;

use crate::auth::AuthUser;
use crate::extractors::JsonBody;
use crate::response::{ok, AppError};
use crate::services::probe::{ConfirmTicket, ProbeResultPayload, CONFIRM_TTL_SECS};
use crate::state::AppState;
use crate::store::operations::probe::ProbeStatusUpdate;

const MAX_RESULT_BODY: usize = 320 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/results", post(submit_result))
        .layer(DefaultBodyLimit::max(MAX_RESULT_BODY))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResultBody {
    request_id: String,
    /// 'ok' | 'error' | 'timeout' | 'confirm_required' | 'unsupported_ctx_version'
    status: String,
    #[serde(default)]
    result_json: Option<serde_json::Value>,
    #[serde(default)]
    stderr: Option<String>,
    duration_ms: u32,
    #[serde(default)]
    truncated: bool,
    #[serde(default)]
    confirm_token: Option<String>,
}

async fn submit_result(
    State(state): State<AppState>,
    _auth: AuthUser,
    headers: HeaderMap,
    JsonBody(body): JsonBody<ResultBody>,
) -> Result<impl IntoResponse, AppError> {
    let device_id = headers
        .get("x-device-id")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("MISSING_DEVICE_ID", "缺少 X-Device-Id 请求头"))?
        .to_string();

    // ── 1. 校验 request_id 存在 & device 匹配 ──
    let request_id = body.request_id.clone();
    let device_id_for_lookup = device_id.clone();
    let store = state.store().clone();
    let row = tokio::task::spawn_blocking(move || store.get_probe_execution(&request_id))
        .await
        .map_err(|e| AppError::internal(&format!("spawn_blocking: {e}")))?
        .map_err(|e| AppError::internal(&format!("get_probe_execution: {e}")))?
        .ok_or_else(|| AppError::not_found("probe request 不存在"))?;
    if row.device_id != device_id_for_lookup {
        return Err(AppError::forbidden("device_id 与 probe request 不匹配"));
    }
    // 已经是终态 → 拒绝重复回传（幂等保护）
    if matches!(
        row.status.as_str(),
        "ok" | "error" | "timeout" | "expired" | "unsupported_ctx_version" | "offline"
    ) {
        return Err(AppError::bad_request(
            "PROBE_ALREADY_COMPLETED",
            "该 probe request 已处于终态",
        ));
    }

    // ── 2. 协议层 status → 表层 status ──
    // confirm_required 是 M3 才会真正用到的入口，M1 也接受但 status 转 confirm_pending。
    let table_status = match body.status.as_str() {
        "ok" => "ok",
        "error" => "error",
        "timeout" => "timeout",
        "confirm_required" => "confirm_pending",
        "unsupported_ctx_version" => "unsupported_ctx_version",
        other => {
            return Err(AppError::bad_request(
                "INVALID_PROBE_STATUS",
                &format!("未知 status: {other}"),
            ));
        }
    };
    let is_terminal = !matches!(table_status, "confirm_pending");

    // ── 3. 持久化（result_json 落 TEXT 列，原样字符串） ──
    let result_str = body
        .result_json
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default());
    let completed_at = if is_terminal {
        Some(chrono::Utc::now().to_rfc3339())
    } else {
        None
    };
    let stderr_owned = body.stderr.clone();
    let result_str_clone = result_str.clone();
    let store2 = state.store().clone();
    let req_id_for_upd = body.request_id.clone();
    let upd_status = table_status.to_string();
    let duration_ms = body.duration_ms;
    let truncated = body.truncated;
    tokio::task::spawn_blocking(move || {
        store2.update_probe_status(
            &req_id_for_upd,
            &ProbeStatusUpdate {
                status: &upd_status,
                result_json: result_str_clone.as_deref(),
                stderr: stderr_owned.as_deref(),
                duration_ms: Some(duration_ms),
                truncated,
                completed_at: completed_at.as_deref(),
                confirmed_at: None,
            },
        )
    })
    .await
    .map_err(|e| AppError::internal(&format!("spawn_blocking: {e}")))?
    .map_err(|e| AppError::internal(&format!("update_probe_status: {e}")))?;

    // ── 4. confirm_required 路径：写 ticket，等待 admin 确认 ──
    if body.status == "confirm_required" {
        let token = body.confirm_token.clone().ok_or_else(|| {
            AppError::bad_request(
                "PROBE_CONFIRM_TOKEN_MISSING",
                "confirm_required 状态必须携带 confirmToken",
            )
        })?;
        state.probe_service().issue_confirm(
            &body.request_id,
            ConfirmTicket {
                token,
                device_id: row.device_id.clone(),
                expires_at: std::time::Instant::now()
                    + std::time::Duration::from_secs(CONFIRM_TTL_SECS),
            },
        );
    }

    // ── 5. 广播到 admin SSE 流 ──
    state.probe_service().publish_result(ProbeResultPayload {
        device_id: row.device_id.clone(),
        request_id: body.request_id.clone(),
        batch_id: row.batch_id.clone(),
        status: body.status.clone(),
        result_json: body.result_json,
        stderr: body.stderr,
        duration_ms: Some(body.duration_ms),
        truncated: body.truncated,
    });

    tracing::info!(
        request_id = %body.request_id,
        device_id = %row.device_id,
        status = %body.status,
        duration_ms = body.duration_ms,
        truncated = body.truncated,
        confirm_token_present = body.confirm_token.is_some(),
        "客户端回传探针结果"
    );

    Ok(ok(serde_json::json!({ "ok": true })))
}
