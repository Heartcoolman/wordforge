//! 数据探针看板(probe-telemetry)后端,挂 /api/admin/probe-telemetry。
//!
//! 诚实约束(对齐需求规格):
//!   - 4 个派生探针(click/lesson_start/word_answer/error_js)是对多源派生的观测
//!     指标,各用真实源(telemetry_summaries / learning_sessions / learning_records)
//!     聚合 24h count/EPS/lastTs。
//!   - 采样真实作用在 telemetry_events 的真实 event_type 上:看板 click 行滑杆绑定
//!     'periodic' telemetry event_type 的 sample_rate(真实 gate);learn/perf 三探针
//!     locked=true 滑杆禁用(核心数据强制 100%)。
//!   - sinks 不实现任何 retention / DELETE,retentionDays 恒 null。
//!   - REPL 后端 /api/admin/probe/* 与 src/routes/admin/probe.rs 一行不改。

use axum::extract::{Path, Query, State};
use axum::routing::{get, patch};
use axum::Router;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::AdminAuthUser;
use crate::extractors::JsonBody;
use crate::response::{ok, AppError};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/overview", get(overview))
        .route("/probes", get(probes))
        .route("/sampling", get(sampling))
        .route("/sampling/:event_type", patch(patch_sampling))
        .route("/sinks", get(sinks))
        .route("/schema", get(schema))
        .route("/audit", get(audit))
        .route("/stream", get(stream))
}

const SECS_PER_DAY: f64 = 86_400.0;

// 派生探针 → 绑定的真实 telemetry event_type(仅 click 行有真实 gate)。
const CLICK_SAMPLING_EVENT_TYPE: &str = "periodic";

/// overview / probes 共用的时间窗 Query。`days` 缺省 1(24h),clamp 到 [1, 90]。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowQuery {
    #[serde(default = "default_window_days")]
    days: i64,
}

fn default_window_days() -> i64 {
    1
}

impl WindowQuery {
    fn clamped(&self) -> i64 {
        self.days.clamp(1, 90)
    }
}

/// 派生探针静态定义。samplingEventType 仅 click(behavior)非空(绑定 'periodic'),
/// learn/perf 三探针 locked=true 滑杆禁用。
struct ProbeDef {
    key: &'static str,
    label: &'static str,
    desc: &'static str,
    group: &'static str,
    locked: bool,
    sampling_event_type: Option<&'static str>,
}

const PROBE_DEFS: [ProbeDef; 4] = [
    ProbeDef {
        key: "click",
        label: "点击行为",
        desc: "telemetry_summaries.click_count(行为遥测)",
        group: "behavior",
        locked: false,
        sampling_event_type: Some(CLICK_SAMPLING_EVENT_TYPE),
    },
    ProbeDef {
        key: "lesson_start",
        label: "开始学习",
        desc: "learning_sessions 新建(核心数据,强制 100%)",
        group: "learn",
        locked: true,
        sampling_event_type: None,
    },
    ProbeDef {
        key: "word_answer",
        label: "单词作答",
        desc: "learning_records 答题(AMAS 训练数据,强制 100%)",
        group: "learn",
        locked: true,
        sampling_event_type: None,
    },
    ProbeDef {
        key: "error_js",
        label: "前端错误",
        desc: "telemetry_summaries.error_count 累加(错误不可丢)",
        group: "perf",
        locked: true,
        sampling_event_type: None,
    },
];

