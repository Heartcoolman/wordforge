//! 远程探针 confirm token TTL sweeper。
//!
//! 每 30s 调 ProbeService::sweep_expired_confirms()，把过期的 request_id
//! 在 probe_executions 表中状态推进到 'expired'，并广播一次 result 给
//! admin SSE 流（让 REPL 卡片状态从「需确认」变「expired」而非永远 hang）。

use std::time::Duration;

use tokio::sync::broadcast;

use crate::services::probe::ProbeResultPayload;
use crate::state::AppState;
use crate::store::operations::probe::ProbeStatusUpdate;

const SWEEP_INTERVAL_SECS: u64 = 30;

pub async fn run(state: AppState, mut shutdown_rx: broadcast::Receiver<()>) {
    let mut interval = tokio::time::interval(Duration::from_secs(SWEEP_INTERVAL_SECS));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                if let Err(e) = sweep_once(&state).await {
                    tracing::warn!(error = %e, "probe_confirm_sweeper tick failed");
                }
            }
            _ = shutdown_rx.recv() => break,
        }
    }
}

async fn sweep_once(state: &AppState) -> Result<(), String> {
    let expired = state.probe_service().sweep_expired_confirms();
    if expired.is_empty() {
        return Ok(());
    }
    tracing::info!(count = expired.len(), "probe confirm tickets expired");

    for request_id in expired {
        // 查 batch_id / device_id 用于 broadcast
        let req_for_db = request_id.clone();
        let store = state.store().clone();
        let row = tokio::task::spawn_blocking(move || store.get_probe_execution(&req_for_db))
            .await
            .map_err(|e| format!("spawn_blocking: {e}"))?
            .map_err(|e| format!("get_probe_execution: {e}"))?;

        let Some(row) = row else {
            // 行已被删（极小概率），跳过
            continue;
        };

        // 推进状态到 expired
        let req_for_upd = request_id.clone();
        let now = chrono::Utc::now().to_rfc3339();
        let store2 = state.store().clone();
        tokio::task::spawn_blocking(move || {
            store2.update_probe_status(
                &req_for_upd,
                &ProbeStatusUpdate {
                    status: "expired",
                    result_json: None,
                    stderr: Some("confirm token TTL 过期，admin 未在 60s 内确认"),
                    duration_ms: None,
                    truncated: false,
                    completed_at: Some(&now),
                    confirmed_at: None,
                },
            )
        })
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
        .map_err(|e| format!("update_probe_status: {e}"))?;

        // 广播给 admin SSE 流
        state.probe_service().publish_result(ProbeResultPayload {
            device_id: row.device_id,
            request_id: row.id,
            batch_id: row.batch_id,
            status: "expired".to_string(),
            result_json: None,
            stderr: Some("confirm token TTL 过期".into()),
            duration_ms: None,
            truncated: false,
        });
    }

    Ok(())
}
