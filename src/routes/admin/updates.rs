//! 管理员后台 - 自更新三件套：
//!   GET  /api/admin/updates/status   缓存内的当前/最新版本视图（含异步 apply task 进度）
//!   POST /api/admin/updates/check    强制刷新（仍走 ETag，命中 304 时省额度）
//!   POST /api/admin/updates/apply    立即返回 taskId + 后台执行下载/解压/替换（v0.5.2+）
//!
//! v0.5.2 起 apply 不再阻塞 handler，避免前端 fetch 超时（HTTP 499）中断 axum
//! handler、连带打断升级流程的设计缺陷。前端发起后立即拿到 taskId，再通过
//! `/api/admin/updates/status` 轮询拿 phase / percent / error。
//!
//!   GET  /api/admin/updates/history  S5：升级历史列表（最近 50 条审计记录）

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use serde_json::json;

use crate::auth::AdminAuthUser;
use crate::extractors::JsonBody;
use crate::response::{ok, AppError};
use crate::services::updater::{ApplyContext, Channel, ChannelStatus, Updater, UpdaterError, UpdatePhase};
use crate::state::{ApplyTaskStatus, AppState, SseEvent};
use crate::store::operations::update_audit::UpdateAuditEntry;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(get_status))
        .route("/check", post(force_check))
        .route("/apply", post(apply))
        .route("/history", get(get_history))
}

async fn require_updater(state: &AppState) -> Result<Arc<Updater>, AppError> {
    state.updater().await.ok_or_else(|| AppError {
        status: StatusCode::SERVICE_UNAVAILABLE,
        code: "UPDATER_DISABLED".into(),
        message: "自更新服务未启用".into(),
        is_operational: true,
    })
}

async fn get_status(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let updater = require_updater(&state).await?;
    let snapshot = updater.snapshot().await;
    // v0.5.2：合并版本视图与后台 apply task 进度，前端单端点轮询即可
    let mut payload = serde_json::to_value(&snapshot).map_err(|e| AppError::internal(&e.to_string()))?;
    if let Some(task) = state.apply_task_snapshot() {
        if let Some(map) = payload.as_object_mut() {
            map.insert(
                "applyTask".into(),
                serde_json::to_value(&task)
                    .unwrap_or(serde_json::Value::Null),
            );
        }
    }
    Ok(ok(payload))
}

async fn force_check(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let updater = require_updater(&state).await?;
    let prev = updater.snapshot().await;
    // 显式走 force 路径，跳过 TTL，仍带 ETag 节省额度
    let status = updater.force_check_latest().await.map_err(map_err)?;
    // v0.6.0-beta.3：stable / beta 各自有更新且 latest 变化 → 分别广播一次
    broadcast_channel_change(&state, Channel::Stable, &prev.stable, &status.stable);
    broadcast_channel_change(&state, Channel::Beta, &prev.beta, &status.beta);
    Ok(ok(status))
}

