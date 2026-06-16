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

use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;
use serde_json::json;

use crate::auth::AdminAuthUser;
use crate::extractors::JsonBody;
use crate::response::{ok, AppError};
use crate::services::updater::{
    ApplyContext, Channel, ChannelStatus, UpdatePhase, Updater, UpdaterError,
};
use crate::state::{AppState, ApplyTaskStatus, SseEvent};
use crate::store::operations::update_audit::UpdateAuditEntry;

/// 备份目录总占用阈值（10 GiB），超过前端做软提示。
const BACKUP_THRESHOLD_BYTES: u64 = 10_737_418_240;
/// 手动 / 每日备份保留个数（合并按 mtime 取最近 N 个）。
const BACKUP_KEEP: usize = 30;
/// download 整读上限：512 MiB。
const DOWNLOAD_MAX_BYTES: u64 = 512 * 1024 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(get_status))
        .route("/check", post(force_check))
        .route("/apply", post(apply))
        .route("/rollback", post(rollback))
        .route("/history", get(get_history))
        .route("/changelog", get(get_changelog))
        .route("/backups", get(list_backups).post(create_backup))
        .route("/backups/:name/restore", post(restore_backup))
        .route("/backups/:name/download", get(download_backup))
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
    let mut payload =
        serde_json::to_value(&snapshot).map_err(|e| AppError::internal(&e.to_string()))?;
    if let Some(map) = payload.as_object_mut() {
        if let Some(task) = state.apply_task_snapshot() {
            map.insert(
                "applyTask".into(),
                serde_json::to_value(&task).unwrap_or(serde_json::Value::Null),
            );
        }
        map.insert("uptimeSecs".into(), json!(state.uptime_secs()));
        let installed = std::env::current_exe()
            .ok()
            .and_then(|p| std::fs::metadata(&p).ok())
            .and_then(|m| m.modified().ok())
            .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339());
        map.insert("installedAt".into(), json!(installed));
        // v1.2.0-beta.8：有本地 DB 备份的版本 = 可安全回滚目标（回滚会恢复其数据快照）。
        // UI 据此限定回滚选项，避免回滚到不兼容旧版本。
        let rollback_targets = data_dir(&state)
            .map(|d| available_rollback_targets(&d))
            .unwrap_or_default();
        map.insert("rollbackTargets".into(), json!(rollback_targets));
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
    // 单次持锁占位，已有进行中 task 直接拒绝，避免并发触发底层文件锁
    if !state.try_begin_apply_task(initial.clone()) {
        return Err(AppError::conflict(
            "UPDATE_IN_PROGRESS",
            "已有升级任务在跑",
        ));
    }

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
    // v1.2.0-beta.10：watcher 探 /health 并读 body.status（见 classify_watcher_health）——
    // store=down 才回滚，维护 503 放行。比只看 HTTP 码（恒 200）或 /health/live（恒 200）更有意义。
    let health_url = format!("http://127.0.0.1:{}/health", state.config().port);
    // watcher 子进程 finalize audit 用的真实 DB 路径（= 运行时 database_url，绝对化以防 watcher CWD 不同）。
    let bg_audit_db_path = {
        let p = state.config().database_url.clone();
        std::fs::canonicalize(&p).unwrap_or_else(|_| std::path::PathBuf::from(p))
    };
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
            task_id: task_id.clone(),
            audit_db_path: bg_audit_db_path,
            allow_downgrade: false,
            // 升级走前向迁移,不回退 DB
            rollback_db_backup: None,
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

/// m022:POST /api/admin/updates/rollback —— 显式回退到某个旧版本。
///
/// 与 `apply` 的差异:
///   1. 接收 `target_version` 不必是 channel 的 latest;backend 先调
///      `fetch_release_by_tag(channel, target_version)` 把那个 tag 的元数据塞 cache,
///      再走 apply pipeline。
///   2. `ApplyContext.allow_downgrade = true`,绕过 semver 单调向上校验。
///   3. 审计日志 `action = "rollback"`(默认 `self_update`),前端可按 action 区分。
///   4. 仍然走 DB backup → swap → health-check → restart 的完整流程;失败自动回滚。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RollbackRequest {
    channel: Channel,
    target_version: String,
    confirm_current_version: String,
}

