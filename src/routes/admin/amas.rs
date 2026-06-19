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
        // 修改摘要"影响"列:逐字段计算的估算影响(替代静态敏感度文案)
        .route("/config/diff-impact", post(diff_impact))
        .route("/config/canary", get(get_canary).put(set_canary))
        .route("/config/canary/disable", post(disable_canary))
        .route("/metrics", get(get_metrics))
        .route("/metrics/timeseries", get(metrics_timeseries))
        .route("/metrics/kpi", get(metrics_kpi))
        .route(
            "/metrics/algorithm-distribution",
            get(metrics_algorithm_distribution),
        )
        // 看板对齐设计稿新增子面板（真实聚合，store 层已实现 aggregate_amas_*）
        .route(
            "/metrics/stage-distribution",
            get(metrics_stage_distribution),
        )
        .route("/metrics/elo-scatter", get(metrics_elo_scatter))
        .route("/metrics/mdm-heatmap", get(metrics_mdm_heatmap))
        .route(
            "/metrics/fatigue-timeseries",
            get(metrics_fatigue_timeseries),
        )
        .route(
            "/metrics/decision-histogram",
            get(metrics_decision_histogram),
        )
        .route("/monitoring", get(get_monitoring_events))
        .route("/anomalies", get(anomalies_overview))
        .route("/anomalies/feed", get(anomalies_feed))
        .route("/user-state/distribution", get(user_state_distribution))
        // 看板补充:UserState 标量均值 / 认知三轴分布 / 算法对比(accuracy+p95)
        .route("/user-state/summary", get(user_state_summary))
        .route("/cognitive/distribution", get(cognitive_distribution))
        .route("/algo-compare", get(algo_compare))
        // BA：全局策略均值 / 奖励均值 / 单词记忆强度（看板 strategy / reward / WordMastery 卡）
        .route("/strategy/summary", get(strategy_summary))
        .route("/reward/summary", get(reward_summary))
        .route("/word-mastery", get(word_mastery))
        .route("/user-state/transitions", get(user_state_transitions))
        .route("/user-state/clusters", get(user_state_clusters))
        .route("/compare", get(compare_versions))
        .route("/compare/ext", get(compare_versions_ext))
        .route("/suggestions", get(list_suggestions))
        .route("/suggestions/explain", post(explain_param))
        .route("/suggestions/spend", get(suggestion_spend))
        // C5: 历史导出 CSV
        .route("/suggestions/export.csv", get(export_suggestions_csv))
        .route("/suggestions/:id", get(get_suggestion))
        .route("/suggestions/:id/approve", post(approve_suggestion))
        .route("/suggestions/:id/reject", post(reject_suggestion))
        // 沙箱试运行：基于 telemetry baseline 回放预估 patch 影响（不落库、不改 live config）
        .route("/advisor/suggestions/:id/sandbox", post(sandbox_suggestion))
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
        .route(
            "/advisor/whitelist",
            get(list_whitelist).post(add_whitelist),
        )
        .route(
            "/advisor/whitelist/:path",
            axum::routing::delete(delete_whitelist),
        )
        // C6: per-patch canary 子系统
        .route("/advisor/canary", get(list_canaries).post(create_canary))
        .route("/advisor/canary/:id/scale", post(scale_canary))
        // BA：灰度 baseline vs canary 指标对比（看板 Canary 卡）
        .route("/advisor/canary/:id/compare", get(compare_canary_baseline))
        .route("/advisor/canary/:id/rollback", post(rollback_canary))
        .route("/advisor/canary/:id/promote", post(promote_canary))
        // T1.3: 真实留存 A/B 实验
        .route(
            "/experiments",
            get(list_experiments).post(register_experiment),
        )
        .route("/experiments/plan", post(plan_experiment))
        .route("/experiments/:id/metrics", get(experiment_metrics))
        .route("/experiments/:id/conclude", post(conclude_experiment))
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
    let cfg: crate::amas::config::AMASConfig = toml::from_str(&req.toml)
        .map_err(|e| AppError::bad_request("TOML_PARSE_ERROR", &format!("TOML 解析失败:{e}")))?;
    let value = serde_json::to_value(&cfg).map_err(|e| AppError::internal(&e.to_string()))?;
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
    // 发布策略人群过滤(crowd-filter),对齐设计稿"人群过滤"区。
    /// 仅纳入账号年龄 >= N 天的设备(过滤新注册,留空=不限)。
    #[serde(default)]
    min_account_age_days: Option<u32>,
    /// 仅纳入活跃设备(近 14 天有 last_seen_at)。
    #[serde(default)]
    prefer_active: bool,
    /// 仅纳入 Web 端设备。
    #[serde(default)]
    web_only: bool,
    /// 24h 监控窗口无异常自动扩量;后端持久化标记,扩量由运维侧消费(见 set_canary 文档)。
    #[serde(default)]
    auto_scale_24h: bool,
}

impl SetCanaryRequest {
    /// 折叠为持久化用的 crowd_filters JSON;全部为空时返回 None。
    fn crowd_filters_json(&self) -> Option<serde_json::Value> {
        if self.min_account_age_days.is_none()
            && !self.prefer_active
            && !self.web_only
            && !self.auto_scale_24h
        {
            return None;
        }
        Some(serde_json::json!({
            "minAccountAgeDays": self.min_account_age_days,
            "preferActive": self.prefer_active,
            "webOnly": self.web_only,
            "autoScale24h": self.auto_scale_24h,
        }))
    }
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

    let crowd_filters = req.crowd_filters_json();
    let filter_age = req.min_account_age_days;
    let filter_active = req.prefer_active;
    let filter_web = req.web_only;

    let version_hash = req.version_hash.clone();
    let percent_in = req.percent;
    let force_user_ids = req.force_user_ids.clone();
    let admin_id_for_store = admin_id;
    let filters_for_store = crowd_filters.clone();
    let inserted = state
        .run_store_task("admin.amas.canary.set", move |store| {
            store.set_amas_canary(
                &version_hash,
                percent_in,
                &force_user_ids,
                filters_for_store.as_ref(),
                &admin_id_for_store,
            )
        })
        .await??;

    // 真实影响范围估算(设计稿"影响范围"):过滤后基数 × percent 抽样。
    let audience = state
        .run_store_task("admin.amas.canary.audience", move |store| {
            store.estimate_canary_audience(filter_age, filter_active, filter_web)
        })
        .await??;
    let percent = inserted.percent;
    let affected = (audience.eligible_users as f64 * percent as f64 / 100.0).round() as i64;

    state.amas().mark_canary_active();
    Ok(ok(serde_json::json!({
        "canary": inserted,
        "audience": {
            "totalUsers": audience.total_users,
            "eligibleUsers": audience.eligible_users,
            "affectedUsers": affected,
            // m035 已接线:crowd-filter(平台/账龄/活跃)在 effective_config_for_user 按
            // user_id 做 per-user 人群分流,不满足者回退 stable。auto_scale_24h 仍为运维标记
            // (扩量由运维侧消费),不参与人群判定。
            "enforcement": "enforced"
        },
    })))
}

