use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;

use crate::auth::AuthUser;
use crate::extractors::JsonBody;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::operations::telemetry::TelemetrySummaryInput;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(submit_telemetry))
        .layer(DefaultBodyLimit::max(64 * 1024))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TelemetryRequest {
    event_type: String,
    request_id: Option<String>,
    client_ts: String,
    payload: serde_json::Value,
}

async fn submit_telemetry(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    JsonBody(body): JsonBody<TelemetryRequest>,
) -> Result<impl IntoResponse, AppError> {
    let device_id = headers
        .get("x-device-id")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("MISSING_DEVICE_ID", "缺少 X-Device-Id 请求头"))?;

    if body.event_type == "on_demand" && body.request_id.is_none() {
        return Err(AppError::bad_request(
            "INVALID_TELEMETRY",
            "on_demand 事件必须携带 requestId",
        ));
    }

    // §12 strict-mode payload 级校验：timezone / language 必填；session_start 额外要求
    // 指纹四件套（screenWidth / screenHeight / pixelRatio / cpuCores 均 > 0）。
    // hard_block=false 时仅 warn 不拒绝；启用前需客户端先上线 strict-mode 兼容版本。
    let strict = state.config().strict_mode.clone();
    if strict.enabled {
        let device = body.payload.get("device").and_then(|v| v.as_object());
        let timezone_ok = device
            .and_then(|d| d.get("timezone"))
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty());
        let language_ok = device
            .and_then(|d| d.get("language"))
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty());

        if !timezone_ok {
            strict_reject_or_warn(&strict, "MISSING_TIMEZONE", "telemetry payload 缺少 device.timezone")?;
        }
        if !language_ok {
            strict_reject_or_warn(&strict, "MISSING_LANGUAGE", "telemetry payload 缺少 device.language")?;
        }

        if body.event_type == "session_start" {
            let fp_ok = device.is_some_and(|d| {
                ["screenWidth", "screenHeight", "pixelRatio", "cpuCores"]
                    .iter()
                    .all(|k| {
                        d.get(*k)
                            .and_then(|v| v.as_f64())
                            .is_some_and(|n| n > 0.0)
                    })
            });
            if !fp_ok {
                strict_reject_or_warn(
                    &strict,
                    "MISSING_DEVICE_FINGERPRINT",
                    "session_start 必须携带完整设备指纹（screenWidth/screenHeight/pixelRatio/cpuCores 均 > 0）",
                )?;
            }
        }
    }

    if let Some(obj) = body.payload.as_object() {
        for key in ["sessionDurationSecs", "errorCount"] {
            if let Some(v) = obj.get(key).and_then(|v| v.as_i64()) {
                if v < 0 {
                    return Err(AppError {
                        status: StatusCode::UNPROCESSABLE_ENTITY,
                        code: "INVALID_PAYLOAD".into(),
                        message: format!("{} 不能为负数", key),
                        is_operational: true,
                    });
                }
            }
        }
        for key in ["actionsPerMin", "avgResponseTimeMs"] {
            if let Some(v) = obj.get(key).and_then(|v| v.as_f64()) {
                if v < 0.0 {
                    return Err(AppError {
                        status: StatusCode::UNPROCESSABLE_ENTITY,
                        code: "INVALID_PAYLOAD".into(),
                        message: format!("{} 不能为负数", key),
                        is_operational: true,
                    });
                }
            }
        }
    }

    let id = uuid::Uuid::new_v4().to_string();
    let payload_json = serde_json::to_string(&body.payload)
        .map_err(|e| AppError::internal(&format!("payload serialization: {e}")))?;

    let summary = extract_summary(&body.payload);

    let insert_id = id.clone();
    let device_id = device_id.to_string();
    let device_id_for_store = device_id.clone();
    let user_id = auth.user_id.clone();
    let event_type = body.event_type;
    let request_id = body.request_id;
    let client_ts = body.client_ts;
    state
        .run_store_task("telemetry.submit", move |store| {
            store.insert_telemetry_and_summary(
                &insert_id,
                &device_id_for_store,
                &user_id,
                &event_type,
                request_id.as_deref(),
                &payload_json,
                &client_ts,
                &summary,
            )
        })
        .await??;

    state
        .last_heartbeat()
        .insert(device_id.to_string(), std::time::Instant::now());
    state
        .heartbeat_miss_count()
        .insert(device_id.to_string(), 0);

    Ok(ok(serde_json::json!({ "id": id })))
}

/// 根据 strict_mode 配置决定拦截或仅 warn。hard_block=true 时返回 400。
fn strict_reject_or_warn(
    cfg: &crate::config::StrictModeConfig,
    code: &'static str,
    message: &'static str,
) -> Result<(), AppError> {
    if cfg.hard_block {
        Err(AppError::bad_request(code, message))
    } else {
        tracing::warn!(code, message, "telemetry strict-mode soft-block (放行)");
        Ok(())
    }
}

fn extract_summary(payload: &serde_json::Value) -> TelemetrySummaryInput {
    let device = payload.get("device");
    let behavior = payload.get("behavior");

    TelemetrySummaryInput {
        cpu_cores: device
            .and_then(|d| d.get("cpuCores"))
            .and_then(|v| v.as_i64()),
        memory_gb: device
            .and_then(|d| d.get("memoryGb"))
            .and_then(|v| v.as_f64()),
        screen_width: device
            .and_then(|d| d.get("screenWidth"))
            .and_then(|v| v.as_i64()),
        screen_height: device
            .and_then(|d| d.get("screenHeight"))
            .and_then(|v| v.as_i64()),
        pixel_ratio: device
            .and_then(|d| d.get("pixelRatio"))
            .and_then(|v| v.as_f64()),
        os_name: device
            .and_then(|d| d.get("osName"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        browser_name: device
            .and_then(|d| d.get("browserName"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        browser_version: device
            .and_then(|d| d.get("browserVersion"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        timezone: device
            .and_then(|d| d.get("timezone"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        language: device
            .and_then(|d| d.get("language"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        touch_support: device
            .and_then(|d| d.get("touchSupport"))
            .and_then(|v| v.as_bool()),
        online_status: device
            .and_then(|d| d.get("onlineStatus"))
            .and_then(|v| v.as_bool()),
        session_duration_secs: payload
            .get("sessionDurationSecs")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        actions_per_min: payload
            .get("actionsPerMin")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        error_count: payload
            .get("errorCount")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        avg_response_time_ms: payload
            .get("avgResponseTimeMs")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        current_route: behavior
            .and_then(|b| b.get("currentRoute"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        click_count: behavior
            .and_then(|b| b.get("clickCount"))
            .and_then(|v| v.as_i64()),
        click_targets_json: behavior
            .and_then(|b| b.get("clickTargets"))
            .and_then(|v| serde_json::to_string(v).ok()),
        scroll_depth_pct: behavior
            .and_then(|b| b.get("scrollDepthPct"))
            .and_then(|v| v.as_f64()),
        visibility_changes: behavior
            .and_then(|b| b.get("visibilityChanges"))
            .and_then(|v| v.as_i64()),
        route_changes: behavior
            .and_then(|b| b.get("routeChanges"))
            .and_then(|v| v.as_i64()),
        feature_usage_json: payload
            .get("featureUsage")
            .and_then(|v| serde_json::to_string(v).ok())
            .unwrap_or_else(|| "{}".to_string()),
    }
}
