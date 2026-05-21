//! LLM 调参顾问 worker：每 20 分钟跑一次，综合近期指标 + 当前配置，
//! 通过 DeepSeek（OpenAI 兼容协议）产生 patch 建议落 `amas_tuning_suggestions` 表。
//!
//! 安全护栏：
//! 1. patch 字段必须在 `tuning_whitelist` 内
//! 2. 值必须在白名单安全区间
//! 3. 每日成本 ≤ `llm.daily_cost_cap_usd`
//! 4. mock 模式下也会写一条 pending 建议，便于端到端联调

use crate::amas::engine::AMASEngine;
use crate::amas::tuning_whitelist::{validate_patch, TIER_A_WHITELIST};
use crate::config::LLMConfig;
use crate::services::llm_provider::{ChatMessage, ChatRequest, LlmProvider};
use crate::store::operations::amas_suggestions::{InsertSuggestion, SuggestionStatus};
use crate::store::operations::amas_versions::{compute_config_hash, ConfigVersionSource};
use crate::store::Store;

const SYSTEM_PROMPT: &str = r#"你是 AMAS（自适应记忆与算法系统）的调参顾问。
你只能修改下方"白名单参数"中的字段，且必须给出基于证据的理由。
输出严格 JSON：
{
  "patch": { "memoryModel.<path>": <number>, ... },
  "rationale": "中文短理由，引用 evidence 字段",
  "confidence": 0.0-1.0
}

白名单字段（含安全区间，越界会被拒绝）：
"#;

fn build_system_prompt() -> String {
    let mut s = String::from(SYSTEM_PROMPT);
    for entry in TIER_A_WHITELIST {
        s.push_str(&format!(
            "- {} ∈ [{}, {}]\n",
            entry.path, entry.min_safe, entry.max_safe
        ));
    }
    s.push_str("\n要求：patch 至多包含 3 个参数；优先调整与 evidence 关联最强的字段；不确定时输出 {\"patch\":{}}。");
    s
}

