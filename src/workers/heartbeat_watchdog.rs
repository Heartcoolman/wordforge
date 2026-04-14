use std::time::Duration;

use tokio::sync::broadcast;

use crate::state::{AppState, SseEvent};

pub async fn run(state: AppState, mut shutdown_rx: broadcast::Receiver<()>) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                scan(&state);
            }
            _ = shutdown_rx.recv() => break,
        }
    }
}

fn scan(state: &AppState) {
    let device_ids: Vec<String> = state.active_sse().iter().map(|e| e.key().clone()).collect();

    for device_id in device_ids {
        if state.active_sse().get(&device_id).map(|c| c.is_empty()).unwrap_or(true) {
            state.last_heartbeat().remove(&device_id);
            state.heartbeat_miss_count().remove(&device_id);
            continue;
        }

        if state.store().is_device_banned(&device_id).unwrap_or(false) {
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
            state.heartbeat_miss_count().insert(device_id.clone(), count);

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