async fn rollback(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<RollbackRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let updater = require_updater(&state).await?;

    if req.confirm_current_version != updater.current_tag() {
        return Err(AppError::bad_request(
            "CURRENT_VERSION_MISMATCH",
            "前端版本号与后端不一致，请刷新后重试",
        ));
    }

    // v1.2.0-beta.8 最强回滚守卫:目标版本必须有本地 DB 备份(`learning-<target>.backup.db`)
    // 才放行——回滚会把它原子换为现役库,保证回滚后 binary/DB 严格一致。无备份则拒绝并列出
    // 可回滚目标,从源头拦住「回滚到不兼容旧版本→崩溃循环」(本次事故的诱因)。
    let rollback_db_backup = {
        let dir = data_dir(&state).ok_or_else(|| {
            AppError::bad_request(
                "ROLLBACK_NO_DATADIR",
                "内存库 / 无落盘数据目录，不支持带 DB 回滚",
            )
        })?;
        let p = dir.join(format!("learning-{}.backup.db", req.target_version));
        if !p.is_file() {
            let avail = available_rollback_targets(&dir);
            return Err(AppError::bad_request(
                "ROLLBACK_NO_DB_BACKUP",
                &format!(
                    "目标版本 {} 无本地 DB 备份，无法安全回滚（回滚需恢复该版本的数据快照以保证一致）。可回滚目标：{}",
                    req.target_version,
                    if avail.is_empty() { "无".to_string() } else { avail.join("、") }
                ),
            ));
        }
        p
    };

    // 关键一步:把 target_version 的 release 元数据从 GitHub 拉到 cache,
    // 否则 apply 会在 "channel latest != target_tag" 校验失败。
    updater
        .fetch_release_by_tag(req.channel, &req.target_version)
        .await
        .map_err(map_err)?;

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
    // 单次持锁占位，已有进行中 task 直接拒绝
    if !state.try_begin_apply_task(initial.clone()) {
        return Err(AppError::conflict(
            "UPDATE_IN_PROGRESS",
            "已有升级任务在跑",
        ));
    }

    tracing::warn!(
        admin_id = %admin.admin_id,
        task_id = %task_id,
        target = %req.target_version,
        current = updater.current_tag(),
        "管理员触发 rollback(回滚到旧版本)",
    );

    {
        let from = updater.current_tag().to_owned();
        let store = state.store().clone();
        let audit_id = task_id.clone();
        let audit_admin_id = admin.admin_id.clone();
        let to = req.target_version.clone();
        let ch = format!("{:?}", req.channel).to_lowercase();
        // 使用 m022 加入的 with_action 接口写 action="rollback",前端 history 按 action 区分
        if let Err(e) = store.insert_update_audit_with_action(
            &audit_id,
            &audit_admin_id,
            &from,
            &to,
            &ch,
            "rollback",
        ) {
            tracing::warn!(error=%e, "写入 rollback audit 失败(不影响流程)");
        }
    }

    let bg_state = state.clone();
    let bg_updater = updater.clone();
    let bg_store = state.store().clone();
    let target = req.target_version.clone();
    let channel = req.channel;
    // v1.2.0-beta.10：watcher 探 /health 并读 body.status（见 classify_watcher_health）——
    // store=down 才回滚，维护 503 放行。比只看 HTTP 码（恒 200）或 /health/live（恒 200）更有意义。
    let health_url = format!("http://127.0.0.1:{}/health", state.config().port);
    let bg_audit_db_path = {
        let p = state.config().database_url.clone();
        std::fs::canonicalize(&p).unwrap_or_else(|_| std::path::PathBuf::from(p))
    };
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

        let audit_store = bg_store.clone();
        let backup_cb = move |dst: &std::path::Path| -> Result<(), UpdaterError> {
            bg_store.backup_to(dst).map_err(UpdaterError::Store)
        };

        let rollback_state = bg_state.clone();
        let on_rollback = move |msg: String| {
            rollback_state.broadcast_to_all_sse(SseEvent::UpdateProgress {
                phase: format!("rollback: {msg}"),
                percent: 0,
            });
        };

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
            task_id: task_id.clone(),
            audit_db_path: bg_audit_db_path,
            allow_downgrade: true,
            // 最强回滚:把目标版本 DB 备份原子换为现役库,保证 binary/DB 一致
            rollback_db_backup: Some(rollback_db_backup),
        };
        match bg_updater.apply(ctx, backup_cb, sink).await {
            Ok(()) => {
                let _ = audit_store.complete_update_audit(&task_id, "success", None);
                bg_state.update_apply_task(|t| {
                    t.phase = "completed".into();
                    t.percent = 100;
                    t.completed_at = Some(chrono::Utc::now());
                });
            }
            Err(e) => {
                let msg = e.to_string();
                tracing::error!(task_id=%task_id, error=%msg, "rollback 后台 task 失败");
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
            "action": "rollback",
        })),
    ))
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

