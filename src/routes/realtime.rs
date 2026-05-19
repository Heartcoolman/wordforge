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

pub(crate) static SSE_CONNECTION_COUNT: AtomicUsize = AtomicUsize::new(0);

struct SseGuard {
    state: AppState,
    device_id: Option<String>,
    conn_id: String,
}

impl Drop for SseGuard {
    fn drop(&mut self) {
        let _ = SSE_CONNECTION_COUNT.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
            count.checked_sub(1)
        });
        if let Some(ref did) = self.device_id {
            let should_remove_device = if let Some(mut conns) = self.state.active_sse().get_mut(did)
            {
                conns.retain(|c| c.conn_id != self.conn_id);
                conns.is_empty()
            } else {
                false
            };

            if should_remove_device
                && self
                    .state
                    .active_sse()
                    .remove_if(did, |_, conns| conns.is_empty())
                    .is_some()
            {
                self.state.last_heartbeat().remove(did);
                self.state.heartbeat_miss_count().remove(did);
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
        // Initialize heartbeat timestamp to prevent cold-start miss accumulation
        state.last_heartbeat().insert(did.clone(), Instant::now());
        state.heartbeat_miss_count().insert(did.clone(), 0);
    }

    let mut shutdown_rx = state.shutdown_rx();
    let mut maintenance_rx = state.maintenance_rx();
    let mut update_rx = state.update_rx();
    let user_id = auth.user_id.clone();

    let stream = async_stream::stream! {
        let _guard = guard;

        let mut interval = tokio::time::interval(Duration::from_secs(5));
        let mut last_event_count: u64 = 0;

        if let Ok(user_state) = state.amas().get_user_state_async(&user_id).await {
            last_event_count = user_state.total_event_count;
        }

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Ok(user_state) = state.amas().get_user_state_async(&user_id).await {
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
                                    crate::state::SseEvent::DataCorrupted => "data_corrupted",
                                    crate::state::SseEvent::NewLlmSuggestion { .. } => "new_llm_suggestion",
                                    crate::state::SseEvent::ReleaseAvailable { .. } => "release_available",
                                    crate::state::SseEvent::UpdateProgress { .. } => "update_progress",
                                    crate::state::SseEvent::ProbeRequest { .. } => "probe_request",
                                    crate::state::SseEvent::ProbeConfirm { .. } => "probe_confirm",
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::amas::config::AMASConfig;
    use crate::amas::engine::AMASEngine;
    use crate::config::{
        AMASEnvConfig, AuthRateLimitConfig, Config, LLMConfig, RateLimitConfig, UpdateCheckConfig,
        WorkerConfig,
    };
    use crate::store::Store;

    fn test_state() -> AppState {
        let tmp = tempfile::tempdir().expect("tempdir");
        let config = Config {
            host: "127.0.0.1".parse().unwrap(),
            port: 3000,
            log_level: "info".to_string(),
            enable_file_logs: false,
            log_dir: "./logs".to_string(),
            database_url: tmp
                .path()
                .join("realtime_guard.db")
                .to_string_lossy()
                .to_string(),
            api_only: false,
            sqlite_busy_timeout_ms: 5000,
            sqlite_connection_timeout_ms: 250,
            sqlite_pool_size: 4,
            jwt_secret: "test-jwt-secret-abcdefghijklmnopqrstuvwxyz".to_string(),
            refresh_jwt_secret: "test-refresh-secret-abcdefghijklmnopqrstuvwxyz".to_string(),
            jwt_expires_in_hours: 24,
            refresh_token_expires_in_hours: 168,
            admin_jwt_secret: "test-admin-secret-abcdefghijklmnopqrstuvwxyz".to_string(),
            admin_jwt_expires_in_hours: 2,
            cors_origin: "http://localhost:5173".to_string(),
            trust_proxy: false,
            cookie_secure: false,
            self_watchdog: Default::default(),
            rate_limit: RateLimitConfig {
                window_secs: 60,
                max_requests: 100,
            },
            auth_rate_limit: AuthRateLimitConfig::default(),
            worker: WorkerConfig {
                is_leader: false,
                enable_llm_advisor: false,
                enable_monitoring: false,
            },
            amas: AMASEnvConfig {
                ensemble_enabled: true,
                monitor_sample_rate: 0.05,
            },
            amas_config_file: None,
            llm: LLMConfig {
                enabled: false,
                mock: true,
                api_url: String::new(),
                api_key: String::new(),
                model: String::new(),
                timeout_secs: 30,
                daily_cost_cap_usd: 1.0,
                input_price_per_mtok_usd: 0.55,
                output_price_per_mtok_usd: 2.19,
            },
            update_check: UpdateCheckConfig {
                api_url: String::new(),
                cache_ttl_secs: 3600,
                worker_enabled: false,
                worker_interval_secs: 3600,
                github_token: None,
                allow_downgrade: false,
                install_dir: None,
                max_tarball_bytes: 200 * 1024 * 1024,
                download_mirror_prefix: None,
            },
            pagination: Default::default(),
            strict_mode: Default::default(),
            limits: Default::default(),
        };
        let store = Arc::new(
            Store::open(
                &config.database_url,
                config.sqlite_busy_timeout_ms,
                config.sqlite_pool_size,
            )
            .expect("open test store"),
        );
        let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
        let (shutdown_tx, _) = tokio::sync::broadcast::channel(4);
        AppState::new(store, amas, &config, shutdown_tx, false)
    }

    #[test]
    fn sse_guard_drop_removes_empty_device_without_deadlocking() {
        let state = test_state();
        let device_id = "device-1".to_string();
        let conn_id = "conn-1".to_string();
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();

        state.active_sse().insert(
            device_id.clone(),
            vec![SseClientInfo {
                conn_id: conn_id.clone(),
                user_id: "user-1".to_string(),
                platform: "test".to_string(),
                connected_at: Instant::now(),
                tx,
            }],
        );
        state
            .last_heartbeat()
            .insert(device_id.clone(), Instant::now());
        state.heartbeat_miss_count().insert(device_id.clone(), 0);
        SSE_CONNECTION_COUNT.store(1, Ordering::SeqCst);

        let guard = SseGuard {
            state: state.clone(),
            device_id: Some(device_id.clone()),
            conn_id,
        };
        drop(guard);

        assert_eq!(SSE_CONNECTION_COUNT.load(Ordering::SeqCst), 0);
        assert!(state.active_sse().get(&device_id).is_none());
        assert!(state.last_heartbeat().get(&device_id).is_none());
        assert!(state.heartbeat_miss_count().get(&device_id).is_none());
    }
}
