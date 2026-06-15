//! 客户端回传探针结果：`POST /api/probe/results`。
//!
//! 校验链：AuthUser → x-device-id → 设备三态归属核验（owner 须为本人或未认领，
//! 防已认证用户冒充他人 device_id）→ request_id 在 probe_executions 中存在且
//! `device_id` 匹配（防一个用户回传另一个用户的 probe 结果）→ 以 CAS 推进 status
//! （WHERE status IN ('pending','confirm_pending')，并发 / sweeper 安全）→
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
use crate::store::StoreError;
use rusqlite::OptionalExtension;

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

/// CAS 事务成功推进后回传给 handler 后续步骤（confirm ticket / SSE 广播）的字段。
struct AdvancedProbe {
    device_id: String,
    batch_id: String,
}

async fn submit_result(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    JsonBody(body): JsonBody<ResultBody>,
) -> Result<impl IntoResponse, AppError> {
    let device_id = headers
        .get("x-device-id")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("MISSING_DEVICE_ID", "缺少 X-Device-Id 请求头"))?
        .to_string();

    // ── 1. 协议层 status → 表层 status（纯内存校验，先于任何 DB 操作） ──
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
    let is_confirm_required = body.status == "confirm_required";
    let is_terminal = !matches!(table_status, "confirm_pending");

    // confirm_required 必须携带 confirmToken（先于 DB 操作校验，避免无谓写库）。
    let confirm_token = if is_confirm_required {
        Some(body.confirm_token.clone().ok_or_else(|| {
            AppError::bad_request(
                "PROBE_CONFIRM_TOKEN_MISSING",
                "confirm_required 状态必须携带 confirmToken",
            )
        })?)
    } else {
        None
    };

    // result_json 落 TEXT 列，原样字符串序列化。
    let result_str = body
        .result_json
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default());
    let completed_at = if is_terminal {
        Some(chrono::Utc::now().to_rfc3339())
    } else {
        None
    };

    // ── 2. 单 IMMEDIATE 事务内做：设备三态归属核验 + 取 probe 行 + CAS 推进 ──
    // 把归属核验、存在性 / device 匹配、confirm 自环拒绝、幂等判定全部收进同一
    // BEGIN IMMEDIATE 事务，消除「先读后写」TOCTOU，与 sweeper / 并发回传安全。
    // 闭包返回 Ok(Ok(_)) 业务成功 / Ok(Err(AppError)) 业务拒绝 / Err(StoreError) 基础设施故障，
    // 三者在 handler 侧分别映射为 200 / 对应状态码 / 500，互不混淆。
    let store = state.store().clone();
    let request_id = body.request_id.clone();
    let device_id_for_tx = device_id.clone();
    let auth_user_id = auth.user_id.clone();
    let upd_status = table_status.to_string();
    let result_str_for_tx = result_str.clone();
    let stderr_for_tx = body.stderr.clone();
    let completed_at_for_tx = completed_at.clone();
    let duration_ms = body.duration_ms;
    let truncated = body.truncated;

    let tx_result = tokio::task::spawn_blocking(move || -> Result<Result<AdvancedProbe, AppError>, StoreError> {
        store.with_user_tx(|tx| {
            // 2a. 设备三态归属核验（与 telemetry handler 一致：防已认证用户冒充他人 device_id）。
            let owner: Option<Option<String>> = tx
                .query_row(
                    "SELECT user_id FROM client_devices WHERE device_id = ?1",
                    rusqlite::params![&device_id_for_tx],
                    |r| r.get::<_, Option<String>>(0),
                )
                .optional()?;
            match owner {
                // 设备未注册 → 拒绝（须先正常登录使用过再回传探针）。
                None => {
                    return Ok(Err(AppError {
                        status: axum::http::StatusCode::FORBIDDEN,
                        code: "DEVICE_NOT_REGISTERED".into(),
                        message: "设备未注册，无法回传探针结果".into(),
                        is_operational: true,
                    }));
                }
                // 已注册且归属他人 → 拒绝（防冒充）。Some(None)=未认领 / Some(me)=本人 → 放行。
                Some(Some(existing)) if existing != auth_user_id => {
                    return Ok(Err(AppError {
                        status: axum::http::StatusCode::FORBIDDEN,
                        code: "DEVICE_OWNERSHIP_MISMATCH".into(),
                        message: "设备归属与当前账号不符".into(),
                        is_operational: true,
                    }));
                }
                _ => {}
            }

            // 2b. 取 probe 行：存在性 + 协议层 device_id 与 probe request 匹配。
            let row: Option<(String, String, String)> = tx
                .query_row(
                    "SELECT device_id, batch_id, status FROM probe_executions WHERE id = ?1",
                    rusqlite::params![&request_id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let (row_device_id, batch_id, current_status) = match row {
                Some(v) => v,
                None => return Ok(Err(AppError::not_found("probe request 不存在"))),
            };
            if row_device_id != device_id_for_tx {
                return Ok(Err(AppError::forbidden("device_id 与 probe request 不匹配")));
            }

            // 2c. confirm 自环拒绝：confirm_required 仅允许 pending→confirm_pending，
            // 已处于 confirm_pending 时再次 confirm_required 应拒绝（避免重置 ticket TTL）。
            if upd_status == "confirm_pending" && current_status == "confirm_pending" {
                return Ok(Err(AppError::conflict(
                    "PROBE_ALREADY_PENDING_CONFIRM",
                    "该 probe request 已处于待确认状态",
                )));
            }

            // 2d. CAS 推进：幂等判定下沉到 UPDATE 的 WHERE，仅 pending / confirm_pending 可被推进；
            // rows_affected==0 表示已被并发回传 / sweeper 抢先推进到终态 → 幂等拒绝。
            let rows_affected = tx.execute(
                "UPDATE probe_executions SET
                    status = ?2,
                    result_json = COALESCE(?3, result_json),
                    stderr = COALESCE(?4, stderr),
                    duration_ms = COALESCE(?5, duration_ms),
                    truncated = ?6,
                    completed_at = COALESCE(?7, completed_at)
                 WHERE id = ?1 AND status IN ('pending', 'confirm_pending')",
                rusqlite::params![
                    &request_id,
                    &upd_status,
                    result_str_for_tx.as_deref(),
                    stderr_for_tx.as_deref(),
                    duration_ms as i64,
                    if truncated { 1_i64 } else { 0 },
                    completed_at_for_tx.as_deref(),
                ],
            )?;
            if rows_affected == 0 {
                return Ok(Err(AppError::bad_request(
                    "PROBE_ALREADY_COMPLETED",
                    "该 probe request 已处于终态",
                )));
            }

            Ok(Ok(AdvancedProbe {
                device_id: row_device_id,
                batch_id,
            }))
        })
    })
    .await
    .map_err(|e| AppError::internal(&format!("spawn_blocking: {e}")))?
    .map_err(|e| AppError::internal(&format!("update_probe_status: {e}")))?;

    let advanced = tx_result?;

    // ── 3. confirm_required 路径：写 ticket ──
    // 不覆盖已存在 ticket（不重置 TTL）的语义由上游的 2c 自环拒绝在同一 IMMEDIATE
    // 事务内保证：仅 `pending → confirm_pending` 这唯一一次 CAS 推进能走到此处，
    // 已是 confirm_pending 的重复 confirm_required 已在 2c 被 409 拒绝、不会再 issue。
    // 故对单个 request_id，issue_confirm 至多被调用一次，无 TTL 重置 / 结果覆盖风险。
    // （ProbeService.issue_confirm 内部仍为 insert，但本路径已确保其至多触发一次。）
    if let Some(token) = confirm_token {
        state.probe_service().issue_confirm(
            &body.request_id,
            ConfirmTicket {
                token,
                device_id: advanced.device_id.clone(),
                expires_at: std::time::Instant::now()
                    + std::time::Duration::from_secs(CONFIRM_TTL_SECS),
            },
        );
    }

    // ── 4. 广播到 admin SSE 流 ──
    state.probe_service().publish_result(ProbeResultPayload {
        device_id: advanced.device_id.clone(),
        request_id: body.request_id.clone(),
        batch_id: advanced.batch_id.clone(),
        status: body.status.clone(),
        result_json: body.result_json,
        stderr: body.stderr,
        duration_ms: Some(body.duration_ms),
        truncated: body.truncated,
    });

    tracing::info!(
        request_id = %body.request_id,
        device_id = %advanced.device_id,
        status = %body.status,
        duration_ms = body.duration_ms,
        truncated = body.truncated,
        confirm_token_present = body.confirm_token.is_some(),
        "客户端回传探针结果"
    );

    Ok(ok(serde_json::json!({ "ok": true })))
}
