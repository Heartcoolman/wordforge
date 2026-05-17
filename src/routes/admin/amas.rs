use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::Router;

use crate::extractors::JsonBody;
use serde::Deserialize;

use crate::amas::types::RawEvent;
use crate::auth::{AdminAuthUser, AuthUser};
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::operations::amas_versions::ConfigVersionSource;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/process-event", post(process_event))
        .route("/batch-process", post(batch_process))
        // B18-B24: AMAS query endpoints
        .route("/state", get(get_amas_state))
        .route("/strategy", get(get_strategy))
        .route("/phase", get(get_phase))
        .route("/learning-curve", get(get_learning_curve))
        .route("/retention-curve", get(get_retention_curve))
        .route("/intervention", get(get_intervention))
        .route("/reset", post(reset_state))
        .route("/mastery/evaluate", get(evaluate_mastery))
        .route("/visual-fatigue", post(report_visual_fatigue))
}

/// Admin-only AMAS endpoints (config, metrics, monitoring, versions, telemetry)
pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/config", get(get_config).put(update_config))
        .route("/config/versions", get(list_versions))
        .route("/config/versions/:hash", get(get_version))
        .route("/config/versions/:hash/restore", post(restore_version))
        .route("/metrics", get(get_metrics))
        .route("/metrics/timeseries", get(metrics_timeseries))
        .route("/monitoring", get(get_monitoring_events))
        .route("/anomalies", get(anomalies_overview))
        .route("/user-state/distribution", get(user_state_distribution))
        .route("/compare", get(compare_versions))
        .route("/suggestions", get(list_suggestions))
        .route("/suggestions/explain", post(explain_param))
        .route("/suggestions/spend", get(suggestion_spend))
        .route("/suggestions/:id", get(get_suggestion))
        .route("/suggestions/:id/approve", post(approve_suggestion))
        .route("/suggestions/:id/reject", post(reject_suggestion))
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProcessEventRequest {
    word_id: String,
    is_correct: bool,
    #[serde(alias = "response_time")]
    response_time: i64,
    session_id: Option<String>,
    is_quit: Option<bool>,
    dwell_time: Option<i64>,
    pause_count: Option<i32>,
    switch_count: Option<i32>,
    retry_count: Option<i32>,
    focus_loss_duration: Option<i64>,
    interaction_density: Option<f64>,
    paused_time_ms: Option<i64>,
    hint_used: Option<bool>,
    #[serde(default)]
    confused_with: Option<String>,
}

impl From<ProcessEventRequest> for RawEvent {
    fn from(value: ProcessEventRequest) -> Self {
        Self {
            word_id: value.word_id,
            is_correct: value.is_correct,
            response_time_ms: value.response_time,
            session_id: value.session_id,
            is_quit: value.is_quit.unwrap_or(false),
            dwell_time_ms: value.dwell_time,
            pause_count: value.pause_count,
            switch_count: value.switch_count,
            retry_count: value.retry_count,
            focus_loss_duration_ms: value.focus_loss_duration,
            interaction_density: value.interaction_density,
            paused_time_ms: value.paused_time_ms,
            hint_used: value.hint_used.unwrap_or(false),
            confused_with: value.confused_with,
        }
    }
}

