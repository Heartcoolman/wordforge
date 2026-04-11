use std::convert::Infallible;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use axum::{extract::State, Router};
use futures::Stream;

use crate::auth::AuthUser;
use crate::response::AppError;
use crate::state::{AppState, SseClientInfo};

static SSE_CONNECTION_COUNT: AtomicUsize = AtomicUsize::new(0);

struct SseGuard {
    state: AppState,
    device_id: Option<String>,
    conn_id: String,
}

impl Drop for SseGuard {
    fn drop(&mut self) {
        SSE_CONNECTION_COUNT.fetch_sub(1, Ordering::SeqCst);
        if let Some(ref did) = self.device_id {
            let mut remove_key = false;
            if let Some(mut conns) = self.state.active_sse().get_mut(did) {
                conns.retain(|c| c.conn_id != self.conn_id);
                if conns.is_empty() {
                    remove_key = true;
                }
            }
            if remove_key {
                self.state.active_sse().remove(did);
            }
        }
    }
}

pub fn router() -> Router<AppState> {
    Router::new().route("/events", get(sse_handler))
}

pub async fn sse_handler(
    auth: AuthUser,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    let max_sse = state.config().limits.max_sse_connections;
    loop {
        let current = SSE_CONNECTION_COUNT.load(Ordering::SeqCst);
        if current >= max_sse {
            return Err(AppError::too_many_requests("SSE连接数过多"));
        }
        match SSE_CONNECTION_COUNT.compare_exchange(
            current,
            current + 1,
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) {
            Ok(_) => break,
            Err(_) => continue,
        }
    }

    let device_id = headers
        .get("x-device-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let platform = headers
        .get("x-device-platform")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    let conn_id = uuid::Uuid::new_v4().to_string();
    let (per_conn_tx, mut per_conn_rx) = tokio::sync::mpsc::unbounded_channel();

    let guard = SseGuard {
        state: state.clone(),
        device_id: device_id.clone(),
        conn_id: conn_id.clone(),
    };

    if let Some(ref did) = device_id {
        let info = SseClientInfo {
            conn_id: conn_id.clone(),
            user_id: auth.user_id.clone(),
            platform: platform.clone(),
            connected_at: Instant::now(),
            tx: per_conn_tx,
        };
        state
            .active_sse()
            .entry(did.clone())
            .or_default()
            .push(info);
    }

    let mut shutdown_rx = state.shutdown_rx();
    let mut maintenance_rx = state.maintenance_rx();
    let mut update_rx = state.update_rx();
    let user_id = auth.user_id.clone();

    let stream = async_stream::stream! {
        let _guard = guard;

        let mut interval = tokio::time::interval(Duration::from_secs(5));
        let mut last_event_count: u64 = 0;

        if let Ok(user_state) = state.amas().get_user_state(&user_id) {
            last_event_count = user_state.total_event_count;
        }

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Ok(user_state) = state.amas().get_user_state(&user_id) {
                        if user_state.total_event_count > last_event_count {
                            let event_data = serde_json::json!({
                                "type": "state_change",
                                "attention": user_state.attention,
                                "fatigue": user_state.fatigue,
                                "motivation": user_state.motivation,
                                "confidence": user_state.confidence,
                                "sessionEventCount": user_state.session_event_count,
                                "totalEventCount": user_state.total_event_count,
                            });
                            if let Ok(json) = serde_json::to_string(&event_data) {
                                yield Ok(Event::default().event("amas_state").data(json));
                            }
                            last_event_count = user_state.total_event_count;
                        }
                    }
                }
                result = maintenance_rx.recv() => {
                    if let Ok(active) = result {
                        let data = serde_json::json!({ "type": "maintenance", "active": active });
                        if let Ok(json) = serde_json::to_string(&data) {
                            yield Ok(Event::default().event("maintenance").data(json));
                        }
                    }
                }
                result = update_rx.recv() => {
                    if let Ok(payload) = result {
                        if let Ok(json) = serde_json::to_string(&payload) {
                            yield Ok(Event::default().event("update_available").data(json));
                        }
                    }
                }
                msg = per_conn_rx.recv() => {
                    match msg {
                        Some(event) => {
                            if let Ok(json) = serde_json::to_string(&event) {
                                let event_name = match &event {
                                    crate::state::SseEvent::Maintenance { .. } => "maintenance",
                                    crate::state::SseEvent::TelemetryRequest { .. } => "telemetry_request",
                                    crate::state::SseEvent::Banned => "banned",
                                    crate::state::SseEvent::Unbanned => "unbanned",
                                };
                                yield Ok(Event::default().event(event_name).data(json));
                            }
                        }
                        None => break,
                    }
                }
                _ = shutdown_rx.recv() => {
                    break;
                }
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    ))
}
