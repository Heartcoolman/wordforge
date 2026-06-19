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

/// BA3a：摄取拒绝码 fire-and-forget 留痕（m061）。摄取早返(400/403)在写库前丢弃了
/// 拒绝码,此处旁路 spawn 一条 telemetry_ingest_rejections，绝不阻塞/失败响应。
fn record_ingest_rejection(
    state: &AppState,
    code: &'static str,
    device_id: Option<&str>,
    user_id: Option<&str>,
) {
    let state = state.clone();
    let device_id = device_id.map(str::to_string);
    let user_id = user_id.map(str::to_string);
    tokio::spawn(async move {
        let _ = state
            .run_store_task("telemetry.ingest_rejection", move |store| {
                store.insert_ingest_rejection(code, device_id.as_deref(), user_id.as_deref())
            })
            .await;
    });
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(submit_telemetry))
        // 无鉴权错误上报：只记 tracing log，不落库；火焰即忘，失败不影响 UX
        .route("/error", post(report_client_error))
        // v1.1-P0.8：资源包热更安装/校验/回滚 telemetry，对齐对接文档 §7.3
        .route("/resource-pack-install", post(report_resource_pack_install))
        .layer(DefaultBodyLimit::max(64 * 1024))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResourcePackInstallReport {
    pack_id: String,
    version: String,
    /// `installed` | `verify_failed` | `rollback`
    outcome: String,
    app_version: Option<String>,
}