async fn process_event(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<ProcessEventRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let result = state
        .amas()
        .process_event(&auth.user_id, req.into())
        .await?;
    Ok(ok(result))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchProcessRequest {
    events: Vec<ProcessEventRequest>,
}

async fn batch_process(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<BatchProcessRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if req.events.len() > state.config().limits.max_batch_size {
        return Err(AppError::bad_request(
            "BATCH_TOO_LARGE",
            &format!(
                "批量处理事件数量上限为{}",
                state.config().limits.max_batch_size
            ),
        ));
    }
    let mut outputs = Vec::new();
    for event in req.events {
        let result = state
            .amas()
            .process_event(&auth.user_id, event.into())
            .await?;
        outputs.push(result);
    }
    Ok(ok(
        serde_json::json!({"count": outputs.len(), "items": outputs}),
    ))
}

async fn get_config(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let cfg = state.amas().get_config();
    Ok(ok(cfg))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateConfigQuery {
    /// 可选备注，写入 amas_config_versions.note
    note: Option<String>,
}

async fn update_config(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<UpdateConfigQuery>,
    JsonBody(cfg): JsonBody<crate::amas::config::AMASConfig>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    apply_and_persist_config(&state, &admin.admin_id, cfg, ConfigVersionSource::Manual, q.note).await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestoreBody {
    note: Option<String>,
}

async fn list_versions(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<ListVersionsQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let rows = state
        .run_store_task("admin.amas.list_versions", move |store| {
            store.list_amas_config_versions(limit)
        })
        .await??;
    Ok(ok(rows))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListVersionsQuery {
    limit: Option<usize>,
}

async fn get_version(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Path(hash): Path<String>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let hash_clone = hash.clone();
    let detail = state
        .run_store_task("admin.amas.get_version", move |store| {
            store.get_amas_config_version(&hash_clone)
        })
        .await??
        .ok_or_else(|| AppError::not_found("配置版本不存在"))?;
    Ok(ok(detail))
}

async fn restore_version(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    Path(hash): Path<String>,
    JsonBody(body): JsonBody<RestoreBody>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let hash_clone = hash.clone();
    let detail = state
        .run_store_task("admin.amas.restore_lookup", move |store| {
            store.get_amas_config_version(&hash_clone)
        })
        .await??
        .ok_or_else(|| AppError::not_found("配置版本不存在"))?;

    let cfg: crate::amas::config::AMASConfig =
        serde_json::from_value(detail.snapshot_json).map_err(|e| {
            AppError::internal(&format!("快照反序列化失败: {e}"))
        })?;

    let note = body
        .note
        .unwrap_or_else(|| format!("回滚自 {}", &hash[..hash.len().min(8)]));
    apply_and_persist_config(
        &state,
        &admin.admin_id,
        cfg,
        ConfigVersionSource::Manual,
        Some(note),
    )
    .await
}

/// 内部 helper：校验 + 热重载 + 写 toml + 落版本表。
/// 三档来源（manual / llm_suggested / llm_auto）共用此函数，确保审计与回滚都一致。
pub(crate) async fn apply_and_persist_config(
    state: &AppState,
    admin_id: &str,
    cfg: crate::amas::config::AMASConfig,
    source: ConfigVersionSource,
    note: Option<String>,
) -> Result<axum::response::Response, AppError> {
    cfg.validate()
        .map_err(|e| AppError::bad_request("AMAS_INVALID_CONFIG", &e))?;

    state
        .amas()
        .reload_config(cfg.clone())
        .map_err(|e| AppError::bad_request("AMAS_INVALID_CONFIG", &e))?;

    // 写回 TOML 文件，保持文件与内存状态一致
    let toml_path = state
        .config()
        .amas_config_file
        .clone()
        .unwrap_or_else(|| "amas_config.toml".to_string());
    if let Err(e) = cfg.write_to_toml(&toml_path) {
        tracing::warn!(path = %toml_path, error = %e, "写回 AMAS 配置文件失败");
    }

    // 序列化为 canonical JSON 并落版本表
    let snapshot_json = serde_json::to_string(&cfg)
        .map_err(|e| AppError::internal(&format!("配置序列化失败: {e}")))?;

    let admin_id_owned = admin_id.to_string();
    let snapshot_for_db = snapshot_json.clone();
    let note_for_db = note.clone();
    let (version_id, version_hash) = state
        .run_store_task("admin.amas.insert_version", move |store| {
            let parent = store
                .list_amas_config_versions(1)
                .ok()
                .and_then(|mut v| v.pop())
                .map(|r| r.version_hash);
            store.insert_amas_config_version(
                &snapshot_for_db,
                &admin_id_owned,
                source,
                note_for_db.as_deref(),
                parent.as_deref(),
            )
        })
        .await??;

    tracing::info!(
        admin_id = %admin_id,
        action = "update_amas_config",
        version_id,
        version_hash = %version_hash,
        source = ?source,
        "管理员更新 AMAS 配置"
    );

    use axum::response::IntoResponse;
    Ok(ok(serde_json::json!({
        "updated": true,
        "versionHash": version_hash,
        "versionId": version_id,
    }))
    .into_response())
}

async fn get_metrics(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    Ok(ok(state.amas().metrics_registry().snapshot()))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DaysQuery {
    days: Option<u32>,
}

async fn metrics_timeseries(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<DaysQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let days = q.days.unwrap_or(7).clamp(1, 90);
    let series = state
        .run_store_task("admin.amas.metrics_timeseries", move |store| {
            store.list_amas_metrics_timeseries(days)
        })
        .await??;
    Ok(ok(series))
}

async fn anomalies_overview(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<DaysQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let days = q.days.unwrap_or(7).clamp(1, 30);
    let overview = state
        .run_store_task("admin.amas.anomalies", move |store| {
            store.aggregate_amas_anomalies(days)
        })
        .await??;
    Ok(ok(overview))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserStateDistQuery {
    days: Option<u32>,
    bins: Option<u32>,
}

async fn user_state_distribution(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<UserStateDistQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let days = q.days.unwrap_or(1).clamp(1, 30);
    let bins = q.bins.unwrap_or(20);
    let dist = state
        .run_store_task("admin.amas.user_state_dist", move |store| {
            store.aggregate_amas_user_state_distribution(days, bins)
        })
        .await??;
    Ok(ok(dist))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompareQuery {
    version_a: String,
    version_b: String,
}

async fn compare_versions(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<CompareQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let va = q.version_a.clone();
    let vb = q.version_b.clone();
    let (a, b) = state
        .run_store_task("admin.amas.compare", move |store| -> Result<_, crate::store::StoreError> {
            let a = store.aggregate_amas_version_slice(&va)?;
            let b = store.aggregate_amas_version_slice(&vb)?;
            Ok((a, b))
        })
        .await??;
    Ok(ok(serde_json::json!({"a": a, "b": b})))
}

// ─────────── PR-5: Suggestions / Advisor 路由 ───────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListSuggestionsQuery {
    status: Option<String>,
    limit: Option<usize>,
}

async fn list_suggestions(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<ListSuggestionsQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    use crate::store::operations::amas_suggestions::SuggestionStatus;
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let status = if let Some(s) = q.status.as_deref() {
        Some(SuggestionStatus::parse(s).map_err(|e| AppError::bad_request("BAD_STATUS", &e.to_string()))?)
    } else {
        None
    };
    let rows = state
        .run_store_task("admin.amas.list_suggestions", move |store| {
            store.list_amas_suggestions(status, limit)
        })
        .await??;
    Ok(ok(rows))
}

async fn get_suggestion(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let row = state
        .run_store_task("admin.amas.get_suggestion", move |store| {
            store.get_amas_suggestion(id)
        })
        .await??
        .ok_or_else(|| AppError::not_found("建议不存在"))?;
    Ok(ok(row))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DecisionBody {
    note: Option<String>,
}

async fn approve_suggestion(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    JsonBody(body): JsonBody<DecisionBody>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    use crate::amas::tuning_whitelist::validate_patch;
    use crate::store::operations::amas_suggestions::SuggestionStatus;

    let suggestion = state
        .run_store_task("admin.amas.approve_lookup", move |store| {
            store.get_amas_suggestion(id)
        })
        .await??
        .ok_or_else(|| AppError::not_found("建议不存在"))?;

    if !matches!(suggestion.status, SuggestionStatus::Pending) {
        return Err(AppError::bad_request(
            "BAD_STATUS",
            "仅 pending 建议可被批准",
        ));
    }

    // 校验 patch（防止数据库篡改）
    let patch_obj = suggestion
        .patch_json
        .as_object()
        .ok_or_else(|| AppError::internal("patch_json 非对象"))?
        .clone();
    let errs = validate_patch(&patch_obj);
    if !errs.is_empty() {
        return Err(AppError::bad_request("PATCH_INVALID", &errs.join("；")));
    }

    // 应用 patch 到当前 config
    let current = state.amas().get_config();
    let cfg_value =
        serde_json::to_value(&current).map_err(|e| AppError::internal(&format!("ser: {e}")))?;
    let mut cfg_value = cfg_value;
    for (path, value) in &patch_obj {
        write_path(&mut cfg_value, path, value.clone());
    }
    let new_cfg: crate::amas::config::AMASConfig =
        serde_json::from_value(cfg_value).map_err(|e| AppError::bad_request("PATCH_INVALID", &format!("应用 patch 后反序列化失败: {e}")))?;

    let resp = apply_and_persist_config(
        &state,
        &admin.admin_id,
        new_cfg,
        ConfigVersionSource::LlmSuggested,
        Some(format!("approve suggestion#{}", id)),
    )
    .await?;

    let admin_id = admin.admin_id.clone();
    state
        .run_store_task("admin.amas.approve_update", move |store| {
            store.update_amas_suggestion_status(id, SuggestionStatus::Approved, Some(&admin_id), body.note.as_deref())
        })
        .await??;

    Ok(resp)
}

async fn reject_suggestion(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    JsonBody(body): JsonBody<DecisionBody>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    use crate::store::operations::amas_suggestions::SuggestionStatus;
    let admin_id = admin.admin_id.clone();
    state
        .run_store_task("admin.amas.reject", move |store| {
            store.update_amas_suggestion_status(
                id,
                SuggestionStatus::Rejected,
                Some(&admin_id),
                body.note.as_deref(),
            )
        })
        .await??;
    Ok(ok(serde_json::json!({"rejected": true})))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExplainBody {
    path: String,
    current_value: serde_json::Value,
}

async fn explain_param(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(body): JsonBody<ExplainBody>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    use crate::services::llm_provider::{ChatMessage, ChatRequest, LlmProvider};

    let llm_cfg = state.config().llm.clone();
    if !llm_cfg.enabled {
        return Err(AppError::bad_request("LLM_DISABLED", "LLM 未启用"));
    }

    let provider = LlmProvider::new(&llm_cfg);
    let prompt = format!(
        "用 80 字以内中文，向运营人员解释 AMAS 参数 `{}` 当前值 {} 的含义、增大/减小会带来什么后果。直接给结论，不要寒暄。",
        body.path, body.current_value
    );
    let resp = provider
        .chat(ChatRequest {
            messages: vec![ChatMessage { role: "user".into(), content: prompt }],
            json_object: false,
            temperature: 0.3,
        })
        .await
        .map_err(|e| AppError::internal(&format!("LLM 调用失败: {e}")))?;

    let cost = provider.estimate_cost_usd(&resp.usage);
    Ok(ok(serde_json::json!({
        "explanation": resp.content,
        "model": resp.model,
        "costUsd": cost,
        "tokensInput": resp.usage.prompt_tokens,
        "tokensOutput": resp.usage.completion_tokens,
    })))
}

async fn suggestion_spend(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let (cost, tin, tout) = state
        .run_store_task("admin.amas.spend", move |store| {
            store.aggregate_amas_suggestion_spend_today()
        })
        .await??;
    let cap = state.config().llm.daily_cost_cap_usd;
    Ok(ok(serde_json::json!({
        "todayCostUsd": cost,
        "todayTokensInput": tin,
        "todayTokensOutput": tout,
        "dailyCapUsd": cap,
        "remainingUsd": (cap - cost).max(0.0),
    })))
}

/// 简化版按点分式路径写值，支持 `mem.w[0]`
fn write_path(cfg: &mut serde_json::Value, path: &str, value: serde_json::Value) {
    let parts: Vec<&str> = path.split('.').collect();
    let mut cur: &mut serde_json::Value = cfg;
    for (i, part) in parts.iter().enumerate() {
        let is_last = i == parts.len() - 1;
        if let Some(open) = part.find('[') {
            let (key, rest) = part.split_at(open);
            let idx: usize = rest[1..rest.len() - 1].parse().unwrap_or(0);
            let Some(obj) = cur.as_object_mut() else { return };
            let Some(arr_val) = obj.get_mut(key) else { return };
            let Some(arr) = arr_val.as_array_mut() else { return };
            if is_last {
                if idx < arr.len() {
                    arr[idx] = value;
                }
                return;
            }
            let Some(next) = arr.get_mut(idx) else { return };
            cur = next;
        } else if is_last {
            if let Some(obj) = cur.as_object_mut() {
                obj.insert(part.to_string(), value);
            }
            return;
        } else {
            let Some(obj) = cur.as_object_mut() else { return };
            cur = obj
                .entry(part.to_string())
                .or_insert_with(|| serde_json::Value::Object(Default::default()));
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MonitoringQuery {
    limit: Option<usize>,
}

async fn get_monitoring_events(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(query): Query<MonitoringQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = query.limit.unwrap_or(50).clamp(1, 500);
    let events = state
        .run_store_task("admin.amas.monitoring_events", move |store| {
            store.get_recent_monitoring_events(limit)
        })
        .await??;
    Ok(ok(events))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VisualFatigueRequest {
    score: f64,
}

async fn report_visual_fatigue(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<VisualFatigueRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if !(0.0..=100.0).contains(&req.score) {
        return Err(AppError::bad_request(
            "INVALID_SCORE",
            "分数必须在0到100之间",
        ));
    }
    let user_state = state
        .amas()
        .update_visual_fatigue(&auth.user_id, req.score)
        .await?;
    Ok(ok(user_state))
}

// B18: GET /api/amas/state
async fn get_amas_state(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_state = state.amas().get_user_state_async(&auth.user_id).await?;
    Ok(ok(user_state))
}

// B19: GET /api/amas/strategy
async fn get_strategy(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_state = state.amas().get_user_state_async(&auth.user_id).await?;
    let strategy = state.amas().compute_strategy_from_state(&user_state);
    Ok(ok(strategy))
}

// B20: GET /api/amas/phase
async fn get_phase(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let phase = state.amas().get_phase(&auth.user_id).await?;
    Ok(ok(serde_json::json!({"phase": phase})))
}

// B21: GET /api/amas/learning-curve
async fn get_learning_curve(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let records = state
        .run_store_task("admin.amas.learning_curve", move |store| {
            store.get_user_records(&auth.user_id, 1000)
        })
        .await??;

    // Aggregate by day
    let mut daily: std::collections::BTreeMap<String, (usize, usize)> =
        std::collections::BTreeMap::new();
    for r in &records {
        let day = r.created_at.format("%Y-%m-%d").to_string();
        let entry = daily.entry(day).or_insert((0, 0));
        entry.0 += 1;
        if r.is_correct {
            entry.1 += 1;
        }
    }

    let curve: Vec<serde_json::Value> = daily
        .iter()
        .map(|(day, (total, correct))| {
            serde_json::json!({
                "date": day,
                "total": total,
                "correct": correct,
                "accuracy": if *total > 0 { *correct as f64 / *total as f64 } else { 0.0 },
            })
        })
        .collect();

    Ok(ok(serde_json::json!({"curve": curve})))
}

const RETENTION_BUCKETS: &[u32] = &[1, 2, 4, 7, 15, 30];

fn assign_retention_bucket(days_since_learn: f64) -> Option<u32> {
    // 取最近的桶；要求至少有半天的间隔，否则视为太新不计入
    if days_since_learn < 0.5 {
        return None;
    }
    let mut best: Option<(u32, f64)> = None;
    for &b in RETENTION_BUCKETS {
        let dist = (days_since_learn - b as f64).abs();
        if best.map(|(_, bd)| dist < bd).unwrap_or(true) {
            best = Some((b, dist));
        }
    }
    best.map(|(b, _)| b)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RetentionPoint {
    days_since_learn: u32,
    retention: Option<f64>,
    sample_size: u64,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RetentionCurveResponse {
    points: Vec<RetentionPoint>,
    average_retention: Option<f64>,
}

// GET /api/amas/retention-curve - 按距首次学习天数 1/2/4/7/15/30 聚合保持率
async fn get_retention_curve(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let memory_config = state.amas().get_config().memory_model;
    let user_id = auth.user_id.clone();
    let now = chrono::Utc::now();

    let response = state
        .run_store_task(
            "amas.retention_curve",
            move |store| -> Result<_, AppError> {
                let states = store.list_all_user_word_states(&user_id)?;
                let word_ids: Vec<String> = states.iter().map(|s| s.word_id.clone()).collect();
                let mdm_states = store.batch_get_engine_mastery_mdm_states(&user_id, &word_ids)?;
                let first_times = store.first_record_times_for_words(&user_id, &word_ids)?;

                let mut sums: std::collections::HashMap<u32, (f64, u64)> =
                    std::collections::HashMap::new();
                let mut total_sum = 0.0;
                let mut total_count: u64 = 0;
                for s in &states {
                    let Some(first_at) = first_times.get(&s.word_id) else {
                        continue;
                    };
                    let days = (now - *first_at).num_seconds().max(0) as f64 / 86_400.0;
                    let Some(bucket) = assign_retention_bucket(days) else {
                        continue;
                    };
                    let retention = crate::routes::analytics::estimated_retention(
                        s,
                        mdm_states.get(&s.word_id),
                        now,
                        &memory_config,
                    );
                    let entry = sums.entry(bucket).or_insert((0.0, 0));
                    entry.0 += retention;
                    entry.1 += 1;
                    total_sum += retention;
                    total_count += 1;
                }

                let points: Vec<RetentionPoint> = RETENTION_BUCKETS
                    .iter()
                    .map(|&b| {
                        let (sum, count) = sums.get(&b).copied().unwrap_or((0.0, 0));
                        RetentionPoint {
                            days_since_learn: b,
                            retention: if count > 0 { Some(sum / count as f64) } else { None },
                            sample_size: count,
                        }
                    })
                    .collect();

                Ok(RetentionCurveResponse {
                    points,
                    average_retention: if total_count > 0 {
                        Some(total_sum / total_count as f64)
                    } else {
                        None
                    },
                })
            },
        )
        .await??;

    Ok(ok(response))
}

// B22: GET /api/amas/intervention
async fn get_intervention(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_state = state.amas().get_user_state_async(&auth.user_id).await?;
    let amas_config = state.amas().get_config();
    let iv = &amas_config.intervention;
    let mut suggestions = Vec::new();

    if user_state.fatigue > iv.fatigue_alert_threshold {
        suggestions.push(serde_json::json!({
            "type": "rest",
            "message": "您似乎有些疲劳，建议休息一下",
            "severity": "warning",
        }));
    }
    if user_state.motivation < iv.motivation_alert_threshold {
        suggestions.push(serde_json::json!({
            "type": "encouragement",
            "message": "试试更简单的单词来重建信心",
            "severity": "info",
        }));
    }
    if user_state.attention < iv.attention_alert_threshold {
        suggestions.push(serde_json::json!({
            "type": "focus",
            "message": "您的注意力似乎有所下降，建议缩短学习时间",
            "severity": "warning",
        }));
    }
    if suggestions.is_empty() {
        suggestions.push(serde_json::json!({
            "type": "continue",
            "message": "表现很棒！继续保持",
            "severity": "success",
        }));
    }

    Ok(ok(serde_json::json!({"interventions": suggestions})))
}

// B23: POST /api/amas/reset
async fn reset_state(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    state.amas().reset_user_state_async(&auth.user_id).await?;
    Ok(ok(serde_json::json!({"reset": true})))
}

// B24: GET /api/amas/mastery/evaluate
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvaluateMasteryQuery {
    word_id: String,
}

async fn evaluate_mastery(
    auth: AuthUser,
    Query(q): Query<EvaluateMasteryQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let word_id = q.word_id.clone();
    let word_state = state
        .run_store_task("admin.amas.evaluate_mastery", move |store| {
            store.get_word_learning_state(&auth.user_id, &word_id)
        })
        .await??;

    let mastery_info = match word_state {
        Some(ws) => serde_json::json!({
            "wordId": ws.word_id,
            "state": ws.state,
            "masteryLevel": ws.mastery_level,
            "correctStreak": ws.correct_streak,
            "totalAttempts": ws.total_attempts,
            "nextReviewDate": ws.next_review_date,
        }),
        None => serde_json::json!({
            "wordId": q.word_id,
            "state": "NEW",
            "masteryLevel": 0.0,
            "correctStreak": 0,
            "totalAttempts": 0,
            "nextReviewDate": null,
        }),
    };

    Ok(ok(mastery_info))
}
