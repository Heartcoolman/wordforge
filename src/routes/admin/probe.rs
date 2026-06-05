//! 远程探针 admin REST 路由。
//!
//! - `POST /api/admin/probe` 下发探针（M1：单设备 / 多设备 device_ids 列表；
//!   `all_online` 字段 M4 启用）。
//! - `GET  /api/admin/probe/:batch_id/stream` admin SSE 拉取该 batch 的结果。
//! - `GET  /api/admin/probe` 历史列表 / `GET /:request_id` 单条（M4 启用）。

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::Router;
use base64::Engine;
use futures::Stream;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::auth::AdminAuthUser;
use crate::extractors::JsonBody;
use crate::response::{ok, AppError};
use crate::state::{AppState, SseEvent};
use crate::store::operations::probe::{ProbeInsert, ProbeListFilter};

/// 当前后端支持的 ctx schema 版本。客户端 ctx_version 不一致时回
/// `unsupported_ctx_version`。新增 ctx 字段或方法 → 自增本常量 + 同步
/// 前端 `CLIENT_CTX_VERSION`。
pub const PROBE_CTX_VERSION_LATEST: u32 = 1;

/// script 上限 16 KB（编辑器够用、SSE 推送轻、DB 落表轻）。
const MAX_SCRIPT_BYTES: usize = 16 * 1024;
/// timeout 下限（上限 / 默认值走 ProbeConfig）。
const TIMEOUT_MS_MIN: u32 = 100;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(dispatch_probe).get(list_probe))
        .route("/:batch_id/stream", get(batch_stream))
        .route("/:request_id/confirm", post(confirm_probe))
        .route("/by-id/:request_id", get(get_probe))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TargetSpec {
    #[serde(default)]
    device_ids: Vec<String>,
    #[serde(default)]
    all_online: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DispatchRequest {
    targets: TargetSpec,
    script: String,
    #[serde(default)]
    timeout_ms: Option<u32>,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Dispatched {
    device_id: String,
    request_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DispatchResponse {
    batch_id: String,
    dispatched: Vec<Dispatched>,
    skipped_offline: Vec<String>,
}

async fn dispatch_probe(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<DispatchRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // ── kill switch ──
    if !state.config().probe.enabled {
        return Err(AppError::service_unavailable(
            "PROBE_DISABLED",
            "远程探针未启用，请联系系统管理员设置 PROBE_ENABLED=true",
        ));
    }

    // ── per-admin 限速 ──
    state
        .probe_service()
        .check_and_record_admin_call(&admin.admin_id, state.config().probe.rate_limit_per_min)
        .map_err(|_| {
            AppError::too_many_requests(&format!(
                "admin 探针调用频率超限（{}/min）",
                state.config().probe.rate_limit_per_min
            ))
        })?;

    // ── 输入校验 ──
    if req.script.len() > MAX_SCRIPT_BYTES {
        return Err(AppError::bad_request(
            "PROBE_SCRIPT_TOO_LARGE",
            "script 长度超过 16KB 上限",
        ));
    }
    if req.script.trim().is_empty() {
        return Err(AppError::bad_request(
            "PROBE_SCRIPT_EMPTY",
            "script 不能为空",
        ));
    }
    let cfg_max_timeout = state.config().probe.max_timeout_ms;
    let cfg_default_timeout = state.config().probe.default_timeout_ms;
    let timeout_ms = req.timeout_ms.unwrap_or(cfg_default_timeout);
    if !(TIMEOUT_MS_MIN..=cfg_max_timeout).contains(&timeout_ms) {
        return Err(AppError::bad_request(
            "PROBE_TIMEOUT_OUT_OF_RANGE",
            &format!("timeoutMs 必须在 [{TIMEOUT_MS_MIN}, {cfg_max_timeout}] 范围内"),
        ));
    }

    // 目标集合：device_ids 显式列表 + all_online 二选一。
    let device_ids: Vec<String> = if req.targets.all_online {
        state
            .active_sse()
            .iter()
            .filter_map(|e| {
                if e.value().is_empty() {
                    None
                } else {
                    Some(e.key().clone())
                }
            })
            .collect()
    } else {
        req.targets
            .device_ids
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };
    if device_ids.is_empty() {
        return Err(AppError::bad_request(
            "PROBE_INVALID_TARGETS",
            "至少需要 1 个 deviceId 目标（或开启 allOnline 但当前无在线设备）",
        ));
    }

    // ── 准备 payload ──
    let batch_id = uuid::Uuid::new_v4().to_string();
    let script_b64 = base64::engine::general_purpose::STANDARD.encode(req.script.as_bytes());
    let script_sha256 = {
        let mut h = Sha256::new();
        h.update(req.script.as_bytes());
        hex::encode(h.finalize())
    };
    let has_cmd_call = req.script.contains("ctx.cmd.");
    let dispatched_at = chrono::Utc::now().to_rfc3339();
    let note = req.note.as_deref().map(str::trim).filter(|s| !s.is_empty());

    // ── 分流 online / offline，准备 DB rows ──
    struct Pair {
        device_id: String,
        request_id: String,
        online: bool,
    }
    let mut pairs: Vec<Pair> = Vec::with_capacity(device_ids.len());
    for did in &device_ids {
        let online = state
            .active_sse()
            .get(did)
            .map(|c| !c.is_empty())
            .unwrap_or(false);
        pairs.push(Pair {
            device_id: did.clone(),
            request_id: uuid::Uuid::new_v4().to_string(),
            online,
        });
    }

    // 落库（pending / offline）一次性事务。
    let rows: Vec<ProbeInsert<'_>> = pairs
        .iter()
        .map(|p| ProbeInsert {
            id: &p.request_id,
            batch_id: &batch_id,
            device_id: &p.device_id,
            admin_id: &admin.admin_id,
            // Q1: 无 admin_username 列时复用 admin_id（后续可扩展 join email）
            admin_username: &admin.admin_id,
            script_body: &req.script,
            script_sha256: &script_sha256,
            has_cmd_call,
            note,
            timeout_ms,
            status: if p.online { "pending" } else { "offline" },
            dispatched_at: &dispatched_at,
        })
        .collect();
    let store = state.store().clone();
    let rows_owned: Vec<_> = rows.iter().map(|r| OwnedRow::from_borrowed(r)).collect();
    tokio::task::spawn_blocking(move || {
        let borrowed: Vec<ProbeInsert<'_>> = rows_owned.iter().map(|r| r.as_borrowed()).collect();
        store.insert_probe_executions(&borrowed)
    })
    .await
    .map_err(|e| AppError::internal(&format!("spawn_blocking: {e}")))?
    .map_err(|e| AppError::internal(&format!("insert_probe_executions: {e}")))?;

    // ── 对在线设备推 SSE ──
    for p in &pairs {
        if !p.online {
            continue;
        }
        if let Some(conns) = state.active_sse().get(&p.device_id) {
            for conn in conns.value() {
                let _ = conn.tx.send(SseEvent::ProbeRequest {
                    request_id: p.request_id.clone(),
                    batch_id: batch_id.clone(),
                    script_b64: script_b64.clone(),
                    timeout_ms,
                    ctx_version: PROBE_CTX_VERSION_LATEST,
                });
            }
        }
    }

    tracing::info!(
        admin_id = %admin.admin_id,
        batch_id = %batch_id,
        device_count = pairs.len(),
        has_cmd_call,
        "admin 下发远程探针"
    );

    let dispatched: Vec<Dispatched> = pairs
        .iter()
        .filter(|p| p.online)
        .map(|p| Dispatched {
            device_id: p.device_id.clone(),
            request_id: p.request_id.clone(),
        })
        .collect();
    let skipped_offline: Vec<String> = pairs
        .iter()
        .filter(|p| !p.online)
        .map(|p| p.device_id.clone())
        .collect();

    Ok(ok(serde_json::json!(DispatchResponse {
        batch_id,
        dispatched,
        skipped_offline,
    })))
}

/// `OwnedRow` 是为绕过 ProbeInsert 借用 &str 的麻烦：先把 dispatch handler 中
/// 的所有 &str 用 String 拷贝一份保存（per row），再在 spawn_blocking 内构造
/// borrow form 调用 store。代价是一次 clone，可接受。
struct OwnedRow {
    id: String,
    batch_id: String,
    device_id: String,
    admin_id: String,
    admin_username: String,
    script_body: String,
    script_sha256: String,
    has_cmd_call: bool,
    note: Option<String>,
    timeout_ms: u32,
    status: String,
    dispatched_at: String,
}

impl OwnedRow {
    fn from_borrowed(r: &ProbeInsert<'_>) -> Self {
        Self {
            id: r.id.to_string(),
            batch_id: r.batch_id.to_string(),
            device_id: r.device_id.to_string(),
            admin_id: r.admin_id.to_string(),
            admin_username: r.admin_username.to_string(),
            script_body: r.script_body.to_string(),
            script_sha256: r.script_sha256.to_string(),
            has_cmd_call: r.has_cmd_call,
            note: r.note.map(str::to_string),
            timeout_ms: r.timeout_ms,
            status: r.status.to_string(),
            dispatched_at: r.dispatched_at.to_string(),
        }
    }
    fn as_borrowed(&self) -> ProbeInsert<'_> {
        ProbeInsert {
            id: &self.id,
            batch_id: &self.batch_id,
            device_id: &self.device_id,
            admin_id: &self.admin_id,
            admin_username: &self.admin_username,
            script_body: &self.script_body,
            script_sha256: &self.script_sha256,
            has_cmd_call: self.has_cmd_call,
            note: self.note.as_deref(),
            timeout_ms: self.timeout_ms,
            status: &self.status,
            dispatched_at: &self.dispatched_at,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfirmBody {
    device_id_suffix: String,
}

async fn confirm_probe(
    admin: AdminAuthUser,
    Path(request_id): Path<String>,
    State(state): State<AppState>,
    JsonBody(body): JsonBody<ConfirmBody>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    use crate::services::probe::ConfirmError;

    if !state.config().probe.enabled {
        return Err(AppError::service_unavailable(
            "PROBE_DISABLED",
            "远程探针未启用",
        ));
    }

    let ticket = state
        .probe_service()
        .consume_confirm(&request_id, &body.device_id_suffix)
        .map_err(|e| match e {
            ConfirmError::NotFound => AppError::not_found("confirm ticket 不存在或已被消费"),
            ConfirmError::Expired => AppError::bad_request(
                "PROBE_CONFIRM_EXPIRED",
                "confirm token 已过期（60s TTL），请重新下发探针",
            ),
            ConfirmError::SuffixMismatch => {
                AppError::bad_request("PROBE_CONFIRM_SUFFIX_MISMATCH", "device_id 后 5 位不匹配")
            }
        })?;

    // 落库：confirmed_at + 状态保持 confirm_pending（客户端重跑后由 results
    // 端点推进到 ok/error/timeout）。注意：UPDATE 用 update_probe_status 但
    // status 字段必须从表里 select 后回写——这里简化为只更新 confirmed_at，
    // 用一条小 SQL；不复用 update_probe_status（它会改 status）。
    let req_id_for_db = request_id.clone();
    let now = chrono::Utc::now().to_rfc3339();
    let store = state.store().clone();
    tokio::task::spawn_blocking(move || {
        let conn = store.conn()?;
        conn.execute(
            "UPDATE probe_executions SET confirmed_at = ?2 WHERE id = ?1",
            rusqlite::params![req_id_for_db, now],
        )?;
        Ok::<_, crate::store::StoreError>(())
    })
    .await
    .map_err(|e| AppError::internal(&format!("spawn_blocking: {e}")))?
    .map_err(|e| AppError::internal(&format!("confirmed_at update: {e}")))?;

    // 推 SSE 让客户端用同一 ctx 快照重跑
    if let Some(conns) = state.active_sse().get(&ticket.device_id) {
        for conn in conns.value() {
            let _ = conn.tx.send(SseEvent::ProbeConfirm {
                request_id: request_id.clone(),
                confirm_token: ticket.token.clone(),
            });
        }
    }

    tracing::info!(
        admin_id = %admin.admin_id,
        request_id = %request_id,
        device_id = %ticket.device_id,
        "admin 已确认远程探针 cmd 执行"
    );

    Ok(ok(serde_json::json!({ "confirmed": true })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    batch_id: Option<String>,
    device_id: Option<String>,
    admin_id: Option<String>,
    status: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
}

async fn list_probe(
    _admin: AdminAuthUser,
    Query(q): Query<ListQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if !state.config().probe.enabled {
        return Err(AppError::service_unavailable(
            "PROBE_DISABLED",
            "远程探针未启用",
        ));
    }
    let limit = q.limit.unwrap_or(50).min(200);
    let offset = q.offset.unwrap_or(0);
    let filter = ProbeListFilter {
        batch_id: q.batch_id,
        device_id: q.device_id,
        admin_id: q.admin_id,
        status: q.status,
    };
    let store = state.store().clone();
    let (rows, total) =
        tokio::task::spawn_blocking(move || store.list_probe_executions(&filter, limit, offset))
            .await
            .map_err(|e| AppError::internal(&format!("spawn_blocking: {e}")))?
            .map_err(|e| AppError::internal(&format!("list_probe_executions: {e}")))?;
    Ok(ok(serde_json::json!({ "rows": rows, "total": total })))
}

async fn get_probe(
    _admin: AdminAuthUser,
    Path(request_id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if !state.config().probe.enabled {
        return Err(AppError::service_unavailable(
            "PROBE_DISABLED",
            "远程探针未启用",
        ));
    }
    let store = state.store().clone();
    let row = tokio::task::spawn_blocking(move || store.get_probe_execution(&request_id))
        .await
        .map_err(|e| AppError::internal(&format!("spawn_blocking: {e}")))?
        .map_err(|e| AppError::internal(&format!("get_probe_execution: {e}")))?
        .ok_or_else(|| AppError::not_found("probe execution 不存在"))?;
    Ok(ok(serde_json::json!(row)))
}

async fn batch_stream(
    _admin: AdminAuthUser,
    Path(batch_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    if !state.config().probe.enabled {
        return Err(AppError::service_unavailable(
            "PROBE_DISABLED",
            "远程探针未启用",
        ));
    }
    let batch_id_for_total = batch_id.clone();
    let store = state.store().clone();
    let expected =
        tokio::task::spawn_blocking(move || store.count_probe_in_batch(&batch_id_for_total))
            .await
            .map_err(|e| AppError::internal(&format!("spawn_blocking: {e}")))?
            .map_err(|e| AppError::internal(&format!("count_probe_in_batch: {e}")))?;
    if expected == 0 {
        return Err(AppError::not_found("batch 不存在或已过期"));
    }

    let mut rx = state.probe_service().subscribe_batch(&batch_id);
    let probe_service = state.probe_service().clone();
    let batch_for_cleanup = batch_id.clone();

    let stream = async_stream::stream! {
        let mut received: u64 = 0;
        loop {
            match rx.recv().await {
                Ok(payload) => {
                    if payload.batch_id != batch_id {
                        continue;
                    }
                    let is_terminal = payload.status != "confirm_required";
                    if let Ok(json) = serde_json::to_string(&payload) {
                        yield Ok(Event::default().event("result").data(json));
                    }
                    // confirm_required 非终态：不计入 received，否则流会在 admin 确认、
                    // 客户端重跑的终态结果（ok/error/timeout/unsupported）到达前就关闭，
                    // 导致 admin 永远看不到受控写动作的成败。
                    if !is_terminal {
                        continue;
                    }
                    received += 1;
                    if received >= expected {
                        let completed = serde_json::json!({
                            "received": received,
                            "expected": expected,
                        });
                        yield Ok(Event::default().event("completed").data(completed.to_string()));
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(batch_id=%batch_id, skipped, "probe SSE 流追赶不及，跳过若干结果");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
        probe_service.drop_batch(&batch_for_cleanup);
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}
