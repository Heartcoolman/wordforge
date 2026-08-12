//! m073：客户端埋点命名事件流摄取（`POST /api/telemetry/app-events`，批量 ≤50）。
//!
//! 与 `/api/telemetry` 的分工：那边是心跳/摘要通道（单事件、eventType 三值枚举、summary
//! 投影），这里是离散命名事件（behavior/error/perf + name + props）。校验流水照抄
//! telemetry.rs 顺序：device-id 格式 → per-user 限频（软丢弃）→ 头校验 → owner 三态；
//! **不做 device upsert、不刷心跳**（职责仍归 /api/telemetry）。逐条校验镜像
//! learning ingest_session_events：部分成功 + errors[] 逐条错误，绝不整批 400。
//! 本流与 records/AMAS 管线完全隔离（clientEventId===clientRecordId 不变式不适用）。

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::auth::AuthUser;
use crate::extractors::JsonBody;
use crate::response::{ok, AppError};
use crate::routes::telemetry::{record_ingest_rejection, sampling_bucket};
use crate::state::AppState;
use crate::store::operations::app_events::AppEventRow;

pub(crate) const APP_EVENTS_BATCH_LIMIT: usize = 50;
const APP_EVENT_ID_MAX_LEN: usize = 128;
const APP_EVENT_NAME_MAX_LEN: usize = 64;
const APP_EVENT_PROPS_MAX_KEYS: usize = 16;
const APP_EVENT_PROPS_STR_MAX_CHARS: usize = 128;
const APP_EVENT_PROPS_MAX_BYTES: usize = 2048;
/// clientTsMs 回填钳制窗（7d）：离线队列可滞留数天，比 learning 事件的 30min 宽是
/// **有意为之**（埋点时间真值分析价值高且不进 AMAS），与 app_event_rollup 的 7 天
/// 重算窗一致——勿"统一"回 30min。
const APP_EVENT_TS_BACKFILL_LIMIT_DAYS: i64 = 7;
const APP_EVENT_TS_FUTURE_TOLERANCE_MIN: i64 = 5;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEventUpload {
    client_event_id: String,
    name: String,
    category: String,
    client_ts_ms: i64,
    props: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEventsRequest {
    events: Vec<AppEventUpload>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppEventIngestError {
    index: usize,
    client_event_id: String,
    code: &'static str,
    message: String,
}

/// name 词表纪律：`^[a-z0-9_]{1,64}$`。name 是 rollup 键，自由字符串只进 props。
fn is_valid_event_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= APP_EVENT_NAME_MAX_LEN
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// props 校验：对象、≤16 键、值仅标量（字符串 ≤128 字符）、序列化 ≤2KB。
/// 返回序列化好的 JSON；None 表示违规（错误码由调用方决定）。
fn validate_props(props: &serde_json::Value) -> Option<String> {
    let obj = props.as_object()?;
    if obj.len() > APP_EVENT_PROPS_MAX_KEYS {
        return None;
    }
    for v in obj.values() {
        match v {
            serde_json::Value::String(s) if s.chars().count() > APP_EVENT_PROPS_STR_MAX_CHARS => {
                return None
            }
            serde_json::Value::Array(_) | serde_json::Value::Object(_) => return None,
            _ => {}
        }
    }
    let json = serde_json::to_string(props).ok()?;
    (json.len() <= APP_EVENT_PROPS_MAX_BYTES).then_some(json)
}

/// clientTsMs 钳制到 [now-7d, now+5min]，返回 (钳制后 ms, event_day)。
fn clamp_client_ts(client_ts_ms: i64, now: chrono::DateTime<chrono::Utc>) -> (i64, String) {
    let lower = now - chrono::Duration::days(APP_EVENT_TS_BACKFILL_LIMIT_DAYS);
    let upper = now + chrono::Duration::minutes(APP_EVENT_TS_FUTURE_TOLERANCE_MIN);
    let clamped = client_ts_ms.clamp(lower.timestamp_millis(), upper.timestamp_millis());
    let day = chrono::DateTime::from_timestamp_millis(clamped)
        .unwrap_or(now)
        .format("%Y-%m-%d")
        .to_string();
    (clamped, day)
}

/// platform 是 rollup 键：trim + 小写 + 仅 [a-z0-9_-] + ≤16 字符，规避自由串基数。
fn normalize_platform(raw: &str) -> String {
    raw.trim()
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_' || *c == '-')
        .take(16)
        .collect()
}

pub async fn submit_app_events(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    JsonBody(body): JsonBody<AppEventsRequest>,
) -> Result<impl IntoResponse, AppError> {
    let device_id = headers
        .get("x-device-id")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("MISSING_DEVICE_ID", "缺少 X-Device-Id 请求头"))?
        .to_string();
    // 中间件对 /telemetry 跳过格式校验与 owner 认领，此处自校验（同 submit_telemetry）。
    if !crate::middleware::device::is_valid_device_id(&device_id) {
        return Err(AppError::bad_request(
            "INVALID_DEVICE_ID",
            "x-device-id 格式非法",
        ));
    }

    // per-user 限频闸门上移到任何 DB 操作之前（同 submit_telemetry 的 W3-1）；
    // 独立 "ae:" 键空间，不与 /api/telemetry 心跳共享配额。命中软丢弃、零 DB 写。
    let throttle_key = format!("ae:{}", auth.user_id);
    let max_entries = state.config().limits.rate_limit_max_entries;
    let telemetry_max = state.config().telemetry_rate_limit.max_requests;
    let throttle = state
        .telemetry_rate_limit()
        .limiter
        .check_with_max(&throttle_key, max_entries, telemetry_max)
        .await;
    if !throttle.allowed {
        return Ok(ok(serde_json::json!({
            "accepted": 0, "duplicates": 0, "sampledOut": 0, "failed": 0,
            "throttled": true
        })));
    }

    let platform = match headers
        .get("x-device-platform")
        .and_then(|v| v.to_str().ok())
        .map(normalize_platform)
        .filter(|s| !s.is_empty() && s != "unknown")
    {
        Some(p) => p,
        None => {
            record_ingest_rejection(&state, "MISSING_OS", Some(&device_id), Some(&auth.user_id));
            return Err(AppError::bad_request(
                "MISSING_OS",
                "缺少 x-device-platform 头（客户端类型）",
            ));
        }
    };
    let app_version = match headers
        .get("x-app-version")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(v) => v.chars().take(32).collect::<String>(),
        None => {
            record_ingest_rejection(
                &state,
                "MISSING_APP_VERSION",
                Some(&device_id),
                Some(&auth.user_id),
            );
            return Err(AppError::bad_request(
                "MISSING_APP_VERSION",
                "缺少 x-app-version 头（版本号）",
            ));
        }
    };

    if body.events.len() > APP_EVENTS_BATCH_LIMIT {
        return Err(AppError::bad_request(
            "APP_EVENTS_TOO_LARGE",
            &format!("events 数量上限为 {APP_EVENTS_BATCH_LIMIT}"),
        ));
    }

    // 设备归属三态（照抄 report_resource_pack_install）：不 upsert、不刷心跳、不认领。
    let owner = state
        .run_store_task("app_events.device_owner", {
            let did = device_id.clone();
            move |store| store.get_client_device_owner(&did)
        })
        .await??;
    match owner {
        None => {
            record_ingest_rejection(
                &state,
                "DEVICE_NOT_REGISTERED",
                Some(&device_id),
                Some(&auth.user_id),
            );
            return Err(AppError {
                status: StatusCode::FORBIDDEN,
                code: "DEVICE_NOT_REGISTERED".into(),
                message: "设备未注册，请先正常登录使用后再上报埋点".into(),
                is_operational: true,
            });
        }
        Some(Some(existing)) if existing != auth.user_id => {
            record_ingest_rejection(
                &state,
                "DEVICE_OWNERSHIP_MISMATCH",
                Some(&device_id),
                Some(&auth.user_id),
            );
            return Err(AppError {
                status: StatusCode::FORBIDDEN,
                code: "DEVICE_OWNERSHIP_MISMATCH".into(),
                message: "设备归属与当前账号不符".into(),
                is_operational: true,
            });
        }
        _ => {}
    }

    // 采样率一批一取（非逐条查库）；error 类恒不采样。
    let needs_rate = body
        .events
        .iter()
        .any(|e| e.category == "behavior" || e.category == "perf");
    let (behavior_rate, perf_rate) = if needs_rate {
        state
            .run_store_task("app_events.sample_rate", move |store| {
                Ok::<_, crate::store::StoreError>((
                    store.effective_sample_rate("app_behavior")?,
                    store.effective_sample_rate("app_perf")?,
                ))
            })
            .await??
    } else {
        (1.0, 1.0)
    };

    let now = chrono::Utc::now();
    let mut errors: Vec<AppEventIngestError> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut sampled_out = 0usize;
    let mut rows: Vec<AppEventRow> = Vec::new();

    for (index, event) in body.events.into_iter().enumerate() {
        let client_event_id = event.client_event_id.trim().to_string();
        let mut push_err = |code: &'static str, message: &str, ceid: &str| {
            errors.push(AppEventIngestError {
                index,
                client_event_id: ceid.to_string(),
                code,
                message: message.to_string(),
            });
        };

        if client_event_id.is_empty() || client_event_id.len() > APP_EVENT_ID_MAX_LEN {
            push_err(
                "APP_EVENT_INVALID_ID",
                "clientEventId 不能为空且长度 ≤128",
                &client_event_id,
            );
            continue;
        }
        if !seen.insert(client_event_id.clone()) {
            push_err(
                "APP_EVENT_DUPLICATE_ID",
                "同一请求内 clientEventId 不能重复",
                &client_event_id,
            );
            continue;
        }
        if !is_valid_event_name(&event.name) {
            push_err(
                "APP_EVENT_INVALID_NAME",
                "name 必须匹配 ^[a-z0-9_]{1,64}$",
                &client_event_id,
            );
            continue;
        }
        if !matches!(event.category.as_str(), "behavior" | "error" | "perf") {
            push_err(
                "APP_EVENT_INVALID_CATEGORY",
                "category 必须为 behavior|error|perf",
                &client_event_id,
            );
            continue;
        }
        if event.client_ts_ms <= 0 {
            push_err(
                "APP_EVENT_INVALID_TS",
                "clientTsMs 必须大于 0",
                &client_event_id,
            );
            continue;
        }
        let props_json = match &event.props {
            None => None,
            Some(serde_json::Value::Null) => None,
            Some(p) => match validate_props(p) {
                Some(json) => Some(json),
                None => {
                    push_err(
                        "APP_EVENT_INVALID_PROPS",
                        "props 须为 ≤16 键的标量对象（字符串 ≤128 字符、序列化 ≤2KB）",
                        &client_event_id,
                    );
                    continue;
                }
            },
        };

        // 采样门（逐条确定性哈希，同 (device_id, clientEventId) 复现同一判定）。
        let rate = match event.category.as_str() {
            "behavior" => behavior_rate,
            "perf" => perf_rate,
            _ => 1.0,
        };
        if rate < 1.0 && sampling_bucket(&device_id, &client_event_id) >= rate {
            sampled_out += 1;
            continue;
        }

        let (client_ts_ms, event_day) = clamp_client_ts(event.client_ts_ms, now);
        rows.push(AppEventRow {
            device_id: device_id.clone(),
            user_id: auth.user_id.clone(),
            platform: platform.clone(),
            app_version: app_version.clone(),
            category: event.category,
            name: event.name,
            client_event_id,
            client_ts_ms,
            event_day,
            props_json,
        });
    }

    let (accepted, duplicates) = if rows.is_empty() {
        (0, 0)
    } else {
        state
            .run_store_task("app_events.insert_batch", move |store| {
                store.insert_app_events_batch(&rows)
            })
            .await??
    };

    Ok(ok(serde_json::json!({
        "accepted": accepted,
        "duplicates": duplicates,
        "sampledOut": sampled_out,
        "failed": errors.len(),
        "errors": errors,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_name_regex_discipline() {
        assert!(is_valid_event_name("screen_view"));
        assert!(is_valid_event_name("api_rtt"));
        assert!(!is_valid_event_name(""));
        assert!(!is_valid_event_name("Screen_View"));
        assert!(!is_valid_event_name("scr een"));
        assert!(!is_valid_event_name("名字"));
        assert!(!is_valid_event_name(&"a".repeat(65)));
    }

    #[test]
    fn props_scalar_object_only() {
        let ok_props = serde_json::json!({"screen": "home", "ms": 120, "cold": true, "x": null});
        assert!(validate_props(&ok_props).is_some());
        assert!(validate_props(&serde_json::json!("str")).is_none());
        assert!(validate_props(&serde_json::json!({"nested": {"a": 1}})).is_none());
        assert!(validate_props(&serde_json::json!({"arr": [1, 2]})).is_none());
        let long = "x".repeat(APP_EVENT_PROPS_STR_MAX_CHARS + 1);
        assert!(validate_props(&serde_json::json!({ "s": long })).is_none());
        let many: serde_json::Map<String, serde_json::Value> = (0..17)
            .map(|i| (format!("k{i}"), serde_json::json!(1)))
            .collect();
        assert!(validate_props(&serde_json::Value::Object(many)).is_none());
    }

    #[test]
    fn client_ts_clamped_to_seven_day_window() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-08-11T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        // 窗口内原样保留
        let in_window = now.timestamp_millis() - 3 * 86_400_000;
        let (ms, day) = clamp_client_ts(in_window, now);
        assert_eq!(ms, in_window);
        assert_eq!(day, "2026-08-08");
        // 过古老 → 钳到 now-7d;过未来 → 钳到 now+5min
        let (old_ms, old_day) = clamp_client_ts(1, now);
        assert_eq!(old_ms, (now - chrono::Duration::days(7)).timestamp_millis());
        assert_eq!(old_day, "2026-08-04");
        let far_future = now.timestamp_millis() + 86_400_000;
        let (fut_ms, fut_day) = clamp_client_ts(far_future, now);
        assert_eq!(
            fut_ms,
            (now + chrono::Duration::minutes(5)).timestamp_millis()
        );
        assert_eq!(fut_day, "2026-08-11");
    }

    #[test]
    fn platform_normalized_for_rollup_key() {
        assert_eq!(normalize_platform(" iOS "), "ios");
        assert_eq!(normalize_platform("Android"), "android");
        assert_eq!(normalize_platform("web\u{1F600}!"), "web");
        assert_eq!(normalize_platform(&"x".repeat(40)).len(), 16);
    }
}
