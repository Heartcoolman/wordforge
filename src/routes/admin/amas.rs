use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use axum::Router;

use crate::extractors::JsonBody;
use serde::Deserialize;

use crate::amas::types::RawEvent;
use crate::auth::{AdminAuthUser, AuthUser};
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::operations::amas_versions::ConfigVersionSource;
use chrono::Datelike;

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
        // schemars 导出：供前端 codegen 拉取，作为 TS 类型生成的单一事实源
        .route("/config/schema", get(get_config_schema))
        .route("/config/versions", get(list_versions))
        .route("/config/versions/:hash", get(get_version))
        .route("/config/versions/:hash/restore", post(restore_version))
        // m022:TOML ↔ JSON 转换端点(admin UI CodeMirror TOML 编辑器使用)
        .route("/config/parse-toml", post(parse_toml))
        .route("/config/serialize-toml", post(serialize_toml))
        // m022:百分比抽样灰度发布
        .route("/config/canary", get(get_canary).put(set_canary))
        .route(
            "/config/canary/disable",
            post(disable_canary),
        )
        .route("/metrics", get(get_metrics))
        .route("/metrics/timeseries", get(metrics_timeseries))
        .route("/monitoring", get(get_monitoring_events))
        .route("/anomalies", get(anomalies_overview))
        .route("/user-state/distribution", get(user_state_distribution))
        .route("/compare", get(compare_versions))
        .route("/suggestions", get(list_suggestions))
        .route("/suggestions/explain", post(explain_param))
        .route("/suggestions/spend", get(suggestion_spend))
        // C5: 历史导出 CSV
        .route("/suggestions/export.csv", get(export_suggestions_csv))
        .route("/suggestions/:id", get(get_suggestion))
        .route("/suggestions/:id/approve", post(approve_suggestion))
        .route("/suggestions/:id/reject", post(reject_suggestion))
        // C5: 建议回滚（版本链 restore parent）
        .route("/suggestions/:id/rollback", post(rollback_suggestion))
        // C1: advisor 成本/统计
        .route("/advisor/cost", get(advisor_cost))
        .route("/advisor/cost/daily", get(advisor_cost_daily))
        // C2: 巡查控制
        .route("/advisor/run", post(advisor_run))
        .route("/suggestions/approve-all", post(approve_all_suggestions))
        // C3: 顾问配置
        .route(
            "/advisor/config",
            get(get_advisor_config).put(update_advisor_config),
        )
        // C4: 调参白名单 CRUD
        .route("/advisor/whitelist", get(list_whitelist).post(add_whitelist))
        .route(
            "/advisor/whitelist/:path",
            axum::routing::delete(delete_whitelist),
        )
        // C6: per-patch canary 子系统
        .route("/advisor/canary", get(list_canaries).post(create_canary))
        .route("/advisor/canary/:id/scale", post(scale_canary))
        .route("/advisor/canary/:id/rollback", post(rollback_canary))
        .route("/advisor/canary/:id/promote", post(promote_canary))
}

// ─────────────────── m022:TOML 互转 + canary ───────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ParseTomlRequest {
    toml: String,
}

/// POST /config/parse-toml —— TOML 字符串 → JSON `AMASConfig`。
/// 前端 CodeMirror TOML 编辑器保存前用此校验语法 + 转 JSON 给 PUT /config。
async fn parse_toml(
    _admin: AdminAuthUser,
    JsonBody(req): JsonBody<ParseTomlRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let cfg: crate::amas::config::AMASConfig = toml::from_str(&req.toml).map_err(|e| {
        AppError::bad_request("TOML_PARSE_ERROR", &format!("TOML 解析失败:{e}"))
    })?;
    let value =
        serde_json::to_value(&cfg).map_err(|e| AppError::internal(&e.to_string()))?;
    Ok(ok(value))
}