/// Worker 入口。失败时仅记录日志，不向调度器抛出（保持 timer 不被 disable）。
pub async fn run(
    store: &Store,
    llm_cfg: Option<&LLMConfig>,
    engine: &AMASEngine,
    app_state: Option<&crate::state::AppState>,
) {
    tracing::debug!("llm_advisor: start");

    let Some(llm_cfg) = llm_cfg else {
        tracing::debug!("llm_advisor: llm config missing, skip");
        return;
    };

    if !llm_cfg.enabled {
        tracing::debug!("llm_advisor: llm disabled, skip");
        return;
    }

    // 日成本守卫
    match store.aggregate_amas_suggestion_spend_today() {
        Ok((cost, _, _)) if cost >= llm_cfg.daily_cost_cap_usd => {
            tracing::warn!(
                cost_usd = cost,
                cap = llm_cfg.daily_cost_cap_usd,
                "llm_advisor: 已达日成本上限，跳过本次"
            );
            return;
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(error = %e, "llm_advisor: 读取每日成本失败，仍尝试运行");
        }
    }

    let current_month = chrono::Utc::now().format("%Y-%m").to_string();
    let month_cap = store.get_system_settings().ok().map(|s| s.llm_advisor_max_cost_per_month_yuan).unwrap_or(llm_cfg.max_cost_per_month_yuan);
    match store.get_llm_cost_this_month(&current_month) {
        Ok(spent) if spent >= month_cap => {
            tracing::warn!(spent_yuan = spent, cap_yuan = month_cap, month = %current_month, "llm_advisor: 已达月成本上限，跳过本次");
            let resume = next_month_str(&current_month);
            if let Some(state) = app_state {
                state.broadcast_to_all_sse(crate::state::SseEvent::LlmBudgetExceeded { spent_yuan: spent, cap_yuan: month_cap, resume_month: resume });
            }
            return;
        }
        Ok(_) => {}
        Err(e) => { tracing::warn!(error = %e, "llm_advisor: 读取月成本失败，仍尝试运行"); }
    }

    let evidence = match collect_evidence(store, engine) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(error = %e, "llm_advisor: 收集 evidence 失败，跳过");
            return;
        }
    };

    let provider = LlmProvider::new(llm_cfg);
    let req = ChatRequest {
        messages: vec![
            ChatMessage { role: "system".into(), content: build_system_prompt() },
            ChatMessage {
                role: "user".into(),
                content: format!(
                    "evidence (JSON)：\n{}\n\n请基于以上 evidence 给出 patch。",
                    serde_json::to_string_pretty(&evidence).unwrap_or_default()
                ),
            },
        ],
        json_object: true,
        temperature: 0.2,
    };

    let resp = match provider.chat(req).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "llm_advisor: LLM 调用失败");
            return;
        }
    };

    let cost = provider.estimate_cost_usd(&resp.usage);
    let cost_yuan = cost * llm_cfg.usd_to_cny_rate;
    if let Err(e) = store.add_llm_cost(&current_month, cost_yuan) { tracing::warn!(error = %e, "llm_advisor: 月成本累计写入失败"); }

    let parsed: serde_json::Value = match serde_json::from_str(&resp.content) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, raw = %resp.content, "llm_advisor: 响应非 JSON");
            return;
        }
    };

    let patch_obj = match parsed.get("patch").and_then(|p| p.as_object()).cloned() {
        Some(o) => o,
        None => {
            tracing::warn!(raw = %resp.content, "llm_advisor: 缺少 patch 字段");
            return;
        }
    };
    let rationale = parsed
        .get("rationale")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let confidence = parsed.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.5);

    if patch_obj.is_empty() {
        tracing::info!(rationale = %rationale, "llm_advisor: LLM 不建议改动，跳过");
        return;
    }

    let validation_errors = validate_patch(&patch_obj);
    if !validation_errors.is_empty() {
        tracing::warn!(
            errors = ?validation_errors,
            "llm_advisor: patch 校验失败，落 rejected 一条留痕"
        );
        let _ = store.insert_amas_suggestion(&InsertSuggestion {
            based_on_version_hash: current_version_hash(engine),
            patch_json: serde_json::to_string(&patch_obj).unwrap_or_default(),
            rationale: rationale.clone(),
            evidence_json: serde_json::to_string(&evidence).unwrap_or_default(),
            cost_usd: Some(cost),
            tokens_input: Some(resp.usage.prompt_tokens),
            tokens_output: Some(resp.usage.completion_tokens),
            confidence: Some(confidence),
            initial_status: SuggestionStatus::Rejected,
            decided_by: Some("worker:llm_advisor".into()),
            decision_note: Some(validation_errors.join("；")),
        });
        return;
    }

    // 决策：是否走灰度自动应用
    let auto_decision = should_auto_apply(store, &patch_obj, confidence);
    let (initial_status, decided_by, decision_note) = match auto_decision {
        AutoDecision::Apply => (
            SuggestionStatus::AutoApplied,
            Some("worker:llm_advisor".to_string()),
            Some("auto apply: 满足开关 + 单参 + 范围 + 置信度 + 日额度".to_string()),
        ),
        AutoDecision::Skip(reason) => {
            tracing::debug!(reason = %reason, "llm_advisor: 不满足自动应用条件，落 pending");
            (SuggestionStatus::Pending, None, None)
        }
    };

    let id = match store.insert_amas_suggestion(&InsertSuggestion {
        based_on_version_hash: current_version_hash(engine),
        patch_json: serde_json::to_string(&patch_obj).unwrap_or_default(),
        rationale: rationale.clone(),
        evidence_json: serde_json::to_string(&evidence).unwrap_or_default(),
        cost_usd: Some(cost),
        tokens_input: Some(resp.usage.prompt_tokens),
        tokens_output: Some(resp.usage.completion_tokens),
        confidence: Some(confidence),
        initial_status,
        decided_by: decided_by.clone(),
        decision_note: decision_note.clone(),
    }) {
        Ok(id) => id,
        Err(e) => {
            tracing::warn!(error = %e, "llm_advisor: 落库失败");
            return;
        }
    };
    tracing::info!(
        suggestion_id = id,
        status = ?initial_status,
        "llm_advisor: 新建建议"
    );

    // 自动应用：reload + write toml + 写版本
    if matches!(initial_status, SuggestionStatus::AutoApplied) {
        if let Err(e) = auto_apply_patch(store, engine, &patch_obj, id).await {
            tracing::error!(error = %e, suggestion_id = id, "llm_advisor: 自动应用失败");
        }
    }
    tracing::debug!("llm_advisor: done");
}

enum AutoDecision {
    Apply,
    Skip(String),
}

fn should_auto_apply(
    store: &Store,
    patch_obj: &serde_json::Map<String, serde_json::Value>,
    confidence: f64,
) -> AutoDecision {
    let settings = match store.get_system_settings() {
        Ok(s) => s,
        Err(e) => return AutoDecision::Skip(format!("settings 加载失败: {e}")),
    };
    if !settings.amas_auto_apply_enabled {
        return AutoDecision::Skip("开关未开".into());
    }
    if patch_obj.len() != 1 {
        return AutoDecision::Skip(format!("patch 含 {} 项（仅允许 1 项）", patch_obj.len()));
    }
    if confidence < settings.amas_auto_apply_min_confidence {
        return AutoDecision::Skip(format!(
            "置信度 {confidence:.3} < {:.3}",
            settings.amas_auto_apply_min_confidence
        ));
    }
    // 当日已自动应用数
    if let Ok(rows) =
        store.list_amas_suggestions(Some(SuggestionStatus::AutoApplied), 200)
    {
        let today = chrono::Utc::now() - chrono::Duration::days(1);
        let today_count = rows.iter().filter(|r| r.created_at >= today).count() as u32;
        if today_count >= settings.amas_auto_apply_max_per_day {
            return AutoDecision::Skip(format!(
                "今日已自动应用 {today_count} 次 ≥ 上限 {}",
                settings.amas_auto_apply_max_per_day
            ));
        }
    }
    AutoDecision::Apply
}