fn broadcast_channel_change(
    state: &AppState,
    channel: Channel,
    prev: &Option<ChannelStatus>,
    new: &Option<ChannelStatus>,
) {
    let Some(new) = new else {
        return;
    };
    if !new.has_update {
        return;
    }
    if prev.as_ref().map(|p| p.latest_version.as_str()) != Some(new.latest_version.as_str()) {
        state.broadcast_to_all_sse(SseEvent::ReleaseAvailable {
            latest_tag: new.latest_version.clone(),
            channel,
        });
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApplyRequest {
    /// v0.6.0-beta.3：必填，后端用它定位 cache.<channel> 校验 target_version
    channel: Channel,
    target_version: String,
    confirm_current_version: String,
}

async fn apply(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<ApplyRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let updater = require_updater(&state).await?;

    // 二次防误操作：客户端必须把它看到的 current 回传，与服务端实际不符则拒绝
    if req.confirm_current_version != updater.current_tag() {
        return Err(AppError::bad_request(
            "CURRENT_VERSION_MISMATCH",
            "前端版本号与后端不一致，请刷新后重试",
        ));
    }

    // 已有进行中的 task 直接拒绝，避免并发触发底层文件锁
    if let Some(existing) = state.apply_task_snapshot() {
        if existing.is_running() {
            return Err(AppError {
                status: StatusCode::CONFLICT,
                code: "UPDATE_IN_PROGRESS".into(),
                message: format!("已有升级任务在跑：{}", existing.target_version),
                is_operational: true,
            });
        }
    }

    let task_id = uuid::Uuid::new_v4().to_string();
    let started_at = chrono::Utc::now();
    let initial = ApplyTaskStatus {
        task_id: task_id.clone(),
        phase: "pending".into(),
        percent: 0,
        target_version: req.target_version.clone(),
        started_at,
        completed_at: None,
        error: None,
    };
    state.set_apply_task(Some(initial.clone()));

    tracing::warn!(
        admin_id = %admin.admin_id,
        task_id = %task_id,
        target = %req.target_version,
        current = updater.current_tag(),
        "管理员触发一键自更新（异步执行）",
    );

    // S5：写入审计记录（fire-and-forget，失败只记日志不阻塞流程）
    {
        let from = updater.current_tag().to_owned();
        let store = state.store().clone();
        let audit_id = task_id.clone();
        let audit_admin_id = admin.admin_id.clone();
        let to = req.target_version.clone();
        let ch = format!("{:?}", req.channel).to_lowercase();
        if let Err(e) = store.insert_update_audit(&audit_id, &audit_admin_id, &from, &to, &ch) {
            tracing::warn!(error=%e, "写入 update_audit_log 失败（不影响升级流程）");
        }
    }

    // spawn 后台 task，handler 立即返回，避免前端 fetch 超时中断 axum handler
    let bg_state = state.clone();
    let bg_updater = updater.clone();
    let bg_store = state.store().clone();
    let target = req.target_version.clone();
    let channel = req.channel;
    // M0-R3：构造子进程健康自检 URL
    let health_url = format!(
        "http://127.0.0.1:{}/health",
        state.config().port
    );
    tokio::spawn(async move {
        let progress_state = bg_state.clone();
        let sink: crate::services::updater::ProgressSink = Arc::new(move |phase| {
            let (label, percent) = phase_label_percent(&phase);
            progress_state.broadcast_to_all_sse(SseEvent::UpdateProgress {
                phase: label.clone(),
                percent,
            });
            progress_state.update_apply_task(|t| {
                t.phase = label;
                t.percent = percent;
            });
        });

        // S5：audit_store 供后续 complete_update_audit 使用（bg_store 被 backup_cb move 消耗）
        let audit_store = bg_store.clone();
        let backup_cb = move |dst: &std::path::Path| -> Result<(), UpdaterError> {
            bg_store.backup_to(dst).map_err(UpdaterError::Store)
        };

        // M0-R3：回滚告警回调，通过 SSE 广播给 admin 前端
        let rollback_state = bg_state.clone();
        let on_rollback = move |msg: String| {
            rollback_state.broadcast_to_all_sse(SseEvent::UpdateProgress {
                phase: format!("rollback: {msg}"),
                percent: 0,
            });
        };

        // M0-R4：Swapping 前开启维护模式，完成后关闭（防止写入期间的数据一致性问题）
        let maintenance_state = bg_state.clone();
        let on_maintenance = move |active: bool| {
            maintenance_state.set_maintenance(active);
        };

        let ctx = ApplyContext {
            channel,
            target_tag: target,
            health_url,
            on_rollback: Box::new(on_rollback),
            on_maintenance: Box::new(on_maintenance),
        };
        match bg_updater.apply(ctx, backup_cb, sink).await {
            Ok(()) => {
                // 成功路径已 process::exit(0)，理论到不了这里；保底标记 completed
                let _ = audit_store.complete_update_audit(&task_id, "success", None);
                bg_state.update_apply_task(|t| {
                    t.phase = "completed".into();
                    t.percent = 100;
                    t.completed_at = Some(chrono::Utc::now());
                });
            }
            Err(e) => {
                let msg = e.to_string();
                tracing::error!(task_id=%task_id, error=%msg, "apply 后台 task 失败");
                let _ = audit_store.complete_update_audit(&task_id, "failed", Some(msg.as_str()));
                bg_state.update_apply_task(|t| {
                    t.phase = "failed".into();
                    t.completed_at = Some(chrono::Utc::now());
                    t.error = Some(msg);
                });
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        ok(json!({
            "taskId": initial.task_id,
            "phase": initial.phase,
            "percent": initial.percent,
            "targetVersion": initial.target_version,
            "startedAt": initial.started_at,
        })),
    ))
}

/// 把 updater 内部的 `UpdatePhase` 枚举映射到前端可读 label + percent。
fn phase_label_percent(phase: &UpdatePhase) -> (String, u8) {
    match phase {
        UpdatePhase::Downloading { downloaded, total } => {
            let p = if *total > 0 {
                ((*downloaded * 70 / *total).min(70)) as u8
            } else {
                0
            };
            ("downloading".into(), p)
        }
        UpdatePhase::Verifying => ("verifying".into(), 75),
        UpdatePhase::Extracting => ("extracting".into(), 80),
        UpdatePhase::BackingUpDb => ("backing_up_db".into(), 85),
        UpdatePhase::Swapping => ("swapping".into(), 95),
        UpdatePhase::HealthChecking => ("health_checking".into(), 97),
        UpdatePhase::Restarting => ("restarting".into(), 99),
    }
}

fn map_err(e: UpdaterError) -> AppError {
    match e {
        UpdaterError::DowngradeRefused { .. } => {
            AppError::bad_request("DOWNGRADE_REFUSED", &e.to_string())
        }
        UpdaterError::NoAsset { .. } => AppError::bad_request("NO_ASSET", &e.to_string()),
        UpdaterError::Sha256Mismatch { .. } => {
            AppError::bad_request("SHA256_MISMATCH", &e.to_string())
        }
        UpdaterError::TarballTooLarge { .. } => {
            AppError::bad_request("TARBALL_TOO_LARGE", &e.to_string())
        }
        UpdaterError::UnsafePath(_) => AppError::bad_request("UNSAFE_PATH", &e.to_string()),
        UpdaterError::RateLimited => AppError {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "GITHUB_RATE_LIMITED".into(),
            message: e.to_string(),
            is_operational: true,
        },
        UpdaterError::Locked => AppError {
            status: StatusCode::CONFLICT,
            code: "UPDATE_IN_PROGRESS".into(),
            message: e.to_string(),
            is_operational: true,
        },
        UpdaterError::InvalidTarget(_) => {
            AppError::bad_request("INVALID_TARGET_VERSION", &e.to_string())
        }
        UpdaterError::Api { status, .. } => AppError {
            status: StatusCode::BAD_GATEWAY,
            code: format!("GITHUB_API_{status}"),
            message: e.to_string(),
            is_operational: true,
        },
        // M0-P5：phase watchdog 超时 → 503，前端可展示明确错误并提示用户重试
        UpdaterError::PhaseTimeout { .. } => AppError {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "UPDATE_PHASE_TIMEOUT".into(),
            message: e.to_string(),
            is_operational: true,
        },
        other => AppError::internal(&other.to_string()),
    }
}

/// S5：GET /api/admin/updates/history — 最近 50 条升级审计记录（倒序）。
async fn get_history(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let entries: Vec<UpdateAuditEntry> = state
        .store()
        .list_update_audit(50)
        .map_err(|e| AppError::internal(&e.to_string()))?;
    Ok(ok(json!({ "entries": entries })))
}