/// POST /config/serialize-toml —— JSON `AMASConfig` → TOML 字符串。
/// 前端从 JSON 切到 TOML 视图时调一次,把当前 config 渲染成 TOML。
async fn serialize_toml(
    _admin: AdminAuthUser,
    JsonBody(cfg): JsonBody<crate::amas::config::AMASConfig>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let s = toml::to_string_pretty(&cfg)
        .map_err(|e| AppError::internal(&format!("TOML 序列化失败:{e}")))?;
    Ok(ok(serde_json::json!({ "toml": s })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetCanaryRequest {
    version_hash: String,
    /// 0..=100,按 user_id hash % 100 判定是否落入 canary 桶。
    percent: u32,
    /// 强制走 canary 的用户白名单,与 percent 抽样并存(任一命中即用 canary)。
    #[serde(default)]
    force_user_ids: Vec<String>,
}

/// PUT /config/canary —— 设置 active canary 配置。
/// 同一时刻只能有一个 active(由 wordbook_canary_config 的 unique index 强制)。
async fn set_canary(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<SetCanaryRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if req.percent > 100 {
        return Err(AppError::bad_request(
            "INVALID_PERCENT",
            "percent must be in 0..=100",
        ));
    }
    // 校验 version_hash 存在
    let vhash = req.version_hash.clone();
    let admin_id = admin.admin_id.clone();
    let exists = state
        .run_store_task("admin.amas.canary.validate", move |store| {
            store.get_amas_config_version(&vhash)
        })
        .await??;
    if exists.is_none() {
        return Err(AppError::bad_request(
            "VERSION_HASH_NOT_FOUND",
            "version_hash 在 amas_config_versions 中不存在",
        ));
    }

    let req_for_store = req;
    let admin_id_for_store = admin_id;
    let inserted = state
        .run_store_task("admin.amas.canary.set", move |store| {
            store.set_amas_canary(
                &req_for_store.version_hash,
                req_for_store.percent,
                &req_for_store.force_user_ids,
                &admin_id_for_store,
            )
        })
        .await??;
    Ok(ok(serde_json::json!({
        "canary": inserted,
    })))
}

/// GET /config/canary —— 读当前 active canary 配置(可能为 None)。
async fn get_canary(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let current = state
        .run_store_task("admin.amas.canary.get", |store| store.get_active_amas_canary())
        .await??;
    Ok(ok(serde_json::json!({ "canary": current })))
}

/// POST /config/canary/disable —— 把当前 active canary 标记为 inactive。后续
/// effective_config_for_user 永远返回 stable。
async fn disable_canary(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let cleared = state
        .run_store_task("admin.amas.canary.disable", |store| {
            store.disable_active_amas_canary()
        })
        .await??;
    Ok(ok(serde_json::json!({ "disabled": cleared })))
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

/// GET /api/admin/amas/config/schema —— 返回 AMASConfig 的 JSON Schema（由 schemars 派生）
///
/// 用途：作为前端 `admin-ui/src/types/amas.generated.ts` codegen 的事实源，
/// 任何后端结构体增删字段都会自动反映到 schema，避免手写 TS 漂移。
async fn get_config_schema(
    _admin: AdminAuthUser,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let schema = schemars::schema_for!(crate::amas::config::AMASConfig);
    Ok(ok(serde_json::to_value(&schema).unwrap_or_default()))
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
    apply_and_persist_config(
        &state,
        &admin.admin_id,
        cfg,
        ConfigVersionSource::Manual,
        q.note,
    )
    .await
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

    let cfg: crate::amas::config::AMASConfig = serde_json::from_value(detail.snapshot_json)
        .map_err(|e| AppError::internal(&format!("快照反序列化失败: {e}")))?;

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
        .run_store_task(
            "admin.amas.compare",
            move |store| -> Result<_, crate::store::StoreError> {
                let a = store.aggregate_amas_version_slice(&va)?;
                let b = store.aggregate_amas_version_slice(&vb)?;
                Ok((a, b))
            },
        )
        .await??;
    Ok(ok(serde_json::json!({"a": a, "b": b})))
}

// ─────────── PR-5: Suggestions / Advisor 路由 ───────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListSuggestionsQuery {
    status: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
    q: Option<String>,
}

async fn list_suggestions(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<ListSuggestionsQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    use crate::store::operations::amas_suggestions::SuggestionStatus;
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let offset = q.offset.unwrap_or(0);
    let status = if let Some(s) = q.status.as_deref() {
        Some(
            SuggestionStatus::parse(s)
                .map_err(|e| AppError::bad_request("BAD_STATUS", &e.to_string()))?,
        )
    } else {
        None
    };
    let keyword = q.q.clone();
    let rows = state
        .run_store_task("admin.amas.list_suggestions", move |store| {
            store.list_amas_suggestions_paged(status, limit, offset, keyword.as_deref())
        })
        .await??;
    Ok(ok(rows))
}

// ─────────── C5: 历史导出 CSV ───────────

/// CSV 字段转义：含逗号/引号/换行的值用双引号包裹，内部引号翻倍。
fn csv_cell(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

async fn export_suggestions_csv(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<ListSuggestionsQuery>,
) -> Result<Response, AppError> {
    use crate::store::operations::amas_suggestions::SuggestionStatus;
    let status = if let Some(s) = q.status.as_deref() {
        Some(
            SuggestionStatus::parse(s)
                .map_err(|e| AppError::bad_request("BAD_STATUS", &e.to_string()))?,
        )
    } else {
        None
    };
    let keyword = q.q.clone();
    let rows = state
        .run_store_task("admin.amas.export_csv", move |store| {
            store.list_amas_suggestions_paged(status, 500, 0, keyword.as_deref())
        })
        .await??;

    let mut out =
        String::from("id,created_at,based_on_version_hash,patch,rationale,cost_usd,status,decided_by\n");
    for r in &rows {
        let patch = serde_json::to_string(&r.patch_json).unwrap_or_default();
        let cost = r.cost_usd.map(|c| c.to_string()).unwrap_or_default();
        let decided_by = r.decided_by.clone().unwrap_or_default();
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            r.id,
            csv_cell(&r.created_at.to_rfc3339()),
            csv_cell(&r.based_on_version_hash),
            csv_cell(&patch),
            csv_cell(&r.rationale),
            csv_cell(&cost),
            r.status.as_str(),
            csv_cell(&decided_by),
        ));
    }

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"amas-suggestions.csv\"",
        )
        .body(Body::from(out))
        .map_err(|e| AppError::internal(&e.to_string()))
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

/// C2 复用核心：校验 pending → validate_patch → 应用 patch → 落版本 → 标记 approved。
/// approve_suggestion 单条端点与 approve-all 批量端点共用，确保白名单校验一致。
/// 返回 apply_and_persist_config 的版本响应（单条端点据此保留 {updated,versionHash,versionId} 契约）。
pub(crate) async fn approve_one(
    state: &AppState,
    admin_id: &str,
    id: i64,
    note: Option<&str>,
) -> Result<axum::response::Response, AppError> {
    use crate::amas::tuning_whitelist::validate_patch;
    use crate::store::operations::amas_suggestions::SuggestionStatus;

    let suggestion = state
        .run_store_task("admin.amas.approve_lookup", move |store| {
            store.get_amas_suggestion(id)
        })
        .await??
        .ok_or_else(|| AppError::not_found("建议不存在"))?;

    if !matches!(suggestion.status, SuggestionStatus::Pending) {
        return Err(AppError::bad_request("BAD_STATUS", "仅 pending 建议可被批准"));
    }

    // 校验 patch（防止数据库篡改）—— 白名单从 store 读
    let patch_obj = suggestion
        .patch_json
        .as_object()
        .ok_or_else(|| AppError::internal("patch_json 非对象"))?
        .clone();
    let patch_for_validate = patch_obj.clone();
    let errs = state
        .run_store_task("admin.amas.approve_validate", move |store| {
            Ok::<_, crate::store::StoreError>(validate_patch(&store, &patch_for_validate))
        })
        .await??;
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
        serde_json::from_value(cfg_value).map_err(|e| {
            AppError::bad_request("PATCH_INVALID", &format!("应用 patch 后反序列化失败: {e}"))
        })?;

    let resp = apply_and_persist_config(
        state,
        admin_id,
        new_cfg,
        ConfigVersionSource::LlmSuggested,
        Some(format!("approve suggestion#{}", id)),
    )
    .await?;

    let admin_id_owned = admin_id.to_string();
    let note_owned = note.map(|s| s.to_string());
    state
        .run_store_task("admin.amas.approve_update", move |store| {
            store.update_amas_suggestion_status(
                id,
                SuggestionStatus::Approved,
                Some(&admin_id_owned),
                note_owned.as_deref(),
            )
        })
        .await??;

    Ok(resp)
}

async fn approve_suggestion(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    JsonBody(body): JsonBody<DecisionBody>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    approve_one(&state, &admin.admin_id, id, body.note.as_deref()).await
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

// ─────────── C5: 建议回滚（版本链 restore parent）───────────

/// 回滚某条已批准建议引入的配置改动：定位该建议 approve 产出的版本，restore 其 parent 版本快照，
/// 并把建议标记为 superseded（复用现有 apply_and_persist_config restore 通路 + 审计）。
async fn rollback_suggestion(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    use crate::store::operations::amas_suggestions::SuggestionStatus;

    // 先确认建议存在（不存在 → 404）
    state
        .run_store_task("admin.amas.rollback_lookup", move |store| {
            store.get_amas_suggestion(id)
        })
        .await??
        .ok_or_else(|| AppError::not_found("建议不存在"))?;

    // 定位该建议 approve 产出的版本（note == "approve suggestion#{id}"），回滚目标 = 其 parent 版本。
    let approve_note = format!("approve suggestion#{id}");
    let parent_hash = state
        .run_store_task("admin.amas.rollback_find_version", move |store| {
            let versions = store.list_amas_config_versions(500)?;
            Ok::<_, crate::store::StoreError>(
                versions
                    .into_iter()
                    .find(|v| v.note.as_deref() == Some(approve_note.as_str()))
                    .and_then(|v| v.parent_version_hash),
            )
        })
        .await??
        .ok_or_else(|| {
            AppError::bad_request(
                "PARENT_NOT_FOUND",
                "该建议无可回滚的父版本（版本链缺失或为创世版本）",
            )
        })?;

    let lookup_hash = parent_hash.clone();
    let detail = state
        .run_store_task("admin.amas.rollback_target", move |store| {
            store.get_amas_config_version(&lookup_hash)
        })
        .await??
        .ok_or_else(|| AppError::bad_request("PARENT_NOT_FOUND", "回滚目标版本不存在于版本链"))?;

    let cfg: crate::amas::config::AMASConfig = serde_json::from_value(detail.snapshot_json)
        .map_err(|e| AppError::internal(&format!("快照反序列化失败: {e}")))?;

    apply_and_persist_config(
        &state,
        &admin.admin_id,
        cfg,
        ConfigVersionSource::Manual,
        Some(format!(
            "rollback suggestion#{id} → {}",
            &parent_hash[..parent_hash.len().min(8)]
        )),
    )
    .await?;

    let admin_id = admin.admin_id.clone();
    state
        .run_store_task("admin.amas.rollback_mark", move |store| {
            store.update_amas_suggestion_status(
                id,
                SuggestionStatus::Superseded,
                Some(&admin_id),
                Some("rolled back to parent version"),
            )
        })
        .await??;

    Ok(ok(serde_json::json!({
        "rolledBack": true,
        "versionHash": parent_hash,
    })))
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
            messages: vec![ChatMessage {
                role: "user".into(),
                content: prompt,
            }],
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

// ─────────── C4: 调参白名单 CRUD ───────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddWhitelistBody {
    path: String,
    min_safe: f64,
    max_safe: f64,
}

async fn list_whitelist(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let rows = state
        .run_store_task("admin.amas.list_whitelist", |store| {
            store.list_tuning_whitelist()
        })
        .await??;
    Ok(ok(rows))
}

async fn add_whitelist(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(body): JsonBody<AddWhitelistBody>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // path 必须是 memoryModel.* 命名空间（与 Tier-A 白名单语义一致，拒绝越界命名空间）
    if !body.path.starts_with("memoryModel.") {
        return Err(AppError::bad_request(
            "INVALID_PATH",
            "白名单 path 必须以 memoryModel. 开头",
        ));
    }
    if body.min_safe >= body.max_safe {
        return Err(AppError::bad_request(
            "INVALID_RANGE",
            "minSafe 必须小于 maxSafe",
        ));
    }
    let admin_id = admin.admin_id.clone();
    let row = state
        .run_store_task("admin.amas.add_whitelist", move |store| {
            store.insert_tuning_whitelist(&body.path, body.min_safe, body.max_safe, &admin_id)
        })
        .await??;
    Ok(ok(row))
}

async fn delete_whitelist(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let deleted = state
        .run_store_task("admin.amas.delete_whitelist", move |store| {
            store.delete_tuning_whitelist(&path)
        })
        .await??;
    Ok(ok(serde_json::json!({ "deleted": deleted })))
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
            let Some(obj) = cur.as_object_mut() else {
                return;
            };
            let Some(arr_val) = obj.get_mut(key) else {
                return;
            };
            let Some(arr) = arr_val.as_array_mut() else {
                return;
            };
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
            let Some(obj) = cur.as_object_mut() else {
                return;
            };
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
    // 前端 MonitoringPage 期望 { timestamp, eventType, data } 包装结构
    let wrapped: Vec<serde_json::Value> = events
        .into_iter()
        .map(|ev| {
            let timestamp = ev
                .get("timestamp")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let event_type = ev
                .get("eventType")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            serde_json::json!({
                "timestamp": timestamp,
                "eventType": event_type,
                "data": ev,
            })
        })
        .collect();
    Ok(ok(wrapped))
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
                            retention: if count > 0 {
                                Some(sum / count as f64)
                            } else {
                                None
                            },
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

// ─────────── C1: advisor 成本 / 统计 ───────────

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AdvisorCostStats {
    month_yuan: f64,
    month_cap_yuan: f64,
    quota_pct: f64,
    forecast_yuan: f64,
    avg7d_cost_yuan: f64,
    month_calls: i64,
    accepted_count: i64,
    rejected_count: i64,
    acceptance_rate: f64,
}

async fn advisor_cost(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let usd_to_cny = state.config().llm.usd_to_cny_rate;
    let month = chrono::Utc::now().format("%Y-%m").to_string();
    let stats = state
        .run_store_task("admin.amas.advisor_cost", move |store| {
            let month_yuan = store.get_llm_cost_this_month(&month)?;
            let settings = store.get_system_settings()?;
            let (approved, rejected) = store.aggregate_suggestion_acceptance()?;
            // 本月调用次数 + 近 7 天¥成本（用于均单次 + 预测）
            let daily = store.aggregate_daily_suggestion_cost_yuan(7, usd_to_cny)?;
            let counts = store.count_suggestions_by_status()?;
            let month_calls: i64 = counts.iter().map(|(_, c)| *c).sum();
            Ok::<_, crate::store::StoreError>((
                month_yuan,
                settings.llm_advisor_max_cost_per_month_yuan,
                approved,
                rejected,
                daily,
                month_calls,
            ))
        })
        .await??;

    let (month_yuan, month_cap_yuan, approved, rejected, daily7, month_calls) = stats;
    let quota_pct = if month_cap_yuan > 0.0 {
        (month_yuan / month_cap_yuan * 100.0).min(999.0)
    } else {
        0.0
    };
    // 月末预测：按当前 day-of-month 线性外推
    let now = chrono::Utc::now();
    let day = now.day().max(1) as f64;
    let days_in_month = days_in_month(now.year(), now.month()) as f64;
    let forecast_yuan = month_yuan / day * days_in_month;
    let total7: f64 = daily7.iter().map(|(_, c)| *c).sum();
    let avg7d_cost_yuan = if month_calls > 0 {
        total7 / (month_calls as f64).max(1.0)
    } else {
        0.0
    };
    let decided = approved + rejected;
    let acceptance_rate = if decided > 0 {
        approved as f64 / decided as f64
    } else {
        0.0
    };

    Ok(ok(AdvisorCostStats {
        month_yuan,
        month_cap_yuan,
        quota_pct,
        forecast_yuan,
        avg7d_cost_yuan,
        month_calls,
        accepted_count: approved,
        rejected_count: rejected,
        acceptance_rate,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CostDailyQuery {
    days: Option<i64>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CostDailyPoint {
    date: String,
    cost_yuan: f64,
}

async fn advisor_cost_daily(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<CostDailyQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let days = q.days.unwrap_or(30).clamp(1, 90);
    let usd_to_cny = state.config().llm.usd_to_cny_rate;
    let rows = state
        .run_store_task("admin.amas.advisor_cost_daily", move |store| {
            store.aggregate_daily_suggestion_cost_yuan(days, usd_to_cny)
        })
        .await??;
    let points: Vec<CostDailyPoint> = rows
        .into_iter()
        .map(|(date, cost_yuan)| CostDailyPoint { date, cost_yuan })
        .collect();
    Ok(ok(points))
}

/// 给定年月返回该月天数（用于月末成本线性外推）。
fn days_in_month(year: i32, month: u32) -> u32 {
    let (ny, nm) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let first_next = chrono::NaiveDate::from_ymd_opt(ny, nm, 1).unwrap_or_default();
    let first_this = chrono::NaiveDate::from_ymd_opt(year, month, 1).unwrap_or_default();
    (first_next - first_this).num_days() as u32
}

// ─────────── C2: 巡查控制 ───────────

async fn advisor_run(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // 触发前记录最新 suggestion id，作为"是否新产出"的基准
    let before = state
        .run_store_task("admin.amas.advisor_run.before", |store| {
            store.list_amas_suggestions(None, 1)
        })
        .await??
        .first()
        .map(|r| r.id)
        .unwrap_or(0);

    let llm_cfg = state.config().llm.clone();
    crate::workers::llm_advisor::run(state.store(), Some(&llm_cfg), state.amas(), Some(&state))
        .await;

    let latest = state
        .run_store_task("admin.amas.advisor_run.after", |store| {
            store.list_amas_suggestions(None, 1)
        })
        .await??;
    let produced = latest.first().map(|r| r.id).unwrap_or(0) > before;
    let suggestion_id = if produced {
        latest.first().map(|r| r.id)
    } else {
        None
    };
    Ok(ok(serde_json::json!({
        "produced": produced,
        "suggestionId": suggestion_id,
    })))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ApproveAllItem {
    id: i64,
    ok: bool,
    error: Option<String>,
}

async fn approve_all_suggestions(
    admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    use crate::store::operations::amas_suggestions::SuggestionStatus;

    let pending = state
        .run_store_task("admin.amas.approve_all.list", |store| {
            store.list_amas_suggestions(Some(SuggestionStatus::Pending), 500)
        })
        .await??;

    let mut results = Vec::with_capacity(pending.len());
    for s in pending {
        let id = s.id;
        match approve_one(&state, &admin.admin_id, id, None).await {
            Ok(_) => results.push(ApproveAllItem {
                id,
                ok: true,
                error: None,
            }),
            Err(e) => results.push(ApproveAllItem {
                id,
                ok: false,
                error: Some(e.message.clone()),
            }),
        }
    }
    Ok(ok(serde_json::json!({ "results": results })))
}

// ─────────── C3: 顾问配置 ───────────

/// advisor 巡查 cron（与 workers/mod.rs LlmAdvisor 注册一致，每 20 分钟）。
const ADVISOR_POLL_CRON: &str = "0 */20 * * * *";

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AdvisorConfig {
    model: String,
    poll_cron: String,
    api_key_tail: String,
    month_cap_yuan: f64,
    auto_apply_enabled: bool,
    auto_apply_max_per_day: i64,
    auto_apply_min_confidence: f64,
    grayscale_steps: [u32; 3],
    advisor_enabled: bool,
}

fn build_advisor_config(
    llm: &crate::config::LLMConfig,
    settings: &crate::store::operations::system_settings::SystemSettings,
) -> AdvisorConfig {
    let tail = if llm.api_key.len() >= 4 {
        llm.api_key[llm.api_key.len() - 4..].to_string()
    } else {
        String::new()
    };
    AdvisorConfig {
        model: llm.model.clone(),
        poll_cron: ADVISOR_POLL_CRON.to_string(),
        api_key_tail: tail,
        month_cap_yuan: settings.llm_advisor_max_cost_per_month_yuan,
        auto_apply_enabled: settings.amas_auto_apply_enabled,
        auto_apply_max_per_day: settings.amas_auto_apply_max_per_day as i64,
        auto_apply_min_confidence: settings.amas_auto_apply_min_confidence,
        grayscale_steps: settings.amas_grayscale_steps,
        advisor_enabled: settings.llm_advisor_enabled,
    }
}

async fn get_advisor_config(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let settings = state
        .run_store_task("admin.amas.advisor_config.get", |store| {
            store.get_system_settings()
        })
        .await??;
    let llm = state.config().llm.clone();
    Ok(ok(build_advisor_config(&llm, &settings)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateAdvisorConfigBody {
    month_cap_yuan: Option<f64>,
    auto_apply_enabled: Option<bool>,
    auto_apply_max_per_day: Option<i64>,
    auto_apply_min_confidence: Option<f64>,
    grayscale_steps: Option<[u32; 3]>,
    advisor_enabled: Option<bool>,
}

async fn update_advisor_config(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(body): JsonBody<UpdateAdvisorConfigBody>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // 校验灰度档位单调递增且末档 = 100
    if let Some(steps) = body.grayscale_steps {
        if !(steps[0] < steps[1] && steps[1] <= steps[2] && steps[2] == 100 && steps[0] >= 1) {
            return Err(AppError::bad_request(
                "INVALID_GRAYSCALE",
                "灰度档位需满足 1 ≤ s0 < s1 ≤ s2 且 s2 = 100",
            ));
        }
    }
    if let Some(c) = body.auto_apply_min_confidence {
        if !(0.0..=1.0).contains(&c) {
            return Err(AppError::bad_request(
                "INVALID_CONFIDENCE",
                "min_confidence 需在 0..=1",
            ));
        }
    }

    let settings = state
        .run_store_task("admin.amas.advisor_config.put", move |store| {
            let mut s = store.get_system_settings()?;
            if let Some(v) = body.month_cap_yuan {
                s.llm_advisor_max_cost_per_month_yuan = v.max(0.0);
            }
            if let Some(v) = body.auto_apply_enabled {
                s.amas_auto_apply_enabled = v;
            }
            if let Some(v) = body.auto_apply_max_per_day {
                s.amas_auto_apply_max_per_day = v.clamp(0, 100) as u32;
            }
            if let Some(v) = body.auto_apply_min_confidence {
                s.amas_auto_apply_min_confidence = v;
            }
            if let Some(v) = body.grayscale_steps {
                s.amas_grayscale_steps = v;
            }
            if let Some(v) = body.advisor_enabled {
                s.llm_advisor_enabled = v;
            }
            store.save_system_settings(&s)?;
            Ok::<_, crate::store::StoreError>(s)
        })
        .await??;

    let llm = state.config().llm.clone();
    Ok(ok(build_advisor_config(&llm, &settings)))
}

// ─────────────────── C6:per-patch canary 子系统 ───────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateCanaryRequest {
    suggestion_id: i64,
    /// 灰度初始百分比 1..=100,cohort 取 [0, percent)。
    percent: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScaleCanaryRequest {
    percent: u32,
}

/// POST /advisor/canary —— approve 一条 pending suggestion 进灰度(非直接生效)。
/// 落 canary version snapshot + 抓 stable baseline 切片 + 建 patch_canary 行(cohort [0,percent))。
async fn create_canary(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<CreateCanaryRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    use crate::store::operations::amas_suggestions::SuggestionStatus;

    if req.percent == 0 || req.percent > 100 {
        return Err(AppError::bad_request(
            "INVALID_PERCENT",
            "percent must be in 1..=100",
        ));
    }

    // 取 pending suggestion + 校验状态
    let sid = req.suggestion_id;
    let suggestion = state
        .run_store_task("admin.amas.canary.create_lookup", move |store| {
            store.get_amas_suggestion(sid)
        })
        .await??
        .ok_or_else(|| AppError::not_found("建议不存在"))?;
    if !matches!(suggestion.status, SuggestionStatus::Pending) {
        return Err(AppError::bad_request("BAD_STATUS", "仅 pending 建议可进灰度"));
    }

    // baseline:当前 stable version 的切片(灰度起点)
    let stable_hash = suggestion.based_on_version_hash.clone();
    let baseline_json = state
        .run_store_task("admin.amas.canary.baseline", move |store| {
            store.aggregate_amas_version_slice(&stable_hash)
        })
        .await?
        .map(|slice| serde_json::to_string(&slice).unwrap_or_else(|_| "{}".into()))
        .unwrap_or_else(|_| "{}".into());

    // 落 canary version snapshot:把 patch 应用到 stable 后入版本表(与 approve 同构造)
    let patch_obj = suggestion
        .patch_json
        .as_object()
        .ok_or_else(|| AppError::internal("patch_json 非对象"))?
        .clone();
    let current = state.amas().get_config();
    let mut cfg_value =
        serde_json::to_value(&current).map_err(|e| AppError::internal(&format!("ser: {e}")))?;
    for (path, value) in &patch_obj {
        write_path(&mut cfg_value, path, value.clone());
    }
    let new_cfg: crate::amas::config::AMASConfig = serde_json::from_value(cfg_value)
        .map_err(|e| AppError::bad_request("PATCH_INVALID", &format!("应用 patch 失败: {e}")))?;
    new_cfg
        .validate()
        .map_err(|e| AppError::bad_request("AMAS_INVALID_CONFIG", &e))?;
    let snapshot_json = serde_json::to_string(&new_cfg)
        .map_err(|e| AppError::internal(&format!("配置序列化失败: {e}")))?;

    let parent_hash = suggestion.based_on_version_hash.clone();
    let percent = req.percent;
    let admin_id = admin.admin_id.clone();
    let inserted = state
        .run_store_task("admin.amas.canary.create", move |store| {
            let (_vid, vhash) = store.insert_amas_config_version(
                &snapshot_json,
                &admin_id,
                ConfigVersionSource::LlmSuggested,
                Some(&format!("canary suggestion#{sid}")),
                Some(&parent_hash),
            )?;
            let id = store.insert_patch_canary(sid, &vhash, percent, 0, percent, &baseline_json)?;
            store.get_patch_canary(id)
        })
        .await??
        .ok_or_else(|| AppError::internal("canary 落库后读取失败"))?;

    Ok(ok(serde_json::to_value(&inserted).unwrap()))
}

/// GET /advisor/canary —— active+历史列表,每行附 live 切片(liveReward/liveAnomalyRate/baselineReward)。
async fn list_canaries(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let rows = state
        .run_store_task("admin.amas.canary.list", |store| {
            let canaries = store.list_patch_canaries(None)?;
            let mut out = Vec::with_capacity(canaries.len());
            for c in canaries {
                let live = store
                    .aggregate_amas_version_slice(&c.version_hash)
                    .unwrap_or_default();
                let baseline: serde_json::Value =
                    serde_json::from_str(&c.baseline_metrics_json).unwrap_or(serde_json::json!({}));
                let mut v = serde_json::to_value(&c).unwrap_or(serde_json::json!({}));
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("liveReward".into(), serde_json::json!(live.mean_reward));
                    obj.insert(
                        "liveAnomalyRate".into(),
                        serde_json::json!(live.anomaly_rate),
                    );
                    obj.insert(
                        "baselineReward".into(),
                        baseline
                            .get("meanReward")
                            .cloned()
                            .unwrap_or(serde_json::json!(0.0)),
                    );
                }
                out.push(v);
            }
            Ok::<_, crate::store::StoreError>(out)
        })
        .await??;
    Ok(ok(rows))
}

/// POST /advisor/canary/:id/scale —— 扩量到目标 percent,cohort 重算 [0,percent)。
async fn scale_canary(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    JsonBody(req): JsonBody<ScaleCanaryRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if req.percent == 0 || req.percent > 100 {
        return Err(AppError::bad_request(
            "INVALID_PERCENT",
            "percent must be in 1..=100",
        ));
    }
    let percent = req.percent;
    state
        .run_store_task("admin.amas.canary.scale", move |store| {
            store.update_patch_canary_scale(id, percent, 0, percent)
        })
        .await?
        .map_err(|e| AppError::bad_request("SCALE_FAILED", &e.to_string()))?;
    let updated = state
        .run_store_task("admin.amas.canary.scale_read", move |store| {
            store.get_patch_canary(id)
        })
        .await??
        .ok_or_else(|| AppError::not_found("canary 不存在"))?;
    Ok(ok(serde_json::to_value(&updated).unwrap()))
}

/// POST /advisor/canary/:id/rollback —— 手动回滚(status='rolled_back')。
async fn rollback_canary(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    state
        .run_store_task("admin.amas.canary.rollback", move |store| {
            store.set_patch_canary_status(id, "rolled_back")
        })
        .await?
        .map_err(|e| AppError::bad_request("ROLLBACK_FAILED", &e.to_string()))?;
    Ok(ok(serde_json::json!({ "rolledBack": true })))
}

/// POST /advisor/canary/:id/promote —— 100% → 提升 stable,status='effective'。
async fn promote_canary(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let canary = state
        .run_store_task("admin.amas.canary.promote_lookup", move |store| {
            store.get_patch_canary(id)
        })
        .await??
        .ok_or_else(|| AppError::not_found("canary 不存在"))?;
    if canary.status != "active" {
        return Err(AppError::bad_request("BAD_STATUS", "仅 active canary 可提升"));
    }
    if canary.percent != 100 {
        return Err(AppError::bad_request(
            "NOT_FULL_ROLLOUT",
            "仅 100% 灰度可提升 stable",
        ));
    }
    // 把 canary version snapshot 提升为 stable(复用 restore 通路)
    let vhash = canary.version_hash.clone();
    let vhash_lookup = vhash.clone();
    let detail = state
        .run_store_task("admin.amas.canary.promote_version", move |store| {
            store.get_amas_config_version(&vhash_lookup)
        })
        .await??
        .ok_or_else(|| AppError::internal("canary version 不存在"))?;
    let cfg: crate::amas::config::AMASConfig = serde_json::from_value(detail.snapshot_json)
        .map_err(|e| AppError::internal(&format!("快照反序列化失败: {e}")))?;
    apply_and_persist_config(
        &state,
        &admin.admin_id,
        cfg,
        ConfigVersionSource::Manual,
        Some(format!("promote canary#{id} → stable")),
    )
    .await?;
    state
        .run_store_task("admin.amas.canary.promote_status", move |store| {
            store.set_patch_canary_status(id, "effective")
        })
        .await??;
    Ok(ok(serde_json::json!({ "promoted": true, "versionHash": vhash })))
}