async fn auto_apply_patch(
    store: &Store,
    engine: &AMASEngine,
    patch_obj: &serde_json::Map<String, serde_json::Value>,
    suggestion_id: i64,
) -> Result<(), String> {
    let cur = engine.get_config();
    let mut cfg_value = serde_json::to_value(&cur).map_err(|e| e.to_string())?;
    for (path, value) in patch_obj {
        crate::workers::llm_advisor::write_path(&mut cfg_value, path, value.clone());
    }
    let new_cfg: crate::amas::config::AMASConfig =
        serde_json::from_value(cfg_value).map_err(|e| format!("反序列化: {e}"))?;
    new_cfg.validate().map_err(|e| format!("validate: {e}"))?;
    engine
        .reload_config(new_cfg.clone())
        .map_err(|e| format!("reload: {e}"))?;
    // 写回 toml（路径从 env 不易获取；fallback 默认）
    let toml_path = std::env::var("AMAS_CONFIG_FILE").unwrap_or_else(|_| "amas_config.toml".to_string());
    if let Err(e) = new_cfg.write_to_toml(&toml_path) {
        tracing::warn!(path = %toml_path, error = %e, "llm_advisor: 写回 toml 失败");
    }
    let snapshot_json = serde_json::to_string(&new_cfg).map_err(|e| e.to_string())?;
    let parent = store
        .list_amas_config_versions(1)
        .ok()
        .and_then(|mut v| v.pop())
        .map(|r| r.version_hash);
    let (vid, vhash) = store
        .insert_amas_config_version(
            &snapshot_json,
            "worker:llm_advisor",
            ConfigVersionSource::LlmAuto,
            Some(&format!("auto apply suggestion#{}", suggestion_id)),
            parent.as_deref(),
        )
        .map_err(|e| e.to_string())?;
    tracing::info!(suggestion_id, version_id = vid, version_hash = %vhash, "llm_advisor: 已自动应用");
    Ok(())
}

/// 通用 path 写入辅助（与 routes/admin/amas.rs 中同名函数语义一致，避免循环依赖）
pub(crate) fn write_path(cfg: &mut serde_json::Value, path: &str, value: serde_json::Value) {
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

fn current_version_hash(engine: &AMASEngine) -> String {
    let cfg = engine.get_config();
    let json = serde_json::to_string(&cfg).unwrap_or_default();
    compute_config_hash(&json)
}

fn collect_evidence(
    store: &Store,
    engine: &AMASEngine,
) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
    let anom = store.aggregate_amas_anomalies(7)?;
    let metrics = store.list_amas_metrics_timeseries(7)?;
    let cfg = engine.get_config();
    let cfg_json = serde_json::to_value(&cfg)?;

    let mut current_params = serde_json::Map::new();
    for entry in TIER_A_WHITELIST {
        if let Some(v) = read_path(&cfg_json, entry.path) {
            current_params.insert(entry.path.to_string(), v);
        }
    }

    Ok(serde_json::json!({
        "windowDays": 7,
        "currentVersionHash": current_version_hash(engine),
        "currentParams": current_params,
        "anomalies": {
            "totalEvents": anom.total_events,
            "anomalyCount": anom.anomaly_count,
            "violationCount": anom.violation_count,
            "topViolationFields": anom.top_violation_fields.iter().take(5).collect::<Vec<_>>(),
        },
        "metrics": metrics.iter().take(60).collect::<Vec<_>>(),
    }))
}

fn read_path(cfg: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
    let mut cur = cfg;
    for part in path.split('.') {
        if let Some(open) = part.find('[') {
            let (key, rest) = part.split_at(open);
            let idx_str = &rest[1..rest.len() - 1];
            let idx: usize = idx_str.parse().ok()?;
            cur = cur.get(key)?.get(idx)?;
        } else {
            cur = cur.get(part)?;
        }
    }
    Some(cur.clone())
}
fn next_month_str(month: &str) -> String {
    let (year_str, mon_str) = month.split_once('-').unwrap_or(("2000","01"));
    let year: i32 = year_str.parse().unwrap_or(2000);
    let mon: u32 = mon_str.parse().unwrap_or(1);
    if mon == 12 { format!("{:04}-01", year + 1) } else { format!("{year:04}-{:02}", mon + 1) }
}