/// v1.1-P0.8：客户端资源包安装结果上报。鉴权与既有 telemetry 一致，
/// 走 AuthUser（资源包是登录态特性）。
async fn report_resource_pack_install(
    State(state): State<AppState>,
    auth: AuthUser,
    headers: HeaderMap,
    JsonBody(body): JsonBody<ResourcePackInstallReport>,
) -> Result<impl IntoResponse, AppError> {
    let device_id = headers
        .get("x-device-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let pack_id = body.pack_id.clone();
    let version = body.version.clone();
    let outcome = body.outcome.clone();
    let app_version = body.app_version.clone();

    state
        .run_store_task("telemetry.resource_pack_install", move |store| {
            store.record_pack_install(
                &pack_id,
                &version,
                device_id.as_deref(),
                app_version.as_deref(),
                &outcome,
            )
        })
        .await??;

    tracing::info!(
        user_id = %auth.user_id,
        pack_id = %body.pack_id,
        version = %body.version,
        outcome = %body.outcome,
        "记录资源包安装事件"
    );

    Ok(ok(serde_json::json!({ "received": true })))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClientErrorReport {
    message: String,
    stack: Option<String>,
    url: Option<String>,
    user_agent: Option<String>,
    #[allow(dead_code)]
    component_stack: Option<String>,
}

async fn report_client_error(JsonBody(body): JsonBody<ClientErrorReport>) -> impl IntoResponse {
    let stack = body.stack.as_deref().unwrap_or("");
    let url = body.url.as_deref().unwrap_or("");
    let ua = body.user_agent.as_deref().unwrap_or("");
    tracing::warn!(
        message = %body.message,
        stack = %stack,
        url = %url,
        user_agent = %ua,
        "前端 ErrorBoundary 捕获异常"
    );
    ok(serde_json::json!({ "received": true }))
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

    // P1:中间件对 /telemetry 跳过 device upsert/claim,故格式校验须在此自行执行(与中间件其它
    // 端点口径一致),拒绝畸形/超长 x-device-id 进入下方 claim 路径(设备抢注的格式向量防御)。
    if !crate::middleware::device::is_valid_device_id(device_id) {
        return Err(AppError::bad_request(
            "INVALID_DEVICE_ID",
            "x-device-id 格式非法",
        ));
    }

    if body.event_type == "on_demand" && body.request_id.is_none() {
        return Err(AppError::bad_request(
            "INVALID_TELEMETRY",
            "on_demand 事件必须携带 requestId",
        ));
    }

    // m038 遥测硬识别:四要素必填(缺任一直接 400 拦截,不受 strict_mode 开关控制)。
    // 平台/版本走 header,时区/型号走 payload.device。
    let dev_platform = match headers
        .get("x-device-platform")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "unknown")
    {
        Some(p) => p,
        None => {
            record_ingest_rejection(&state, "MISSING_OS", Some(device_id), Some(&auth.user_id));
            return Err(AppError::bad_request(
                "MISSING_OS",
                "缺少 x-device-platform 头（客户端类型）",
            ));
        }
    };
    let dev_app_version = match headers
        .get("x-app-version")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(v) => v,
        None => {
            record_ingest_rejection(
                &state,
                "MISSING_APP_VERSION",
                Some(device_id),
                Some(&auth.user_id),
            );
            return Err(AppError::bad_request(
                "MISSING_APP_VERSION",
                "缺少 x-app-version 头（版本号）",
            ));
        }
    };
    let dev_obj = body.payload.get("device").and_then(|v| v.as_object());
    if dev_obj
        .and_then(|d| d.get("timezone"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_none()
    {
        record_ingest_rejection(
            &state,
            "MISSING_TIMEZONE",
            Some(device_id),
            Some(&auth.user_id),
        );
        return Err(AppError::bad_request(
            "MISSING_TIMEZONE",
            "telemetry payload 缺少 device.timezone（时区）",
        ));
    }
    let dev_model = match dev_obj
        .and_then(|d| d.get("model"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(m) => m.to_string(),
        None => {
            record_ingest_rejection(
                &state,
                "MISSING_DEVICE_MODEL",
                Some(device_id),
                Some(&auth.user_id),
            );
            return Err(AppError::bad_request(
                "MISSING_DEVICE_MODEL",
                "telemetry payload 缺少 device.model（设备型号）",
            ));
        }
    };

    // §12 strict-mode payload 级软校验：language 必填 + session_start 设备指纹。
    // timezone 已由上方四要素硬校验接管,此处不再重复;language/指纹维持受 hard_block 开关。
    let strict = state.config().strict_mode.clone();
    if strict.enabled {
        let device = body.payload.get("device").and_then(|v| v.as_object());
        let language_ok = device
            .and_then(|d| d.get("language"))
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty());

        if !language_ok {
            if let Err(e) = strict_reject_or_warn(
                &strict,
                "MISSING_LANGUAGE",
                "telemetry payload 缺少 device.language",
            ) {
                record_ingest_rejection(
                    &state,
                    "MISSING_LANGUAGE",
                    Some(device_id),
                    Some(&auth.user_id),
                );
                return Err(e);
            }
        }

        if body.event_type == "session_start" {
            let fp_ok = device.is_some_and(|d| {
                ["screenWidth", "screenHeight", "pixelRatio", "cpuCores"]
                    .iter()
                    .all(|k| d.get(*k).and_then(|v| v.as_f64()).is_some_and(|n| n > 0.0))
            });
            if !fp_ok {
                if let Err(e) = strict_reject_or_warn(
                    &strict,
                    "MISSING_DEVICE_FINGERPRINT",
                    "session_start 必须携带完整设备指纹（screenWidth/screenHeight/pixelRatio/cpuCores 均 > 0）",
                ) {
                    record_ingest_rejection(
                        &state,
                        "MISSING_DEVICE_FINGERPRINT",
                        Some(device_id),
                        Some(&auth.user_id),
                    );
                    return Err(e);
                }
            }
        }
    }

    // W3-1(P1 前移):遥测专项 per-user 限频闸门上移到任何 DB 操作之前(四要素硬校验 +
    // strict-mode 内存校验已通过)。命中即软丢弃(received:true, throttled:true),不触发
    // owner 查询、device upsert 等任何 DB 任务,杜绝单个噪声客户端用完整 upsert 写放大打满
    // 全局 SQLite 写信号量挤占学习数据写入。与 sampledOut 早返回同位。
    // 注:前移后被限流的请求不再刷新 device 活跃度(last_seen),以换取闸门前零 DB 写放大。
    let throttle_key = format!("u:{}", auth.user_id);
    let max_entries = state.config().limits.rate_limit_max_entries;
    let telemetry_max = state.config().telemetry_rate_limit.max_requests;
    let throttle = state
        .telemetry_rate_limit()
        .limiter
        .check_with_max(&throttle_key, max_entries, telemetry_max)
        .await;
    if !throttle.allowed {
        return Ok(ok(
            serde_json::json!({ "received": true, "throttled": true }),
        ));
    }

    // m038 遥测硬识别:device 必须已注册且归属一致(三态)。中间件已对本端点跳过 upsert,
    // 故此处看到的是未被覆盖的真实 owner。
    let owner = state
        .run_store_task("telemetry.device_owner", {
            let device_id = device_id.to_string();
            move |store| store.get_client_device_owner(&device_id)
        })
        .await??;
    match owner {
        None => {
            record_ingest_rejection(
                &state,
                "DEVICE_NOT_REGISTERED",
                Some(device_id),
                Some(&auth.user_id),
            );
            return Err(AppError {
                status: StatusCode::FORBIDDEN,
                code: "DEVICE_NOT_REGISTERED".into(),
                message: "设备未注册，请先正常登录使用后再上报遥测".into(),
                is_operational: true,
            });
        }
        Some(Some(existing)) if existing != auth.user_id => {
            record_ingest_rejection(
                &state,
                "DEVICE_OWNERSHIP_MISMATCH",
                Some(device_id),
                Some(&auth.user_id),
            );
            return Err(AppError {
                status: StatusCode::FORBIDDEN,
                code: "DEVICE_OWNERSHIP_MISMATCH".into(),
                message: "设备归属与当前账号不符".into(),
                is_operational: true,
            });
        }
        Some(None) => {
            // P1 首占审计:遥测路径认领一个已注册但未归属(owner=NULL)的设备 —— 这是设备抢注的
            // TOFU 向量(攻击者抢先认领受害者设备致其被锁)。记录以便检测;彻底封堵(claim 身份强绑定
            // 或遥测不认领仅记录)需产品侧 device 归属语义决策,见 telemetry 安全 follow-up backlog。
            tracing::info!(
                device_id = %device_id,
                user_id = %auth.user_id,
                "telemetry first-claim of unowned device (TOFU squat-detection audit)"
            );
        }
        // Some(Some(me))=归属一致→放行
        _ => {}
    }

    // 核验通过/claim:落库设备(平台/版本/型号/归属)并刷新 last_seen。中间件已对本端点
    // 跳过 upsert,此处全权负责;采样丢弃也照常更新设备活跃度(限流命中已在上游早返回,
    // 不会执行到此)。
    state
        .run_store_task("telemetry.upsert_device", {
            let device_id = device_id.to_string();
            let platform = dev_platform.to_string();
            let user_id = auth.user_id.clone();
            let app_version = dev_app_version.to_string();
            let model = dev_model.clone();
            move |store| {
                store.upsert_client_device_with_extras(
                    &device_id,
                    &platform,
                    &user_id,
                    Some(&app_version),
                    None,
                    None,
                    Some(&model),
                )
            }
        })
        .await??;

    // P2 数值校验:对客户端明确拒绝负数(422),覆盖 extract_summary 写库的全部 i64/f64
    // 数值字段(顶层 + device/behavior 子对象)。非对象 payload 在此跳过(无字段可取),
    // 由 extract_summary 内的 clamp 兜底——脏数据(负值/超界)绝不进 summary 表。
    if let Some(obj) = body.payload.as_object() {
        let device_obj = obj.get("device").and_then(|v| v.as_object());
        let behavior_obj = obj.get("behavior").and_then(|v| v.as_object());

        let reject_neg = |name: &str| AppError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "INVALID_PAYLOAD".into(),
            message: format!("{} 不能为负数", name),
            is_operational: true,
        };
        // (字段名, 来源对象);i64 与 f64 各一轮,统一拒绝 < 0。覆盖 extract_summary
        // 写库的全部数值字段(顶层 + device + behavior)。
        type SrcObj<'a> = Option<&'a serde_json::Map<String, serde_json::Value>>;
        let i64_fields: [(&str, SrcObj); 8] = [
            ("sessionDurationSecs", Some(obj)),
            ("errorCount", Some(obj)),
            ("cpuCores", device_obj),
            ("screenWidth", device_obj),
            ("screenHeight", device_obj),
            ("clickCount", behavior_obj),
            ("visibilityChanges", behavior_obj),
            ("routeChanges", behavior_obj),
        ];
        let f64_fields: [(&str, SrcObj); 5] = [
            ("actionsPerMin", Some(obj)),
            ("avgResponseTimeMs", Some(obj)),
            ("memoryGb", device_obj),
            ("pixelRatio", device_obj),
            ("scrollDepthPct", behavior_obj),
        ];

        for (key, src) in i64_fields {
            if let Some(v) = src.and_then(|o| o.get(key)).and_then(|v| v.as_i64()) {
                if v < 0 {
                    return Err(reject_neg(key));
                }
            }
        }
        for (key, src) in f64_fields {
            if let Some(v) = src.and_then(|o| o.get(key)).and_then(|v| v.as_f64()) {
                if v < 0.0 {
                    return Err(reject_neg(key));
                }
            }
        }
    }

    // 心跳与设备活跃度同口径:任何 owner 核验通过的合法上报即视为设备存活(不按 eventType 区分),
    // 在采样门(下方 sampled_out)之前刷新 last_heartbeat。存活判定不应受 telemetry_sample_rate 影响——
    // 否则 sample_rate<1.0 时被采样丢弃的周期心跳不刷心跳,会让 watchdog 误报 data_corrupted。
    // 注:心跳=设备存活,非"数据已落库";落库失败(下方 ??)不回滚此刷新,这超出 watchdog 职责范围。
    state
        .last_heartbeat()
        .insert(device_id.to_string(), std::time::Instant::now());
    state
        .heartbeat_miss_count()
        .insert(device_id.to_string(), 0);

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

    // 数据探针采样注入(m031):locked 事件(on_demand/session_start)与配置里 locked
    // 行恒落库;其余 event_type 按 probe_sampling_config / 全局默认 gate。rate<1.0 时用
    // 确定性哈希 hash(device_id + id) mod 10000 / 10000.0 ∈ [0,1),>=rate 则采样丢弃。
    // 默认全 1.0 → 零行为变化。
    let always_keep = event_type == "on_demand" || event_type == "session_start";
    let sampled_out = if always_keep {
        false
    } else {
        let et = event_type.clone();
        let rate = state
            .run_store_task("telemetry.sample_rate", move |store| {
                store.effective_sample_rate(&et)
            })
            .await??;
        if rate >= 1.0 {
            false
        } else {
            let bucket = sampling_bucket(&device_id_for_store, &insert_id);
            bucket >= rate
        }
    };

    if sampled_out {
        return Ok(ok(
            serde_json::json!({ "received": true, "sampledOut": true }),
        ));
    }

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

/// 确定性采样桶:hash(device_id + id) mod 10000 / 10000.0 ∈ [0,1)。
/// 同一 (device_id, id) 复现同一值,与 effective_rate 比较决定是否落库。
fn sampling_bucket(device_id: &str, id: &str) -> f64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    device_id.hash(&mut hasher);
    id.hash(&mut hasher);
    (hasher.finish() % 10_000) as f64 / 10_000.0
}

/// P2 数值兜底上界(合理量纲):计数/像素类 ≤ 1e9,时长(秒)≤ 1e9。
/// 与负值拒绝(422)互补——即便绕过早返回(如非对象 payload 或未覆盖字段),
/// clamp 也保证脏数据(负值/溢出)不进 summary 表。
const NUM_MAX_I64: i64 = 1_000_000_000;
const NUM_MAX_F64: f64 = 1_000_000_000.0;

/// 把 Option<i64> clamp 到 [0, NUM_MAX_I64];None 透传。
fn clamp_i64(v: Option<i64>) -> Option<i64> {
    v.map(|n| n.clamp(0, NUM_MAX_I64))
}

/// 把 Option<f64> clamp 到 [0.0, NUM_MAX_F64];None 与 NaN(无 as_f64 命中即 None)透传/归零。
fn clamp_f64(v: Option<f64>) -> Option<f64> {
    v.map(|n| if n.is_finite() { n.clamp(0.0, NUM_MAX_F64) } else { 0.0 })
}

/// 顶层标量(写库 NOT NULL,缺省 0)i64:非有限/负值/溢出 → clamp 到 [0, NUM_MAX_I64]。
fn clamp_i64_default(v: Option<i64>) -> i64 {
    clamp_i64(v).unwrap_or(0)
}

/// 顶层标量 f64:同上,缺省 0.0。
fn clamp_f64_default(v: Option<f64>) -> f64 {
    clamp_f64(v).unwrap_or(0.0)
}

fn extract_summary(payload: &serde_json::Value) -> TelemetrySummaryInput {
    let device = payload.get("device");
    let behavior = payload.get("behavior");

    TelemetrySummaryInput {
        cpu_cores: clamp_i64(
            device
                .and_then(|d| d.get("cpuCores"))
                .and_then(|v| v.as_i64()),
        ),
        memory_gb: clamp_f64(
            device
                .and_then(|d| d.get("memoryGb"))
                .and_then(|v| v.as_f64()),
        ),
        screen_width: clamp_i64(
            device
                .and_then(|d| d.get("screenWidth"))
                .and_then(|v| v.as_i64()),
        ),
        screen_height: clamp_i64(
            device
                .and_then(|d| d.get("screenHeight"))
                .and_then(|v| v.as_i64()),
        ),
        pixel_ratio: clamp_f64(
            device
                .and_then(|d| d.get("pixelRatio"))
                .and_then(|v| v.as_f64()),
        ),
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
        session_duration_secs: clamp_i64_default(
            payload.get("sessionDurationSecs").and_then(|v| v.as_i64()),
        ),
        actions_per_min: clamp_f64_default(
            payload.get("actionsPerMin").and_then(|v| v.as_f64()),
        ),
        error_count: clamp_i64_default(payload.get("errorCount").and_then(|v| v.as_i64())),
        avg_response_time_ms: clamp_f64_default(
            payload.get("avgResponseTimeMs").and_then(|v| v.as_f64()),
        ),
        current_route: behavior
            .and_then(|b| b.get("currentRoute"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
        click_count: clamp_i64(
            behavior
                .and_then(|b| b.get("clickCount"))
                .and_then(|v| v.as_i64()),
        ),
        click_targets_json: behavior
            .and_then(|b| b.get("clickTargets"))
            .and_then(|v| serde_json::to_string(v).ok()),
        scroll_depth_pct: clamp_f64(
            behavior
                .and_then(|b| b.get("scrollDepthPct"))
                .and_then(|v| v.as_f64()),
        ),
        visibility_changes: clamp_i64(
            behavior
                .and_then(|b| b.get("visibilityChanges"))
                .and_then(|v| v.as_i64()),
        ),
        route_changes: clamp_i64(
            behavior
                .and_then(|b| b.get("routeChanges"))
                .and_then(|v| v.as_i64()),
        ),
        feature_usage_json: payload
            .get("featureUsage")
            .and_then(|v| serde_json::to_string(v).ok())
            .unwrap_or_else(|| "{}".to_string()),
    }
}
