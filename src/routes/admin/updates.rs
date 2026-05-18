//! 管理员后台 - 自更新三件套：
//!   GET  /api/admin/updates/status   缓存内的当前/最新版本视图（不打 GitHub）
//!   POST /api/admin/updates/check    强制刷新（仍走 ETag，命中 304 时省额度）
//!   POST /api/admin/updates/apply    一键下载 + sha256 + 替换 + fork-exec 自重启

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;

use crate::auth::AdminAuthUser;
use crate::extractors::JsonBody;
use crate::response::{ok, AppError};
use crate::services::updater::{Updater, UpdaterError};
use crate::state::{AppState, SseEvent};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(get_status))
        .route("/check", post(force_check))
        .route("/apply", post(apply))
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
    Ok(ok(updater.snapshot().await))
}

async fn force_check(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let updater = require_updater(&state).await?;
    let prev = updater.snapshot().await.latest_version;
    // 显式走 force 路径，跳过 TTL，仍带 ETag 节省额度
    let status = updater.force_check_latest().await.map_err(map_err)?;
    // 缓存里的 latest 发生变化 → 顺手广播一次，省得用户等下次 worker tick
    if status.has_update && status.latest_version != prev {
        if let Some(ref tag) = status.latest_version {
            state.broadcast_to_all_sse(SseEvent::ReleaseAvailable {
                latest_tag: tag.clone(),
            });
        }
    }
    Ok(ok(status))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApplyRequest {
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

    tracing::warn!(
        admin_id = %admin.admin_id,
        target = %req.target_version,
        current = updater.current_tag(),
        "管理员触发一键自更新",
    );

    let store = state.store().clone();
    let progress_state = state.clone();
    let sink: crate::services::updater::ProgressSink = Arc::new(move |phase| {
        let payload = serde_json::to_value(&phase).unwrap_or(serde_json::Value::Null);
        let pretty = payload
            .get("phase")
            .and_then(|v| v.as_str())
            .unwrap_or("phase")
            .to_string();
        let percent = match &phase {
            crate::services::updater::UpdatePhase::Downloading { downloaded, total } => {
                if *total > 0 {
                    ((*downloaded * 70 / *total).min(70)) as u8
                } else {
                    0
                }
            }
            crate::services::updater::UpdatePhase::Verifying => 75,
            crate::services::updater::UpdatePhase::Extracting => 80,
            crate::services::updater::UpdatePhase::BackingUpDb => 85,
            crate::services::updater::UpdatePhase::Swapping => 95,
            crate::services::updater::UpdatePhase::Restarting => 99,
        };
        progress_state.broadcast_to_all_sse(SseEvent::UpdateProgress {
            phase: pretty,
            percent,
        });
    });

    let backup_cb = move |dst: &std::path::Path| -> Result<(), UpdaterError> {
        store.backup_to(dst).map_err(UpdaterError::Store)
    };

    // 直接 await：成功路径会 process::exit(0)，HTTP 不再返回；失败路径返回 422
    updater
        .apply(&req.target_version, backup_cb, sink)
        .await
        .map_err(map_err)?;

    // 理论上到不了这里（成功已退出）；万一到了，前端轮询 /api/status 看版本号切换
    Ok(ok(serde_json::json!({ "restarting": true })))
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
        other => AppError::internal(&other.to_string()),
    }
}