// ───────────────────────────── 备份 / changelog ─────────────────────────────

/// config.database_url 的父目录；`:memory:` 等无落盘库返回 None。
fn data_dir(state: &AppState) -> Option<PathBuf> {
    let config = state.config();
    let db_path = &config.database_url;
    if db_path == ":memory:" {
        return None;
    }
    match std::path::Path::new(db_path).parent() {
        Some(p) if !p.as_os_str().is_empty() => Some(p.to_path_buf()),
        // parent 为空（如 `learning.db`）→ 当前目录
        Some(_) => Some(PathBuf::from(".")),
        None => None,
    }
}

fn backups_dir(state: &AppState) -> Option<PathBuf> {
    data_dir(state).map(|d| d.join("backups"))
}

/// 安全解析备份文件名 → 绝对路径：拒绝含 `/`、`\\`、`..` 的 name；
/// 在 data_dir（learning-*.backup.db）与 backups_dir（backup-*.db）两处查找，命中且 is_file 才返回。
fn resolve_backup(state: &AppState, name: &str) -> Option<PathBuf> {
    // 路径遍历防护：拒绝分隔符 / .. / Windows 盘符冒号。
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.contains(':')
    {
        return None;
    }
    let dir = data_dir(state)?;
    for candidate in [dir.join(name), dir.join("backups").join(name)] {
        if candidate.is_file() {
            // 二次防御：规范化后必须仍位于 data_dir 内。
            if let (Ok(c), Ok(d)) = (candidate.canonicalize(), dir.canonicalize()) {
                if c.starts_with(&d) {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

/// UTC `YYYYMMDD-HHMMSS` 时间戳，用于备份文件名。
fn ts_now() -> String {
    chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string()
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupEntry {
    name: String,
    kind: &'static str,
    size_bytes: u64,
    created_at: String,
    version: Option<String>,
}

/// 从 `learning-<tag>.backup.db` 提取 `<tag>`（非该形态返回 None）。
fn upgrade_backup_version(name: &str) -> Option<String> {
    name.strip_prefix("learning-")
        .and_then(|s| s.strip_suffix(".backup.db"))
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// v1.2.0-beta.8：扫描 data 目录，列出有 `learning-<tag>.backup.db` 的版本 tag——
/// 即「可安全回滚目标」（带 DB 快照）。供回滚守卫报错提示 + 状态接口下发给 UI。
fn available_rollback_targets(data_dir: &std::path::Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(data_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| upgrade_backup_version(&e.file_name().to_string_lossy()))
        .collect();
    v.sort();
    v.dedup();
    v
}

fn entry_from_path(
    p: &std::path::Path,
    kind: &'static str,
    version: Option<String>,
) -> Option<BackupEntry> {
    let meta = std::fs::metadata(p).ok()?;
    let created_at = meta
        .modified()
        .ok()
        .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
        .unwrap_or_default();
    Some(BackupEntry {
        name: p.file_name()?.to_string_lossy().into_owned(),
        kind,
        size_bytes: meta.len(),
        created_at,
        version,
    })
}

/// GET /api/admin/updates/changelog?channel=stable|beta
#[derive(Debug, Deserialize)]
struct ChangelogQuery {
    channel: Option<Channel>,
}

async fn get_changelog(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(query): Query<ChangelogQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let updater = require_updater(&state).await?;
    let snapshot = updater.snapshot().await;

    // 选 channel：显式指定优先；否则 stable 有 update 取 stable，否则回落 beta。
    let pick = |ch: Option<Channel>| -> Option<(Channel, ChannelStatus)> {
        match ch {
            Some(Channel::Stable) => snapshot.stable.clone().map(|s| (Channel::Stable, s)),
            Some(Channel::Beta) => snapshot.beta.clone().map(|s| (Channel::Beta, s)),
            None => snapshot
                .stable
                .clone()
                .filter(|s| s.has_update)
                .map(|s| (Channel::Stable, s))
                .or_else(|| snapshot.beta.clone().map(|s| (Channel::Beta, s))),
        }
    };
    let Some((channel, ch)) = pick(query.channel) else {
        return Ok(ok(json!({ "available": false })));
    };
    if !ch.has_update {
        return Ok(ok(json!({ "available": false })));
    }

    let current = updater.current_tag().to_owned();
    let head = ch.latest_version.clone();
    match updater.fetch_changelog(&current, &head).await {
        Ok(summary) => {
            let mut v =
                serde_json::to_value(&summary).map_err(|e| AppError::internal(&e.to_string()))?;
            if let Some(map) = v.as_object_mut() {
                map.insert("available".into(), json!(true));
                map.insert("channel".into(), json!(channel.as_str()));
                map.insert("targetVersion".into(), json!(head));
            }
            Ok(ok(v))
        }
        // changelog 拉取失败不报 500：前端回退渲染 releaseNotes。
        Err(e) => Ok(ok(json!({ "available": false, "reason": e.to_string() }))),
    }
}

/// GET /api/admin/updates/backups
async fn list_backups(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let mut entries: Vec<BackupEntry> = Vec::new();

    if let Some(dir) = data_dir(&state) {
        // data_dir 直接层：升级备份 learning-<tag>.backup.db
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for ent in rd.flatten() {
                let p = ent.path();
                if !p.is_file() {
                    continue;
                }
                let fname = ent.file_name().to_string_lossy().into_owned();
                if let Some(version) = upgrade_backup_version(&fname) {
                    if let Some(e) = entry_from_path(&p, "upgrade", Some(version)) {
                        entries.push(e);
                    }
                }
            }
        }
        // backups 子目录：手动 / 每日 / pre-restore
        let bdir = dir.join("backups");
        if let Ok(rd) = std::fs::read_dir(&bdir) {
            for ent in rd.flatten() {
                let p = ent.path();
                if !p.is_file() {
                    continue;
                }
                let fname = ent.file_name().to_string_lossy().into_owned();
                let kind = if fname.starts_with("backup-manual-") && fname.ends_with(".db") {
                    "manual"
                } else if fname.starts_with("backup-daily-") && fname.ends_with(".db") {
                    "daily"
                } else if fname.starts_with("backup-pre-restore-") && fname.ends_with(".db") {
                    "pre_restore"
                } else {
                    continue;
                };
                if let Some(e) = entry_from_path(&p, kind, None) {
                    entries.push(e);
                }
            }
        }
    }

    entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    let total_bytes: u64 = entries.iter().map(|e| e.size_bytes).sum();
    Ok(ok(json!({
        "backups": entries,
        "totalBytes": total_bytes,
        "thresholdBytes": BACKUP_THRESHOLD_BYTES,
    })))
}

/// 在 backups 目录下保留最近 BACKUP_KEEP 个 manual+daily，删多余（按 mtime）。失败仅 warn。
fn prune_backups(bdir: &std::path::Path) {
    let Ok(rd) = std::fs::read_dir(bdir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = rd
        .flatten()
        .filter_map(|ent| {
            let p = ent.path();
            let fname = ent.file_name().to_string_lossy().into_owned();
            let is_target = (fname.starts_with("backup-manual-")
                || fname.starts_with("backup-daily-"))
                && fname.ends_with(".db");
            if !is_target || !p.is_file() {
                return None;
            }
            let mtime = std::fs::metadata(&p).ok()?.modified().ok()?;
            Some((mtime, p))
        })
        .collect();
    if files.len() <= BACKUP_KEEP {
        return;
    }
    files.sort_by(|a, b| b.0.cmp(&a.0)); // 新→旧
    for (_, p) in files.into_iter().skip(BACKUP_KEEP) {
        if let Err(e) = std::fs::remove_file(&p) {
            tracing::warn!(path=%p.display(), error=%e, "清理旧备份失败");
        }
    }
}

/// POST /api/admin/updates/backups —— 手动备份。
async fn create_backup(
    admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let bdir = backups_dir(&state)
        .ok_or_else(|| AppError::bad_request("NO_DATA_DIR", "当前数据库无落盘目录，无法备份"))?;
    std::fs::create_dir_all(&bdir).map_err(|e| AppError::internal(&e.to_string()))?;

    let name = format!("backup-manual-{}.db", ts_now());
    let target = bdir.join(&name);
    state
        .store()
        .backup_to(&target)
        .map_err(|e| AppError::internal(&e.to_string()))?;

    prune_backups(&bdir);

    let _ = state.store().insert_admin_audit(
        &admin.admin_id,
        "db_backup.manual",
        Some("backup"),
        Some(&name),
        None,
    );

    let size_bytes = std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
    Ok(ok(json!({
        "name": name,
        "kind": "manual",
        "sizeBytes": size_bytes,
        "createdAt": chrono::Utc::now().to_rfc3339(),
        "version": serde_json::Value::Null,
    })))
}

/// 维护模式 RAII 守卫：drop 时无条件复位为 false，
/// 保证 restore 即使中途 panic / 早返回也不会把 /api 永久卡在 503。
struct MaintenanceGuard<'a> {
    state: &'a AppState,
}
impl Drop for MaintenanceGuard<'_> {
    fn drop(&mut self) {
        self.state.set_maintenance(false);
    }
}

/// apply task 槽 RAII 守卫：restore 同步占用槽以阻断并发 apply/rollback，
/// drop 时清空（成功/错误/panic 均复位），避免遗留「进行中」task 永久锁死后续升级。
struct ApplyTaskGuard<'a> {
    state: &'a AppState,
}
impl Drop for ApplyTaskGuard<'_> {
    fn drop(&mut self) {
        self.state.set_apply_task(None);
    }
}

/// POST /api/admin/updates/backups/:name/restore
async fn restore_backup(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let path = resolve_backup(&state, &name)
        .ok_or_else(|| AppError::bad_request("BACKUP_NOT_FOUND", "备份文件不存在或文件名非法"))?;

    // 单次持锁占用 apply 槽：升级任务进行中则拒绝，否则原子占位阻断整个 restore 窗口内的并发 apply/rollback。
    let restore_task = ApplyTaskStatus {
        task_id: uuid::Uuid::new_v4().to_string(),
        phase: "restoring".into(),
        percent: 0,
        target_version: format!("restore:{name}"),
        started_at: chrono::Utc::now(),
        completed_at: None,
        error: None,
    };
    if !state.try_begin_apply_task(restore_task) {
        return Err(AppError::conflict("UPDATE_IN_PROGRESS", "已有升级任务在跑"));
    }
    let _apply_slot = ApplyTaskGuard { state: &state };

    let bdir = backups_dir(&state)
        .ok_or_else(|| AppError::bad_request("NO_DATA_DIR", "当前数据库无落盘目录，无法恢复"))?;

    // 开维护模式 + RAII 守卫：任何退出路径（成功/错误/panic）都会复位维护模式，
    // 避免 restore 中途崩溃把 /api 永久卡在 503。
    state.set_maintenance(true);
    let _maint = MaintenanceGuard { state: &state };

    let pre_name = format!("backup-pre-restore-{}.db", ts_now());
    let outcome: Result<(), AppError> = (|| {
        std::fs::create_dir_all(&bdir).map_err(|e| AppError::internal(&e.to_string()))?;
        state
            .store()
            .backup_to(&bdir.join(&pre_name))
            .map_err(|e| AppError::internal(&e.to_string()))?;
        state
            .store()
            .restore_from(&path)
            .map_err(|e| AppError::internal(&e.to_string()))?;
        Ok(())
    })();

    // 破坏性操作：成功/失败都留审计，记录兜底点与失败原因。
    let (audit_outcome, reason) = match &outcome {
        Ok(()) => ("success", serde_json::Value::Null),
        Err(e) => ("failed", json!(e.message.clone())),
    };
    let _ = state.store().insert_admin_audit(
        &admin.admin_id,
        "db_restore",
        Some("backup"),
        Some(&name),
        Some(&json!({
            "restored_from": name,
            "pre_restore_backup": pre_name,
            "outcome": audit_outcome,
            "reason": reason,
        })),
    );

    outcome?;

    tracing::warn!(admin_id=%admin.admin_id, backup=%name, "管理员从备份恢复数据库");

    Ok(ok(json!({
        "restored": true,
        "restartRecommended": true,
        "preRestoreBackup": pre_name,
    })))
}

/// GET /api/admin/updates/backups/:name/download —— 二进制下载，不套 ok() 信封。
async fn download_backup(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Response, AppError> {
    let path = resolve_backup(&state, &name)
        .ok_or_else(|| AppError::not_found("备份文件不存在或文件名非法"))?;

    let size = std::fs::metadata(&path)
        .map(|m| m.len())
        .map_err(|e| AppError::internal(&e.to_string()))?;
    if size > DOWNLOAD_MAX_BYTES {
        return Err(AppError::bad_request(
            "BACKUP_TOO_LARGE",
            "备份文件超过 512MB，无法直接下载",
        ));
    }

    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| AppError::internal(&e.to_string()))?;

    // 文件名已过 resolve_backup 约束，再剥引号防 header 注入。
    let safe_name = name.replace('"', "");
    Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{safe_name}\""),
        )
        .body(Body::from(bytes))
        .map_err(|e| AppError::internal(&e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_backup_version_parsing() {
        assert_eq!(
            upgrade_backup_version("learning-v1.2.0-beta.5.backup.db").as_deref(),
            Some("v1.2.0-beta.5")
        );
        assert_eq!(upgrade_backup_version("learning-.backup.db"), None);
        assert_eq!(upgrade_backup_version("backup-daily-20260610.db"), None);
        assert_eq!(upgrade_backup_version("learning.db"), None);
    }

    /// 可回滚目标 = data 目录里有 learning-<tag>.backup.db 的版本，排序去重，
    /// 忽略 daily/manual 等非升级备份与 live 库本身。
    #[test]
    fn available_rollback_targets_lists_backed_up_versions() {
        let dir = tempfile::tempdir().unwrap();
        for f in [
            "learning-v1.2.0-beta.5.backup.db",
            "learning-v1.2.0-beta.4.backup.db",
            "learning.db",                    // 现役库，非目标
            "backups/backup-daily-20260610.db", // daily，非目标（且在子目录）
            "learning.manual-bak.db",         // 手动备份，非 learning-<tag> 形态
        ] {
            let p = dir.path().join(f);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&p, b"x").unwrap();
        }
        let got = available_rollback_targets(dir.path());
        assert_eq!(got, vec!["v1.2.0-beta.4", "v1.2.0-beta.5"]);
    }
}