/// GET /config/canary —— 读当前 active canary 配置(可能为 None)。
async fn get_canary(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let current = state
        .run_store_task("admin.amas.canary.get", |store| {
            store.get_active_amas_canary()
        })
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
            // admin 调试端点不带 question_mode → 单痕迹 legacy
            question_mode: None,
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

/// 唯一的「把 stored patch 应用到 live config 并对用户生效」前置校验通道(#6/#17)。
/// 无条件先跑 tuning_whitelist::validate_patch(白名单 path allowlist + 更严区间，防 DB 篡改/白名单漂移)，
/// 再把 patch 逐路径写入 base config 副本、反序列化、AMASConfig::validate()。
/// approve_one / create_canary 共用此闸，杜绝 canary 旁路白名单这一不对称缺口。
pub(crate) async fn validate_and_build_patched_config(
    state: &AppState,
    base: &crate::amas::config::AMASConfig,
    patch_obj: &serde_json::Map<String, serde_json::Value>,
) -> Result<crate::amas::config::AMASConfig, AppError> {
    use crate::amas::tuning_whitelist::validate_patch;

    // 白名单闸(store 驱动)：path allowlist + 安全区间。
    let patch_for_validate = patch_obj.clone();
    let errs = state
        .run_store_task("admin.amas.patch_validate", move |store| {
            Ok::<_, crate::store::StoreError>(validate_patch(&store, &patch_for_validate))
        })
        .await??;
    if !errs.is_empty() {
        return Err(AppError::bad_request("PATCH_INVALID", &errs.join("；")));
    }

    // 应用 patch 到 base config 副本 → 反序列化 → 结构校验。
    let mut cfg_value =
        serde_json::to_value(base).map_err(|e| AppError::internal(&format!("ser: {e}")))?;
    for (path, value) in patch_obj {
        write_path(&mut cfg_value, path, value.clone());
    }
    let new_cfg: crate::amas::config::AMASConfig =
        serde_json::from_value(cfg_value).map_err(|e| {
            AppError::bad_request("PATCH_INVALID", &format!("应用 patch 后反序列化失败: {e}"))
        })?;
    new_cfg
        .validate()
        .map_err(|e| AppError::bad_request("AMAS_INVALID_CONFIG", &e))?;
    Ok(new_cfg)
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

    // 原子性收口：先落版本行（唯一易失败的 DB 步骤：SQLITE_BUSY/磁盘满），再做不可逆的 live 热重载。
    // 这样"apply 失败 ⟹ live 内存未变"——approve_one 的 Approved→Pending 回退即可让 live 与建议状态保持
    // 一致、可安全重试。旧顺序（先 reload 后插版本）下版本插入失败会留下"live 已是新配置但无任何已落地
    // 版本行"的悬挂态，而回退只动建议状态、无从复位 live，造成 live/审计/建议三方背离。
    let snapshot_json = serde_json::to_string(&cfg)
        .map_err(|e| AppError::internal(&format!("配置序列化失败: {e}")))?;
    let admin_id_owned = admin_id.to_string();
    let snapshot_for_db = snapshot_json;
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

    // 版本已落库后再热重载进 live 内存。reload_config 仅在 validate 失败时返错，而上面已对同一 cfg
    // validate 过（SSP 预计算无 Result），故此步等效不会失败、不会引入"版本在、live 未变"的新窗口。
    state
        .amas()
        .reload_config(cfg.clone())
        .map_err(|e| AppError::bad_request("AMAS_INVALID_CONFIG", &e))?;

    // 写回 TOML 文件（best-effort），保持文件与内存状态一致
    let toml_path = state
        .config()
        .amas_config_file
        .clone()
        .unwrap_or_else(|| "amas_config.toml".to_string());
    if let Err(e) = cfg.write_to_toml(&toml_path) {
        tracing::warn!(path = %toml_path, error = %e, "写回 AMAS 配置文件失败");
    }

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

async fn metrics_kpi(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<DaysQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let days = q.days.unwrap_or(7).clamp(1, 90);
    // 疲劳触发阈值取实际生效配置（与 apply_constraints 同源），store 层无配置访问故在此读出传入。
    let fatigue_threshold = state.amas().get_config().constraints.high_fatigue_threshold;
    let kpi = state
        .run_store_task("admin.amas.metrics_kpi", move |store| {
            store.aggregate_amas_metrics_kpi(days, fatigue_threshold)
        })
        .await??;
    Ok(ok(kpi))
}

async fn metrics_algorithm_distribution(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<DaysQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let days = q.days.unwrap_or(7).clamp(1, 90);
    let dist = state
        .run_store_task("admin.amas.metrics_algorithm_distribution", move |store| {
            store.aggregate_amas_algorithm_distribution(days)
        })
        .await??;
    Ok(ok(dist))
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

/// GET /user-state/summary —— UserState 标量均值(engine_user_states 当前队列,未采样)。
async fn user_state_summary(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let means = state
        .run_store_task("admin.amas.user_state_summary", move |store| {
            store.aggregate_amas_user_state_means()
        })
        .await??;
    Ok(ok(means))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinsQuery {
    bins: Option<u32>,
}

/// GET /cognitive/distribution —— 认知三轴(memory/processing/stability)mean+直方图。
async fn cognitive_distribution(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<BinsQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let bins = q.bins.unwrap_or(20);
    let dist = state
        .run_store_task("admin.amas.cognitive_distribution", move |store| {
            store.aggregate_amas_cognitive_distribution(bins)
        })
        .await??;
    Ok(ok(dist))
}

/// GET /algo-compare —— 按 routing_algo 的 count/share/accuracy/p50-p95-mean 延迟。
/// 注意:基于采样事件、且 accuracy 为「被选中主算法」条件下的命中率,非反事实 A/B。
async fn algo_compare(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<DaysQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let days = q.days.unwrap_or(7).clamp(1, 90);
    let rows = state
        .run_store_task("admin.amas.algo_compare", move |store| {
            store.aggregate_amas_algo_compare(days)
        })
        .await??;
    Ok(ok(rows))
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

// ─────────── 看板对齐设计稿新增子面板 handlers（store 聚合已实现） ───────────

/// GET /metrics/stage-distribution —— 阶段分布（cold/transition/stable）
async fn metrics_stage_distribution(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let dist = state
        .run_store_task("admin.amas.stage_distribution", move |store| {
            store.aggregate_amas_stage_distribution()
        })
        .await??;
    Ok(ok(dist))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EloScatterQuery {
    limit: Option<u32>,
}

/// GET /metrics/elo-scatter —— ELO 散点（rating × games × 7d Δ）
async fn metrics_elo_scatter(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<EloScatterQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(400).clamp(1, 2000);
    let scatter = state
        .run_store_task("admin.amas.elo_scatter", move |store| {
            store.aggregate_amas_elo_scatter(limit)
        })
        .await??;
    Ok(ok(scatter))
}

/// GET /metrics/mdm-heatmap —— MDM 遗忘热图（天 × 难度段）
async fn metrics_mdm_heatmap(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<DaysQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let days = q.days.unwrap_or(7).clamp(1, 90);
    let heatmap = state
        .run_store_task("admin.amas.mdm_heatmap", move |store| {
            store.aggregate_amas_mdm_heatmap(days)
        })
        .await??;
    Ok(ok(heatmap))
}

/// GET /metrics/fatigue-timeseries —— 疲劳时间序列（阈值取实际生效配置，与 metrics_kpi 同源）
async fn metrics_fatigue_timeseries(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<DaysQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let days = q.days.unwrap_or(7).clamp(1, 90);
    let threshold = state.amas().get_config().constraints.high_fatigue_threshold;
    let ts = state
        .run_store_task("admin.amas.fatigue_timeseries", move |store| {
            store.aggregate_amas_fatigue_timeseries(days, threshold)
        })
        .await??;
    Ok(ok(ts))
}

/// GET /metrics/decision-histogram —— 每用户决策数直方图 + P50/P95
async fn metrics_decision_histogram(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<DaysQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let days = q.days.unwrap_or(7).clamp(1, 90);
    let hist = state
        .run_store_task("admin.amas.decision_histogram", move |store| {
            store.aggregate_amas_decision_histogram(days)
        })
        .await??;
    Ok(ok(hist))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnomalyFeedQuery {
    days: Option<u32>,
    limit: Option<u32>,
}

/// GET /anomalies/feed —— 异常逐条 feed（严重度分级 + 影响面），供异常面板详情/忽略
async fn anomalies_feed(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<AnomalyFeedQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let days = q.days.unwrap_or(7).clamp(1, 30);
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let feed = state
        .run_store_task("admin.amas.anomaly_feed", move |store| {
            store.aggregate_amas_anomaly_feed(days, limit)
        })
        .await??;
    Ok(ok(feed))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HoursQuery {
    hours: Option<u32>,
}

/// GET /user-state/transitions —— 状态流转（窗口内阶段穿越）
async fn user_state_transitions(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<HoursQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let hours = q.hours.unwrap_or(24).clamp(1, 720);
    let transitions = state
        .run_store_task("admin.amas.state_transitions", move |store| {
            store.aggregate_amas_state_transitions(hours)
        })
        .await??;
    Ok(ok(transitions))
}

/// GET /user-state/clusters —— 学习风格 K-Means 聚类
async fn user_state_clusters(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let clusters = state
        .run_store_task("admin.amas.learning_clusters", move |store| {
            store.aggregate_amas_learning_clusters()
        })
        .await??;
    Ok(ok(clusters))
}

/// GET /compare/ext —— 双版本扩展指标对比（命中率/P95/疲劳/ensemble/reward/异常率/留存 + sparkline）
async fn compare_versions_ext(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<CompareQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let fatigue_threshold = state.amas().get_config().constraints.high_fatigue_threshold;
    // live config 的 epsilon 仅作版本无快照时的回退；config_epsilon 由 store 层按各版本 snapshot 解析。
    let fallback_epsilon = state
        .amas()
        .get_config()
        .memory_model
        .half_life_base_epsilon;
    let va = q.version_a.clone();
    let vb = q.version_b.clone();
    let (a, b) = state
        .run_store_task(
            "admin.amas.compare_ext",
            move |store| -> Result<_, crate::store::StoreError> {
                let a = store.aggregate_amas_version_slice_ext(
                    &va,
                    fatigue_threshold,
                    fallback_epsilon,
                )?;
                let b = store.aggregate_amas_version_slice_ext(
                    &vb,
                    fatigue_threshold,
                    fallback_epsilon,
                )?;
                Ok((a, b))
            },
        )
        .await??;
    Ok(ok(serde_json::json!({"a": a, "b": b})))
}

// ─────────── BA: 看板 strategy / reward / WordMastery / Canary 聚合 handlers ───────────

/// GET /strategy/summary?days=7 —— Amas 全局策略均值（engine_monitoring_events 采样窗口口径）。
async fn strategy_summary(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<DaysQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let days = q.days.unwrap_or(7).clamp(1, 90);
    let r = state
        .run_store_task("admin.amas.strategy_summary", move |store| {
            store.aggregate_amas_strategy_means(days)
        })
        .await??;
    Ok(ok(r))
}

/// GET /reward/summary?days=7 —— Amas 全局奖励分量均值（采样窗口口径）。
async fn reward_summary(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<DaysQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let days = q.days.unwrap_or(7).clamp(1, 90);
    let r = state
        .run_store_task("admin.amas.reward_summary", move |store| {
            store.aggregate_amas_reward_means(days)
        })
        .await??;
    Ok(ok(r))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LimitQuery {
    limit: Option<u32>,
}

/// GET /word-mastery?limit=7 —— 全局单词记忆强度 top-N（跨所有用户聚合）。
async fn word_mastery(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<LimitQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(7).clamp(1, 200) as usize;
    let rows = state
        .run_store_task("admin.amas.word_mastery", move |store| {
            store.aggregate_word_mastery(limit)
        })
        .await??;
    Ok(ok(rows))
}

/// GET /advisor/canary/:id/compare —— 灰度 canary vs baseline(parent_version) 指标对比。
/// 每侧 accuracy(hit_rate) / retention7d / dailyReview(event_count÷活跃天数) / eventCount。
/// 无 parent_version 或无活跃流量时各字段由前端按 eventCount 渲染 '—'。非随机 A/B，按
/// config_version 切分，存在混淆，口径为 sampled。
async fn compare_canary_baseline(
    _admin: AdminAuthUser,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let fatigue_threshold = state.amas().get_config().constraints.high_fatigue_threshold;
    let fallback_epsilon = state
        .amas()
        .get_config()
        .memory_model
        .half_life_base_epsilon;

    let canary = state
        .run_store_task("admin.amas.canary.compare.get", move |store| {
            store.get_patch_canary(id)
        })
        .await??
        .ok_or_else(|| AppError::not_found("canary 不存在"))?;

    let cv = canary.version_hash.clone();
    let (canary_slice, baseline_hash) = state
        .run_store_task(
            "admin.amas.canary.compare",
            move |store| -> Result<_, crate::store::StoreError> {
                let cs = store.aggregate_amas_version_slice_ext(
                    &cv,
                    fatigue_threshold,
                    fallback_epsilon,
                )?;
                let bh = store
                    .get_amas_config_version(&cv)?
                    .and_then(|d| d.parent_version_hash);
                Ok((cs, bh))
            },
        )
        .await??;

    let baseline_slice = match baseline_hash.clone() {
        Some(h) => Some(
            state
                .run_store_task("admin.amas.canary.compare.base", move |store| {
                    store.aggregate_amas_version_slice_ext(&h, fatigue_threshold, fallback_epsilon)
                })
                .await??,
        ),
        None => None,
    };

    // 活跃天数 = (last-first)/86400，≥1；dailyReview = event_count / 活跃天数。
    let mk = |s: &crate::store::operations::amas_dashboard::VersionSliceExt| {
        let active_days = match (
            s.first_event_at
                .as_deref()
                .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()),
            s.last_event_at
                .as_deref()
                .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()),
        ) {
            (Some(f), Some(l)) => ((l - f).num_seconds() as f64 / 86_400.0).max(1.0),
            _ => 1.0,
        };
        serde_json::json!({
            "accuracy": s.hit_rate,
            "retention7d": s.retention7d,
            "dailyReview": s.event_count as f64 / active_days,
            "eventCount": s.event_count,
        })
    };

    Ok(ok(serde_json::json!({
        "canaryVersion": canary.version_hash,
        "baselineVersion": baseline_hash,
        "percent": canary.percent,
        "status": canary.status,
        "canary": mk(&canary_slice),
        "baseline": baseline_slice.as_ref().map(|b| mk(b)),
    })))
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
    // offset 上界裁剪：避免超大 OFFSET 触发 SQLite 顺序扫描型 DoS（limit 已 clamp）。
    let offset = q.offset.unwrap_or(0).min(100_000);
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

/// CSV 字段转义：①公式注入中和——首字符为电子表格公式触发符时前置单引号；
/// ②含逗号/引号/换行的值用双引号包裹，内部引号翻倍。
fn csv_cell(s: &str) -> String {
    let neutralized = if s
        .chars()
        .next()
        .is_some_and(|c| matches!(c, '=' | '+' | '-' | '@' | '\t' | '\r'))
    {
        format!("'{s}")
    } else {
        s.to_string()
    };
    if neutralized.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", neutralized.replace('"', "\"\""))
    } else {
        neutralized
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

    let mut out = String::from(
        "id,created_at,based_on_version_hash,patch,rationale,cost_usd,status,decided_by\n",
    );
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

    // 白名单 + 结构校验后构造 new config（单一前置闸，防 DB 篡改/白名单漂移，#6/#17）
    let patch_obj = suggestion
        .patch_json
        .as_object()
        .ok_or_else(|| AppError::internal("patch_json 非对象"))?
        .clone();
    let current = state.amas().get_config();
    let new_cfg = validate_and_build_patched_config(state, &current, &patch_obj).await?;

    // 原子抢占 Pending→Approved（CAS）：并发 approve / approve-all 只有一个胜出，杜绝重复 apply(#10)。
    // 校验通过后才抢占，避免坏 patch 误占状态；抢占在 apply 之前，确保胜者唯一。
    let admin_id_owned = admin_id.to_string();
    let note_owned = note.map(|s| s.to_string());
    state
        .run_store_task("admin.amas.approve_claim", move |store| {
            store.cas_amas_suggestion_status(
                id,
                SuggestionStatus::Pending,
                SuggestionStatus::Approved,
                Some(&admin_id_owned),
                note_owned.as_deref(),
            )
        })
        .await?
        .map_err(|e| match e {
            crate::store::StoreError::Conflict { .. } => {
                AppError::bad_request("BAD_STATUS", "该建议已被并发处理")
            }
            crate::store::StoreError::NotFound { .. } => AppError::not_found("建议不存在"),
            other => AppError::internal(&other.to_string()),
        })?;

    // CAS 抢占在 apply 之前确保胜者唯一,但 apply_and_persist_config 若中途失败(SSP 预计算/reload/
    // 版本插入报错),建议已非 pending,重试与 approve-all 都会跳过它——成了"已批准却无任何已落地的持久
    // 配置版本"。故 apply 失败时把抢占回退 Approved→Pending,让重试可重新抢占。CAS 仅当仍为 Approved
    // 才回退,不覆盖期间可能发生的其它合法迁移。
    let applied = apply_and_persist_config(
        state,
        admin_id,
        new_cfg,
        ConfigVersionSource::LlmSuggested,
        Some(format!("approve suggestion#{}", id)),
    )
    .await;
    if applied.is_err() {
        let revert = state
            .run_store_task("admin.amas.approve_revert", move |store| {
                store.cas_amas_suggestion_status(
                    id,
                    SuggestionStatus::Approved,
                    SuggestionStatus::Pending,
                    None,
                    None,
                )
            })
            .await;
        if let Err(e) = revert.map_err(|e| e.to_string()).and_then(|r| r.map_err(|e| e.to_string())) {
            tracing::error!(
                suggestion_id = id,
                error = %e,
                "approve apply 失败后回退 Approved→Pending 未成功，建议可能卡在 approved 态"
            );
        }
    }
    applied
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

    // 定位该建议产出的版本，回滚目标 = 其 parent 版本。两条产出路径：
    //   ① approve（note == "approve suggestion#{id}"）；
    //   ② canary promote（建议 in_canary→approved，promote 版本 note 为 "promote canary#..." 不含
    //      suggestion id）→ 按 suggestion_id 结构化定位 effective canary，其版本的 parent 即回滚目标
    //      （promote 时新 stable 版本 parent = 该 canary 版本 parent = 灰度起点 baseline）。
    let approve_note = format!("approve suggestion#{id}");
    let parent_hash = state
        .run_store_task("admin.amas.rollback_find_version", move |store| {
            let versions = store.list_amas_config_versions(500)?;
            if let Some(parent) = versions
                .into_iter()
                .find(|v| v.note.as_deref() == Some(approve_note.as_str()))
                .and_then(|v| v.parent_version_hash)
            {
                return Ok::<_, crate::store::StoreError>(Some(parent));
            }
            if let Some(vhash) = store.effective_canary_version_for_suggestion(id)? {
                if let Some(v) = store.get_amas_config_version(&vhash)? {
                    return Ok(v.parent_version_hash);
                }
            }
            Ok(None)
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

    // 标记 superseded + 把该建议关联的活跃灰度行一并置 rolled_back，收进同一事务(#19)：
    // 否则 stable 已恢复但 amas_patch_canary 仍 active，engine 继续把 cohort 路由到被回滚的版本。
    let admin_id = admin.admin_id.clone();
    let rolled_canaries = state
        .run_store_task("admin.amas.rollback_mark", move |store| {
            store.with_transaction(|conn| {
                conn.execute(
                    "UPDATE amas_tuning_suggestions
                     SET status = ?1, decided_by = ?2, decided_at = ?3, decision_note = ?4
                     WHERE id = ?5",
                    rusqlite::params![
                        SuggestionStatus::Superseded.as_str(),
                        &admin_id,
                        chrono::Utc::now().to_rfc3339(),
                        "rolled back to parent version",
                        id,
                    ],
                )?;
                crate::store::operations::amas_patch_canary::rollback_active_canaries_for_suggestion_in_conn(
                    conn, id,
                )
            })
        })
        .await??;
    if rolled_canaries > 0 {
        // 灰度路由不再活体下发，引擎需刷新 active canary 缓存标记。
        state.amas().mark_canary_active();
    }

    Ok(ok(serde_json::json!({
        "rolledBack": true,
        "versionHash": parent_hash,
        "rolledBackCanaries": rolled_canaries,
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

    // 预算守卫(#7)：调用前复用 llm_advisor 的日/月成本上限，超限直接 4xx 拦截，
    // 防 admin token 泄露后对付费 LLM 端点无限刷量(billing-DoS)。
    let daily_cap = llm_cfg.daily_cost_cap_usd;
    let current_month = chrono::Utc::now().format("%Y-%m").to_string();
    let month_for_query = current_month.clone();
    let (spend_today, month_cap, spent_month) = state
        .run_store_task("admin.amas.explain.budget", move |store| {
            let (cost, _, _) = store.aggregate_amas_suggestion_spend_today()?;
            let cap = store
                .get_system_settings()
                .map(|s| s.llm_advisor_max_cost_per_month_yuan)
                .unwrap_or(0.0);
            let spent = store.get_llm_cost_this_month(&month_for_query)?;
            Ok::<_, crate::store::StoreError>((cost, cap, spent))
        })
        .await??;
    let month_cap = if month_cap > 0.0 {
        month_cap
    } else {
        llm_cfg.max_cost_per_month_yuan
    };
    if spend_today >= daily_cap {
        return Err(AppError::bad_request(
            "BUDGET_EXCEEDED",
            "已达 LLM 日成本上限，请稍后再试",
        ));
    }
    if spent_month >= month_cap {
        return Err(AppError::bad_request(
            "BUDGET_EXCEEDED",
            "已达 LLM 月成本上限，请下月再试",
        ));
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
    // 计费留痕(#7)：把本次调用成本写入月度台账，使下次预算守卫与 spend 看板可见。
    let cost_yuan = cost * llm_cfg.usd_to_cny_rate;
    if let Err(e) = state
        .run_store_task("admin.amas.explain.ledger", move |store| {
            store.add_llm_cost(&current_month, cost_yuan)
        })
        .await?
    {
        tracing::warn!(error = %e, "explain_param: 月成本台账写入失败");
    }
    // 计费留痕(#7) 之二：日成本守卫按 amas_tuning_suggestions.cost_usd 汇总
    // （aggregate_amas_suggestion_spend_today），故 explain 的花费也必须落一条 billing-only 留痕行，
    // 否则 explain 永不计入 spend_today、日上限形同虚设（只剩月上限+限流兜底）。与 worker 侧
    // record_billing_only 同源；状态 Rejected，不进 pending 待办列表。
    let billing_cost = cost;
    let billing_in = resp.usage.prompt_tokens;
    let billing_out = resp.usage.completion_tokens;
    if let Err(e) = state
        .run_store_task("admin.amas.explain.billing_row", move |store| {
            use crate::store::operations::amas_suggestions::{InsertSuggestion, SuggestionStatus};
            let based = store
                .list_amas_config_versions(1)
                .ok()
                .and_then(|mut v| v.pop())
                .map(|r| r.version_hash)
                .unwrap_or_default();
            store
                .insert_amas_suggestion(&InsertSuggestion {
                    based_on_version_hash: based,
                    patch_json: "{}".into(),
                    rationale: String::new(),
                    evidence_json: "{}".into(),
                    cost_usd: Some(billing_cost),
                    tokens_input: Some(billing_in),
                    tokens_output: Some(billing_out),
                    confidence: None,
                    initial_status: SuggestionStatus::Rejected,
                    decided_by: Some("admin:explain".into()),
                    decision_note: Some("explain billing-only".into()),
                    base_values_json: None,
                })
                .map(|_| ())
        })
        .await?
    {
        tracing::warn!(error = %e, "explain_param: 日成本留痕写入失败");
    }
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
    // 与 add_whitelist 一致的命名空间约束，避免单边删除非 memoryModel.* 条目削弱白名单完整性。
    if !path.starts_with("memoryModel.") {
        return Err(AppError::bad_request(
            "INVALID_PATH",
            "白名单 path 必须以 memoryModel. 开头",
        ));
    }
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
            // 校验下标段形如 `[N]`：strip 失败(如末尾裸 `[`)则路径不可解析，整体跳过，
            // 避免对 `rest[1..rest.len()-1]` 做逆序切片 panic(#33)。
            let Some(idx) = rest
                .strip_prefix('[')
                .and_then(|r| r.strip_suffix(']'))
                .and_then(|inner| inner.parse::<usize>().ok())
            else {
                return;
            };
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

// ─────────────────── diff-impact:逐字段估算影响 ───────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiffImpactRequest {
    /// path → 新值 的扁平 patch(点分式路径),与当前 live config 对比。
    patch: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct MetricImpact {
    metric: &'static str,
    delta_low_pt: f64,
    delta_high_pt: f64,
    direction: &'static str,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FieldImpact {
    path: String,
    from: serde_json::Value,
    to: serde_json::Value,
    rel_change: Option<f64>,
    in_whitelist: bool,
    impacts: Vec<MetricImpact>,
    confidence: &'static str,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DiffImpactResponse {
    fields: Vec<FieldImpact>,
    telemetry_sample_size: i64,
    confidence: &'static str,
    method: &'static str,
}

fn diff_impact_kpi_f64(kpi: &serde_json::Value, keys: &[&str]) -> Option<f64> {
    for k in keys {
        if let Some(v) = kpi.get(k).and_then(|v| v.as_f64()) {
            return Some(v);
        }
    }
    None
}

/// 按点分式路径读 config 值,支持 `mem.w[0]` 下标。
fn diff_impact_read_path(cfg: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
    let mut cur = cfg;
    for part in path.split('.') {
        if let Some(open) = part.find('[') {
            let (key, rest) = part.split_at(open);
            // 下标段须形如 `[N]`；末尾裸 `[` 等非法形态返回 None 而非逆序切片 panic(#33)。
            let idx: usize = rest
                .strip_prefix('[')
                .and_then(|r| r.strip_suffix(']'))?
                .parse()
                .ok()?;
            cur = cur.get(key)?.get(idx)?;
        } else {
            cur = cur.get(part)?;
        }
    }
    Some(cur.clone())
}

/// POST /config/diff-impact —— 对一组 config 变更逐字段返回**计算的**估算影响(替代设计稿
/// "命中率 +2~4%" 等静态敏感度文案)。
///
/// 模型:仅白名单(记忆模型)参数可建模;以有符号相对变化为驱动,经保守系数映射到百分点区间;
/// 非白名单字段返回 flat。区间宽度随样本量收窄。dry-run 方向性估算(非在线 counterfactual),
/// 故标 confidence。复用既有 read_path / tuning_whitelist::find。
async fn diff_impact(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<DiffImpactRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let current = state.amas().get_config();
    let current_value =
        serde_json::to_value(&current).map_err(|e| AppError::internal(&format!("ser: {e}")))?;

    let fatigue_threshold = current.constraints.high_fatigue_threshold;
    let kpi: serde_json::Value = state
        .run_store_task("admin.amas.diff_impact_kpi", move |store| {
            store
                .aggregate_amas_metrics_kpi(7, fatigue_threshold)
                .map(|k| serde_json::to_value(&k).unwrap_or(serde_json::Value::Null))
        })
        .await??;
    let sample = diff_impact_kpi_f64(
        &kpi,
        &[
            "decisionTotal",
            "decision_total",
            "sampleSize",
            "totalEvents",
        ],
    )
    .map(|v| v as i64)
    .unwrap_or(0);

    let width = if sample >= 200 {
        0.6
    } else if sample >= 50 {
        1.0
    } else {
        1.6
    };

    let mut fields = Vec::with_capacity(req.patch.len());
    let mut in_wl_count = 0usize;
    for (path, to_val) in &req.patch {
        let from_val =
            diff_impact_read_path(&current_value, path).unwrap_or(serde_json::Value::Null);
        let rel_change = match (from_val.as_f64(), to_val.as_f64()) {
            (Some(f), Some(t)) if f.abs() > 1e-9 => Some((t - f) / f.abs()),
            _ => None,
        };
        let in_whitelist = crate::amas::tuning_whitelist::find(path).is_some();
        if in_whitelist {
            in_wl_count += 1;
        }

        let drive = rel_change.unwrap_or(0.0).clamp(-1.0, 1.0);
        let modeled = in_whitelist && rel_change.is_some() && drive.abs() > 1e-6;
        let mk = |center: f64, metric: &'static str| {
            let mid = (drive * center).clamp(-8.0, 8.0);
            let half = (mid.abs() * 0.4 * width).max(0.3);
            let (lo, hi) = (mid - half, mid + half);
            let direction = if mid > 0.05 {
                "up"
            } else if mid < -0.05 {
                "down"
            } else {
                "flat"
            };
            MetricImpact {
                metric,
                delta_low_pt: (lo * 10.0).round() / 10.0,
                delta_high_pt: (hi * 10.0).round() / 10.0,
                direction,
            }
        };
        let flat = |metric: &'static str| MetricImpact {
            metric,
            delta_low_pt: 0.0,
            delta_high_pt: 0.0,
            direction: "flat",
        };
        let impacts = if modeled {
            vec![
                mk(2.0, "accuracy"),
                mk(-0.8, "fatigue"),
                mk(4.0, "d7Retention"),
            ]
        } else {
            vec![flat("accuracy"), flat("fatigue"), flat("d7Retention")]
        };

        let confidence = if !in_whitelist {
            "low"
        } else if sample >= 200 {
            "high"
        } else if sample >= 50 {
            "medium"
        } else {
            "low"
        };

        fields.push(FieldImpact {
            path: path.clone(),
            from: from_val,
            to: to_val.clone(),
            rel_change,
            in_whitelist,
            impacts,
            confidence,
        });
    }

    let all_in_wl = !fields.is_empty() && in_wl_count == fields.len();
    let confidence = if all_in_wl && sample >= 200 {
        "high"
    } else if (all_in_wl || in_wl_count > 0) && sample >= 50 {
        "medium"
    } else {
        "low"
    };

    Ok(ok(DiffImpactResponse {
        fields,
        telemetry_sample_size: sample,
        confidence,
        method: "telemetry-baseline + 白名单参数相对变化启发式(逐字段区间,无在线 counterfactual)",
    }))
}

// ─────────────────── 沙箱试运行：单条 suggestion 影响预估 ───────────────────

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SandboxChange {
    path: String,
    from: serde_json::Value,
    to: serde_json::Value,
    rel_change: Option<f64>,
    in_whitelist: bool,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SandboxMetricImpact {
    /// baseline 当前值（百分点）。telemetry 无留存维度时 d7Retention 恒 None（诚实降级）。
    baseline: Option<f64>,
    predicted: Option<f64>,
    delta_pt: f64,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SandboxSuggestionResponse {
    suggestion_id: i64,
    based_on_version_hash: String,
    config_valid: bool,
    config_error: Option<String>,
    whitelist_ok: bool,
    whitelist_errors: Vec<String>,
    changes: Vec<SandboxChange>,
    accuracy: SandboxMetricImpact,
    fatigue: SandboxMetricImpact,
    d7_retention: SandboxMetricImpact,
    confidence: &'static str,
    method: &'static str,
    telemetry_sample_size: i64,
}

/// POST /advisor/suggestions/:id/sandbox —— 对一条 suggestion 做沙箱试运行：把 patch 应用到 live
/// config 副本（不落库、不热重载），校验 config 合法性 + 白名单，并以 telemetry KPI 为 baseline
/// 用白名单参数相对变化启发式预估 accuracy / fatigue / d7Retention 的百分点偏移。
///
/// 诚实降级：telemetry 仅有命中率/疲劳触发率维度，无 d7 留存观测，故 d7Retention.baseline 恒 None，
/// 只返回方向性预估 delta。method 字段如实标注非在线 counterfactual。
async fn sandbox_suggestion(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let suggestion = state
        .run_store_task("admin.amas.sandbox_lookup", move |store| {
            store.get_amas_suggestion(id)
        })
        .await??
        .ok_or_else(|| AppError::not_found("建议不存在"))?;

    let patch_obj = suggestion
        .patch_json
        .as_object()
        .ok_or_else(|| AppError::internal("patch_json 非对象"))?
        .clone();

    // 白名单校验（store 驱动，const fallback）
    let patch_for_validate = patch_obj.clone();
    let whitelist_errors = state
        .run_store_task("admin.amas.sandbox_whitelist", move |store| {
            Ok::<_, crate::store::StoreError>(crate::amas::tuning_whitelist::validate_patch(
                &store,
                &patch_for_validate,
            ))
        })
        .await??;
    let whitelist_ok = whitelist_errors.is_empty();

    // 应用 patch 到 live config 副本并校验（不热重载、不落库）
    let current = state.amas().get_config();
    let current_value =
        serde_json::to_value(&current).map_err(|e| AppError::internal(&format!("ser: {e}")))?;
    let mut cfg_value = current_value.clone();
    for (path, value) in &patch_obj {
        write_path(&mut cfg_value, path, value.clone());
    }
    let (config_valid, config_error) =
        match serde_json::from_value::<crate::amas::config::AMASConfig>(cfg_value) {
            Ok(cfg) => match cfg.validate() {
                Ok(()) => (true, None),
                Err(e) => (false, Some(e)),
            },
            Err(e) => (false, Some(format!("应用 patch 后反序列化失败: {e}"))),
        };

    // telemetry baseline（7 日 KPI）
    let fatigue_threshold = current.constraints.high_fatigue_threshold;
    let kpi = state
        .run_store_task("admin.amas.sandbox_kpi", move |store| {
            store.aggregate_amas_metrics_kpi(7, fatigue_threshold)
        })
        .await??;
    let sample = kpi.decision_total as i64;
    let acc_baseline = kpi.hit_rate * 100.0;
    let fatigue_baseline = kpi.fatigue_trigger_rate * 100.0;

    // 区间宽度→单值取中点；样本越多越收窄（与 diff_impact 同源启发式）。
    let scale = if sample >= 200 {
        1.0
    } else if sample >= 50 {
        0.8
    } else {
        0.5
    };

    // 各 path 相对变化求和作为驱动信号（仅白名单参数计入建模）。
    let mut drive = 0.0;
    let mut changes = Vec::with_capacity(patch_obj.len());
    let mut modeled_any = false;
    let mut all_in_wl = true;
    for (path, to_val) in &patch_obj {
        let from_val =
            diff_impact_read_path(&current_value, path).unwrap_or(serde_json::Value::Null);
        let rel_change = match (from_val.as_f64(), to_val.as_f64()) {
            (Some(f), Some(t)) if f.abs() > 1e-9 => Some((t - f) / f.abs()),
            _ => None,
        };
        let in_whitelist = crate::amas::tuning_whitelist::find(path).is_some();
        if !in_whitelist {
            all_in_wl = false;
        }
        if in_whitelist {
            if let Some(rc) = rel_change {
                if rc.abs() > 1e-6 {
                    drive += rc.clamp(-1.0, 1.0);
                    modeled_any = true;
                }
            }
        }
        changes.push(SandboxChange {
            path: path.clone(),
            from: from_val,
            to: to_val.clone(),
            rel_change,
            in_whitelist,
        });
    }
    let drive = drive.clamp(-1.0, 1.0);

    // 中点百分点偏移（accuracy +, fatigue -, d7 +）；不可建模时全 0。
    let mk_delta = |center: f64| -> f64 {
        if !modeled_any {
            return 0.0;
        }
        ((drive * center * scale).clamp(-8.0, 8.0) * 10.0).round() / 10.0
    };
    let acc_delta = mk_delta(2.0);
    let fatigue_delta = mk_delta(-0.8);
    let d7_delta = mk_delta(4.0);

    let accuracy = SandboxMetricImpact {
        baseline: Some((acc_baseline * 10.0).round() / 10.0),
        predicted: Some(((acc_baseline + acc_delta) * 10.0).round() / 10.0),
        delta_pt: acc_delta,
    };
    let fatigue = SandboxMetricImpact {
        baseline: Some((fatigue_baseline * 10.0).round() / 10.0),
        predicted: Some(((fatigue_baseline + fatigue_delta) * 10.0).round() / 10.0),
        delta_pt: fatigue_delta,
    };
    // 诚实降级：无 d7 留存观测，baseline/predicted 恒 None。
    let d7_retention = SandboxMetricImpact {
        baseline: None,
        predicted: None,
        delta_pt: d7_delta,
    };

    let confidence = if all_in_wl && modeled_any && sample >= 200 {
        "high"
    } else if modeled_any && sample >= 50 {
        "medium"
    } else {
        "low"
    };

    Ok(ok(SandboxSuggestionResponse {
        suggestion_id: id,
        based_on_version_hash: suggestion.based_on_version_hash,
        config_valid,
        config_error,
        whitelist_ok,
        whitelist_errors,
        changes,
        accuracy,
        fatigue,
        d7_retention,
        confidence,
        method: "telemetry-baseline + 白名单参数相对变化启发式（dry-run 方向性预估，非在线 counterfactual）",
        telemetry_sample_size: sample,
    }))
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
        return Err(AppError::bad_request(
            "BAD_STATUS",
            "仅 pending 建议可进灰度",
        ));
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

    // 落 canary version snapshot:把 patch 应用到 stable 后入版本表(与 approve 同构造)。
    // 经唯一前置闸 validate_and_build_patched_config：白名单(path allowlist + 更严区间) + 结构校验，
    // 关闭 canary 旁路 validate_patch 的不对称缺口(#6/#17)。
    let patch_obj = suggestion
        .patch_json
        .as_object()
        .ok_or_else(|| AppError::internal("patch_json 非对象"))?
        .clone();
    let current = state.amas().get_config();
    let new_cfg = validate_and_build_patched_config(&state, &current, &patch_obj).await?;
    let snapshot_json = serde_json::to_string(&new_cfg)
        .map_err(|e| AppError::internal(&format!("配置序列化失败: {e}")))?;

    let parent_hash = suggestion.based_on_version_hash.clone();
    let percent = req.percent;
    let admin_id = admin.admin_id.clone();
    let canary_note = format!("canary suggestion#{sid}");

    // 单事务：拒绝重复灰度 + 落版本 + 建 active canary 行 + 迁出 suggestion(Pending→InCanary)(#9)。
    // 消除「建议仍 Pending 可被 approve 全量应用」与「同一建议反复占满灰度配额」两条缺口。
    let inserted = state
        .run_store_task("admin.amas.canary.create", move |store| {
            store.create_canary_and_claim_suggestion(
                sid,
                &snapshot_json,
                &admin_id,
                Some(&canary_note),
                Some(&parent_hash),
                percent,
                &baseline_json,
            )
        })
        .await?
        .map_err(|e| match e {
            crate::store::StoreError::Validation(msg) => {
                AppError::bad_request("CANARY_QUOTA_FULL", &msg)
            }
            crate::store::StoreError::Conflict { .. } => {
                AppError::bad_request("BAD_STATUS", "该建议已被并发处理（已批准或已进灰度）")
            }
            other => AppError::internal(&other.to_string()),
        })?;

    state.amas().mark_canary_active();
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

/// POST /advisor/canary/:id/scale —— 扩量到目标 percent,保持 cohort_lo 不变扩展 hi(store 校验不与其它 active canary 重叠)。
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
    // 读 cohort_lo + 排重叠校验 + UPDATE 收进单事务（store.scale_active_patch_canary），
    // 消除「读 cohort_lo」与「update」拆两次 run_store_task 的 TOCTOU 窗口。
    let updated = state
        .run_store_task("admin.amas.canary.scale", move |store| {
            store.scale_active_patch_canary(id, percent)
        })
        .await?
        .map_err(|e| AppError::bad_request("SCALE_FAILED", &e.to_string()))?;
    state.amas().mark_canary_active();
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
            store.rollback_patch_canary_and_release_suggestion(id)
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
        return Err(AppError::bad_request(
            "BAD_STATUS",
            "仅 active canary 可提升",
        ));
    }
    if canary.percent != 100 {
        return Err(AppError::bad_request(
            "NOT_FULL_ROLLOUT",
            "仅 100% 灰度可提升 stable",
        ));
    }

    let vhash = canary.version_hash.clone();

    // 复核 baseline 退化守卫(#15)：聚合 live 切片 + 解析 baseline，复用与 canary_monitor 同一
    // should_rollback 纯函数。若已判定应回滚则拒绝提升，避免抢在 monitor 周期前把退化 patch 推全量。
    let vhash_for_slice = vhash.clone();
    let baseline_json = canary.baseline_metrics_json.clone();
    let (live, baseline, reward_drop_th, anomaly_rise_th) = state
        .run_store_task("admin.amas.canary.promote_guard", move |store| {
            let live = store.aggregate_amas_version_slice(&vhash_for_slice)?;
            let baseline: crate::amas::monitoring::CanaryBaseline =
                serde_json::from_str(&baseline_json).unwrap_or_default();
            let (rd, ar) = store
                .get_system_settings()
                .map(|s| {
                    (
                        s.canary_reward_drop_threshold,
                        s.canary_anomaly_rise_threshold,
                    )
                })
                .unwrap_or((0.05, 0.05));
            Ok::<_, crate::store::StoreError>((live, baseline, rd, ar))
        })
        .await??;
    if crate::amas::monitoring::should_rollback(&baseline, &live, reward_drop_th, anomaly_rise_th) {
        return Err(AppError::bad_request(
            "CANARY_DEGRADED",
            "该灰度已触发退化守卫（reward 下跌或异常率飙升），拒绝提升为 stable",
        ));
    }

    // 取 canary version snapshot 并结构校验。
    let vhash_lookup = vhash.clone();
    let detail = state
        .run_store_task("admin.amas.canary.promote_version", move |store| {
            store.get_amas_config_version(&vhash_lookup)
        })
        .await??
        .ok_or_else(|| AppError::internal("canary version 不存在"))?;
    let cfg: crate::amas::config::AMASConfig = serde_json::from_value(detail.snapshot_json)
        .map_err(|e| AppError::internal(&format!("快照反序列化失败: {e}")))?;
    cfg.validate()
        .map_err(|e| AppError::bad_request("AMAS_INVALID_CONFIG", &e))?;
    let snapshot_json = serde_json::to_string(&cfg)
        .map_err(|e| AppError::internal(&format!("配置序列化失败: {e}")))?;

    // 单事务原子提升(#8/#16/#41)：CAS canary active→effective + 落 stable 版本行。
    // 期间被 monitor 自动回滚（active→rolled_back）→ CAS affected==0 → Conflict，放弃提升，
    // 不污染 stable，也不把 rolled_back 覆盖回 effective。
    let admin_id = admin.admin_id.clone();
    let parent = detail.parent_version_hash.clone();
    let promote_note = format!("promote canary#{id} → stable");
    let (version_id, version_hash) = state
        .run_store_task("admin.amas.canary.promote_tx", move |store| {
            store.promote_patch_canary_tx(
                id,
                &snapshot_json,
                &admin_id,
                ConfigVersionSource::Manual,
                Some(&promote_note),
                parent.as_deref(),
            )
        })
        .await?
        .map_err(|e| match e {
            crate::store::StoreError::Conflict { .. } => AppError::bad_request(
                "CANARY_ROLLED_BACK",
                "该灰度已被自动/手动回滚，无法提升",
            ),
            crate::store::StoreError::NotFound { .. } => AppError::not_found("canary 不存在"),
            other => AppError::internal(&other.to_string()),
        })?;

    // DB 状态已原子落定后，再把配置热重载进 live 内存 + 写回 TOML（内存态无法纳入 DB 事务）。
    state
        .amas()
        .reload_config(cfg.clone())
        .map_err(|e| AppError::internal(&format!("热重载配置失败: {e}")))?;
    let toml_path = state
        .config()
        .amas_config_file
        .clone()
        .unwrap_or_else(|| "amas_config.toml".to_string());
    if let Err(e) = cfg.write_to_toml(&toml_path) {
        tracing::warn!(path = %toml_path, error = %e, "promote_canary: 写回 AMAS 配置文件失败");
    }
    tracing::info!(
        admin_id = %admin.admin_id,
        action = "promote_canary",
        canary_id = id,
        version_id,
        version_hash = %version_hash,
        "管理员提升 canary 为 stable"
    );
    state.amas().mark_canary_active();

    Ok(ok(
        serde_json::json!({ "promoted": true, "versionHash": version_hash }),
    ))
}

// ─────────────────── T1.3: 真实留存 A/B 实验 ───────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterExperimentRequest {
    /// 缺省自动生成 uuid。
    experiment_id: Option<String>,
    suggestion_id: Option<i64>,
    canary_version_hash: String,
    baseline_version_hash: String,
    canary_cohort_lo: u32,
    canary_cohort_hi: u32,
    /// 缺省 day7_retention。
    primary_metric: Option<String>,
    min_sample: u64,
    /// 缺省 0.05 / 0.8。
    alpha: Option<f64>,
    power: Option<f64>,
    mde: f64,
    offline_delta: Option<f64>,
    notes: Option<String>,
}

/// POST /experiments —— 注册一个真实留存 A/B 实验（预注册 primary/min_sample/alpha/MDE）。
/// 要求当前无 running 实验（单 active 实验模型）。注册后引擎热路径即开始按桶冻结 arm 入组。
async fn register_experiment(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<RegisterExperimentRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    use crate::store::operations::amas_experiment::NewExperiment;
    let exp = NewExperiment {
        experiment_id: req
            .experiment_id
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        suggestion_id: req.suggestion_id,
        canary_version_hash: req.canary_version_hash,
        baseline_version_hash: req.baseline_version_hash,
        canary_cohort_lo: req.canary_cohort_lo,
        canary_cohort_hi: req.canary_cohort_hi,
        primary_metric: req
            .primary_metric
            .unwrap_or_else(|| "day7_retention".to_string()),
        min_sample: req.min_sample,
        alpha: req.alpha.unwrap_or(0.05),
        power: req.power.unwrap_or(0.8),
        mde: req.mde,
        offline_delta: req.offline_delta,
        notes: req.notes,
    };
    let now = chrono::Utc::now().to_rfc3339();
    let row = state
        .run_store_task("admin.amas.experiment.register", move |store| {
            store.register_experiment(&exp, &now)
        })
        .await?
        .map_err(|e| match e {
            crate::store::StoreError::Validation(msg) => {
                AppError::bad_request("EXPERIMENT_INVALID", &msg)
            }
            other => AppError::internal(&other.to_string()),
        })?;
    // 引擎热路径即时生效实验入组门。
    state.amas().reload_active_experiment();
    Ok(ok(serde_json::to_value(&row).unwrap()))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListExperimentsQuery {
    status: Option<String>,
}

/// GET /experiments —— 列出实验（可按 status 过滤）。
async fn list_experiments(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Query(q): Query<ListExperimentsQuery>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let rows = state
        .run_store_task("admin.amas.experiment.list", move |store| {
            store.list_experiments(q.status.as_deref())
        })
        .await??;
    Ok(ok(serde_json::to_value(&rows).unwrap()))
}

/// GET /experiments/:id/metrics —— 两臂北极星指标 + CI + 采纳门判定。
async fn experiment_metrics(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let (exp, raw) = state
        .run_store_task("admin.amas.experiment.metrics", move |store| {
            let exp = store.get_experiment(&id)?;
            match exp {
                Some(exp) => {
                    let raw = store.experiment_raw_metrics(&exp.experiment_id)?;
                    Ok::<_, crate::store::StoreError>(Some((exp, raw)))
                }
                None => Ok(None),
            }
        })
        .await??
        .ok_or_else(|| AppError::not_found("实验不存在"))?;
    let verdict = crate::amas::experiment::evaluate::evaluate(&exp, &raw);
    Ok(ok(serde_json::json!({
        "experiment": exp,
        "raw": raw,
        "verdict": verdict,
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConcludeExperimentRequest {
    /// true=采纳(concluded_adopt)，false=否决(concluded_reject)。
    adopt: bool,
}

/// POST /experiments/:id/conclude —— 结束实验并记录采纳/否决。停止入组（reload）。
/// 注意：采纳仅记录结论，配置全量上线仍走既有 promote_canary 流程。
async fn conclude_experiment(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    JsonBody(req): JsonBody<ConcludeExperimentRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let status = if req.adopt {
        "concluded_adopt"
    } else {
        "concluded_reject"
    };
    let now = chrono::Utc::now().to_rfc3339();
    let id2 = id.clone();
    state
        .run_store_task("admin.amas.experiment.conclude", move |store| {
            store.conclude_experiment(&id2, status, &now)
        })
        .await?
        .map_err(|e| match e {
            crate::store::StoreError::NotFound { .. } => {
                AppError::not_found("running 实验不存在")
            }
            crate::store::StoreError::Validation(msg) => {
                AppError::bad_request("EXPERIMENT_INVALID", &msg)
            }
            other => AppError::internal(&other.to_string()),
        })?;
    state.amas().reload_active_experiment();
    Ok(ok(serde_json::json!({ "experimentId": id, "status": status })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanExperimentRequest {
    /// "proportion"（默认，如 day7_retention）| "mean"（如 reviews_per_day）。
    kind: Option<String>,
    alpha: Option<f64>,
    power: Option<f64>,
    /// 两臂合计日进桶用户数（反推运行天数）。
    daily_signups: Option<f64>,
    // proportion 入参
    p0: Option<f64>,
    mde_rel: Option<f64>,
    // mean 入参
    sigma: Option<f64>,
    delta: Option<f64>,
}

/// POST /experiments/plan —— 按 primary 基线 + MDE/alpha/power 反推每桶最小样本量与推荐 percent。
/// 决策4：percent 由后端反推（5% 对 Day-30 低频留存大概率不足）。
async fn plan_experiment(
    _admin: AdminAuthUser,
    JsonBody(req): JsonBody<PlanExperimentRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let alpha = req.alpha.unwrap_or(0.05);
    let power = req.power.unwrap_or(0.8);
    let daily = req.daily_signups.unwrap_or(0.0);
    if !(alpha > 0.0 && alpha < 1.0) || !(power > 0.0 && power < 1.0) {
        return Err(AppError::bad_request(
            "INVALID_PARAM",
            "alpha/power 须 ∈ (0,1)",
        ));
    }
    let kind = req.kind.as_deref().unwrap_or("proportion");
    let plan = match kind {
        "mean" => {
            let sigma = req.sigma.ok_or_else(|| {
                AppError::bad_request("MISSING_PARAM", "mean 规划需 sigma")
            })?;
            let delta = req.delta.ok_or_else(|| {
                AppError::bad_request("MISSING_PARAM", "mean 规划需 delta")
            })?;
            if sigma <= 0.0 || delta <= 0.0 {
                return Err(AppError::bad_request(
                    "INVALID_PARAM",
                    "sigma 与 delta 须 > 0",
                ));
            }
            crate::amas::experiment::plan_mean(sigma, delta, alpha, power, daily)
        }
        _ => {
            let p0 = req
                .p0
                .ok_or_else(|| AppError::bad_request("MISSING_PARAM", "proportion 规划需 p0"))?;
            let mde_rel = req.mde_rel.ok_or_else(|| {
                AppError::bad_request("MISSING_PARAM", "proportion 规划需 mdeRel")
            })?;
            if !(p0 > 0.0 && p0 < 1.0) || mde_rel <= 0.0 {
                return Err(AppError::bad_request(
                    "INVALID_PARAM",
                    "p0 须 ∈ (0,1) 且 mdeRel > 0",
                ));
            }
            crate::amas::experiment::plan_proportion(p0, mde_rel, alpha, power, daily)
        }
    };
    Ok(ok(serde_json::to_value(&plan).unwrap()))
}