// ---------------------------------------------------------------------------
// 1) GET /overview
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct KpiCount {
    value: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    total: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    delta_pct: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct KpiRate {
    value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OverviewResponse {
    generated_at: DateTime<Utc>,
    active_probes: KpiCount,
    events24h: KpiCount,
    queue_backlog: KpiCount,
    collect_error_rate: KpiRate,
}

async fn overview(
    _admin: AdminAuthUser,
    Query(q): Query<WindowQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let win = q.clamped();
    let (raw, stats) = state
        .run_store_task("admin.probe_telemetry.overview", move |store| {
            Ok::<_, crate::store::StoreError>((
                store.overview_kpis(win)?,
                [
                    store.probe_stat_click(win)?,
                    store.probe_stat_lesson_start(win)?,
                    store.probe_stat_word_answer(win)?,
                    store.probe_stat_error_js(win)?,
                ],
            ))
        })
        .await??;

    // 活跃探针 = 有数据(count24h>0)的派生探针数 / 总定义数(4)
    let active = stats.iter().filter(|s| s.count24h > 0).count() as i64;

    let delta_pct = if raw.events_prev > 0 {
        Some((raw.events_cur - raw.events_prev) as f64 / raw.events_prev as f64)
    } else {
        None
    };

    // 采集错误率 = SUM(error_count) / 总遥测事件数;无事件返回 0.0(不除零)
    let error_rate = if raw.telemetry_total > 0 {
        raw.error_sum as f64 / raw.telemetry_total as f64
    } else {
        0.0
    };

    let win_label = if win == 1 {
        "24h".to_string()
    } else {
        format!("{win}d")
    };

    Ok(ok(OverviewResponse {
        generated_at: Utc::now(),
        active_probes: KpiCount {
            value: active,
            total: Some(PROBE_DEFS.len() as i64),
            delta_pct: None,
            note: Some(format!("有 {win_label} 数据的派生探针数 / 总定义数")),
        },
        events24h: KpiCount {
            value: raw.events_cur,
            total: None,
            delta_pct,
            note: Some(format!(
                "learning_records + telemetry_events {win_label},delta 比前一 {win_label}"
            )),
        },
        queue_backlog: KpiCount {
            value: raw.queue_backlog,
            total: None,
            delta_pct: None,
            note: Some("probe_executions 未完成(completed_at IS NULL)".into()),
        },
        collect_error_rate: KpiRate {
            value: error_rate,
            note: Some(format!("SUM(error_count) / {win_label} telemetry_events 总数")),
        },
    }))
}

// ---------------------------------------------------------------------------
// 2) GET /probes
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProbeEntry {
    key: String,
    label: String,
    desc: String,
    count24h: i64,
    eps: f64,
    last_ts: Option<String>,
    sample_rate: f64,
    locked: bool,
    enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    sampling_event_type: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProbeGroup {
    group: String,
    hue: u32,
    probes: Vec<ProbeEntry>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProbesResponse {
    generated_at: DateTime<Utc>,
    groups: Vec<ProbeGroup>,
}

async fn probes(
    _admin: AdminAuthUser,
    Query(q): Query<WindowQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let win = q.clamped();
    let (stats, click_rule, global_rate) = state
        .run_store_task("admin.probe_telemetry.probes", move |store| {
            Ok::<_, crate::store::StoreError>((
                [
                    store.probe_stat_click(win)?,
                    store.probe_stat_lesson_start(win)?,
                    store.probe_stat_word_answer(win)?,
                    store.probe_stat_error_js(win)?,
                ],
                store.get_sampling_rule(CLICK_SAMPLING_EVENT_TYPE)?,
                store.global_sample_rate()?,
            ))
        })
        .await??;
    let window_secs = win as f64 * SECS_PER_DAY;

    // click 行的 sampleRate/enabled 来自 'periodic' 真实配置行;learn/perf locked 行恒 1.0。
    let entries: Vec<ProbeEntry> = PROBE_DEFS
        .iter()
        .zip(stats.iter())
        .map(|(def, stat)| {
            let (sample_rate, enabled) = if def.sampling_event_type.is_some() {
                match &click_rule {
                    Some(r) => (r.sample_rate, r.enabled),
                    None => (global_rate, true),
                }
            } else {
                (1.0, true)
            };
            ProbeEntry {
                key: def.key.to_string(),
                label: def.label.to_string(),
                desc: def.desc.to_string(),
                count24h: stat.count24h,
                eps: stat.count24h as f64 / window_secs,
                last_ts: stat.last_ts.clone(),
                sample_rate,
                locked: def.locked,
                enabled,
                sampling_event_type: def.sampling_event_type.map(str::to_string),
            }
        })
        .collect();

    // 按设计稿分组顺序 behavior / learn / perf 组装,保留组内定义顺序。
    let group_meta: [(&str, u32); 3] = [("behavior", 213), ("learn", 162), ("perf", 60)];
    let groups = group_meta
        .iter()
        .map(|(g, hue)| ProbeGroup {
            group: g.to_string(),
            hue: *hue,
            probes: PROBE_DEFS
                .iter()
                .zip(entries.iter())
                .filter(|(def, _)| def.group == *g)
                .map(|(_, e)| ProbeEntry {
                    key: e.key.clone(),
                    label: e.label.clone(),
                    desc: e.desc.clone(),
                    count24h: e.count24h,
                    eps: e.eps,
                    last_ts: e.last_ts.clone(),
                    sample_rate: e.sample_rate,
                    locked: e.locked,
                    enabled: e.enabled,
                    sampling_event_type: e.sampling_event_type.clone(),
                })
                .collect(),
        })
        .collect();

    Ok(ok(ProbesResponse {
        generated_at: Utc::now(),
        groups,
    }))
}

// ---------------------------------------------------------------------------
// 3) GET /sampling
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SamplingRuleOut {
    event_type: String,
    sample_rate: f64,
    enabled: bool,
    locked: bool,
    priority: i64,
    /// 该 telemetry event_type 在看板上绑定的派生探针 key(无则 null)。
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SamplingResponse {
    global_default: f64,
    rules: Vec<SamplingRuleOut>,
}

/// telemetry event_type → 看板派生探针 key(target),目前仅 'periodic' → 'click'。
fn sampling_target(event_type: &str) -> Option<String> {
    if event_type == CLICK_SAMPLING_EVENT_TYPE {
        Some("click".to_string())
    } else {
        None
    }
}

async fn sampling(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let (global_default, rules) = state
        .run_store_task("admin.probe_telemetry.sampling", move |store| {
            Ok::<_, crate::store::StoreError>((
                store.global_sample_rate()?,
                store.list_sampling_rules()?,
            ))
        })
        .await??;

    let rules = rules
        .into_iter()
        .map(|r| SamplingRuleOut {
            target: sampling_target(&r.event_type),
            event_type: r.event_type,
            sample_rate: r.sample_rate,
            enabled: r.enabled,
            locked: r.locked,
            priority: r.priority,
        })
        .collect();

    Ok(ok(SamplingResponse {
        global_default,
        rules,
    }))
}

// ---------------------------------------------------------------------------
// 4) PATCH /sampling/:event_type
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatchSamplingRequest {
    sample_rate: Option<f64>,
    enabled: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PatchSamplingResponse {
    event_type: String,
    sample_rate: f64,
    enabled: bool,
    locked: bool,
    updated_at: DateTime<Utc>,
}

async fn patch_sampling(
    admin: AdminAuthUser,
    Path(event_type): Path<String>,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<PatchSamplingRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if req.sample_rate.is_none() && req.enabled.is_none() {
        return Err(AppError::bad_request(
            "EMPTY_PATCH",
            "至少提供 sampleRate 或 enabled 之一",
        ));
    }
    let admin_id = admin.admin_id.clone();
    let et = event_type.clone();
    let rate = req.sample_rate;
    let enabled = req.enabled;
    let result = state
        .run_store_task("admin.probe_telemetry.patch_sampling", move |store| {
            store.upsert_sampling_rule(&et, rate, enabled, Some(&admin_id))
        })
        .await?;

    let rule = match result {
        Ok(r) => r,
        Err(crate::store::StoreError::Validation(code))
            if code == "SAMPLING_RULE_LOCKED" =>
        {
            return Err(AppError::conflict(
                "SAMPLING_RULE_LOCKED",
                "该事件采样率被锁定(核心数据强制 100%),不可修改",
            ));
        }
        Err(crate::store::StoreError::Validation(code))
            if code == "SAMPLING_RATE_OUT_OF_RANGE" =>
        {
            return Err(AppError::bad_request(
                "SAMPLING_RATE_OUT_OF_RANGE",
                "sampleRate 必须在 [0, 1] 区间",
            ));
        }
        Err(e) => return Err(e.into()),
    };

    Ok(ok(PatchSamplingResponse {
        event_type: rule.event_type,
        sample_rate: rule.sample_rate,
        enabled: rule.enabled,
        locked: rule.locked,
        updated_at: Utc::now(),
    }))
}

// ---------------------------------------------------------------------------
// 5) GET /sinks
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SinksResponse {
    generated_at: DateTime<Utc>,
    sinks: Vec<crate::store::operations::probe_telemetry::SinkStatus>,
}

async fn sinks(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let sinks = state
        .run_store_task("admin.probe_telemetry.sinks", move |store| {
            store.sinks_status()
        })
        .await??;
    Ok(ok(SinksResponse {
        generated_at: Utc::now(),
        sinks,
    }))
}

// ---------------------------------------------------------------------------
// 6) GET /schema?eventType=periodic
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SchemaQuery {
    #[serde(default = "default_schema_event_type")]
    event_type: String,
}

fn default_schema_event_type() -> String {
    "periodic".to_string()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SchemaResponse {
    event_type: String,
    sampled_at: Option<String>,
    /// 解析后的 payload(无数据时 null)。
    payload: serde_json::Value,
}

async fn schema(
    _admin: AdminAuthUser,
    Query(q): Query<SchemaQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let et = q.event_type.clone();
    let sample = state
        .run_store_task("admin.probe_telemetry.schema", move |store| {
            store.schema_sample(&et)
        })
        .await??;

    match sample {
        Some((ts, payload_json)) => {
            let payload = serde_json::from_str::<serde_json::Value>(&payload_json)
                .unwrap_or(serde_json::Value::Null);
            Ok(ok(SchemaResponse {
                event_type: q.event_type,
                sampled_at: Some(ts),
                payload,
            }))
        }
        None => Ok(ok(SchemaResponse {
            event_type: q.event_type,
            sampled_at: None,
            payload: serde_json::Value::Null,
        })),
    }
}

// ---------------------------------------------------------------------------
// 7) GET /audit?limit=20
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuditQuery {
    #[serde(default = "default_audit_limit")]
    limit: u32,
}

fn default_audit_limit() -> u32 {
    20
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AuditResponse {
    rows: Vec<crate::store::operations::probe_telemetry::SamplingAuditRow>,
}

async fn audit(
    _admin: AdminAuthUser,
    Query(q): Query<AuditQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit.clamp(1, 200);
    let rows = state
        .run_store_task("admin.probe_telemetry.audit", move |store| {
            store.list_sampling_audit(limit)
        })
        .await??;
    Ok(ok(AuditResponse { rows }))
}

// ---------------------------------------------------------------------------
// 8) GET /stream?limit=30
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StreamQuery {
    #[serde(default = "default_stream_limit")]
    limit: u32,
}

fn default_stream_limit() -> u32 {
    30
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamResponse {
    events: Vec<crate::store::operations::probe_telemetry::StreamEvent>,
}

async fn stream(
    _admin: AdminAuthUser,
    Query(q): Query<StreamQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit.clamp(1, 200);
    let events = state
        .run_store_task("admin.probe_telemetry.stream", move |store| {
            store.recent_telemetry_events(limit)
        })
        .await??;
    Ok(ok(StreamResponse { events }))
}
