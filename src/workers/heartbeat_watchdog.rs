use std::collections::HashSet;
use std::time::Duration;

use tokio::sync::broadcast;

use crate::state::{AppState, SseEvent};

/// 纯遥测设备(只上报 /telemetry、从不开 SSE)的心跳保留期。超过该时长仍无新心跳即
/// 从 last_heartbeat / heartbeat_miss_count 淘汰,防止两张 DashMap 无界增长。
const TELEMETRY_HEARTBEAT_TTL_SECS: u64 = 300;

pub async fn run(state: AppState, mut shutdown_rx: broadcast::Receiver<()>) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                scan(&state).await;
            }
            _ = shutdown_rx.recv() => break,
        }
    }
}

async fn scan(state: &AppState) {
    // 淘汰路径须覆盖写入路径的全部 key:SSE 设备(active_sse)+ 纯遥测设备
    // (仅 last_heartbeat,从不开 SSE)。仅迭代 active_sse 会让纯遥测心跳永不淘汰。
    let device_ids: HashSet<String> = state
        .active_sse()
        .iter()
        .map(|e| e.key().clone())
        .chain(state.last_heartbeat().iter().map(|e| e.key().clone()))
        .collect();

    for device_id in device_ids {
        let has_sse = state
            .active_sse()
            .get(&device_id)
            .map(|c| !c.is_empty())
            .unwrap_or(false);

        // 无活跃 SSE(纯遥测设备或已空连接):收不到 DataCorrupted,按心跳超期直接淘汰。
        if !has_sse {
            let stale = state
                .last_heartbeat()
                .get(&device_id)
                .map(|t| t.elapsed() > Duration::from_secs(TELEMETRY_HEARTBEAT_TTL_SECS))
                .unwrap_or(true);
            if stale {
                state.last_heartbeat().remove(&device_id);
                state.heartbeat_miss_count().remove(&device_id);
            }
            continue;
        }

        let is_banned = match state
            .run_store_task("worker.heartbeat_watchdog.is_device_banned", {
                let device_id = device_id.clone();
                move |store| store.is_device_banned(&device_id)
            })
            .await
        {
            Ok(Ok(value)) => value,
            Ok(Err(e)) => {
                tracing::warn!(device_id = %device_id, error = %e, "Heartbeat watchdog ban check failed");
                false
            }
            Err(e) => {
                tracing::warn!(device_id = %device_id, error = %e, "Heartbeat watchdog ban check task failed");
                false
            }
        };

        if is_banned {
            continue;
        }

        let missed = state
            .last_heartbeat()
            .get(&device_id)
            .map(|t| t.elapsed() > Duration::from_secs(10))
            .unwrap_or(true);

        if missed {
            let count = state
                .heartbeat_miss_count()
                .get(&device_id)
                .map(|v| *v)
                .unwrap_or(0)
                .saturating_add(1);
            state
                .heartbeat_miss_count()
                .insert(device_id.clone(), count);

            if count >= 5 {
                send_data_corrupted(state, &device_id);
                state.heartbeat_miss_count().insert(device_id, 0);
            }
        } else {
            state.heartbeat_miss_count().insert(device_id, 0);
        }
    }
}

fn send_data_corrupted(state: &AppState, device_id: &str) {
    if let Some(conns) = state.active_sse().get(device_id) {
        for conn in conns.value() {
            let _ = conn.tx.send(SseEvent::DataCorrupted);
        }
    }
}
