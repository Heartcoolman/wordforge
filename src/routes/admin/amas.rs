use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::Router;

use crate::extractors::JsonBody;
use serde::Deserialize;

use crate::amas::types::RawEvent;
use crate::auth::{AdminAuthUser, AuthUser};
use crate::response::{ok, AppError};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/process-event", post(process_event))
        .route("/batch-process", post(batch_process))
        // B18-B24: AMAS query endpoints
        .route("/state", get(get_amas_state))
        .route("/strategy", get(get_strategy))
        .route("/phase", get(get_phase))
        .route("/learning-curve", get(get_learning_curve))
        .route("/intervention", get(get_intervention))
        .route("/reset", post(reset_state))
        .route("/mastery/evaluate", get(evaluate_mastery))
        .route("/visual-fatigue", post(report_visual_fatigue))
}

/// Admin-only AMAS endpoints (config, metrics, monitoring)
pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/config", get(get_config).put(update_config))
        .route("/metrics", get(get_metrics))
        .route("/monitoring", get(get_monitoring_events))
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
            confused_with: None,
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
                "batch_process accepts at most {} events",
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
    let cfg = state.amas().get_config().await;
    Ok(ok(cfg))
}

async fn update_config(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(cfg): JsonBody<crate::amas::config::AMASConfig>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // 先进行配置验证
    cfg.validate()
        .map_err(|e| AppError::bad_request("AMAS_INVALID_CONFIG", &e))?;

    state
        .amas()
        .reload_config(cfg)
        .await
        .map_err(|e| AppError::bad_request("AMAS_INVALID_CONFIG", &e))?;

    tracing::info!(
        admin_id = %admin.admin_id,
        action = "update_amas_config",
        "管理员更新 AMAS 配置"
    );

    Ok(ok(serde_json::json!({"updated": true})))
}

async fn get_metrics(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    Ok(ok(state.amas().metrics_registry().snapshot()))
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
    let events = state.store().get_recent_monitoring_events(limit)?;
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
            "score must be 0-100",
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
    let user_state = state.amas().get_user_state(&auth.user_id)?;
    Ok(ok(user_state))
}

// B19: GET /api/amas/strategy
async fn get_strategy(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_state = state.amas().get_user_state(&auth.user_id)?;
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
    let records = state.store().get_user_records(&auth.user_id, 1000)?;

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

// B22: GET /api/amas/intervention
async fn get_intervention(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_state = state.amas().get_user_state(&auth.user_id)?;
    let amas_config = state.amas().get_config().await;
    let iv = &amas_config.intervention;
    let mut suggestions = Vec::new();

    if user_state.fatigue > iv.fatigue_alert_threshold {
        suggestions.push(serde_json::json!({
            "type": "rest",
            "message": "You seem fatigued. Consider taking a break.",
            "severity": "warning",
        }));
    }
    if user_state.motivation < iv.motivation_alert_threshold {
        suggestions.push(serde_json::json!({
            "type": "encouragement",
            "message": "Try easier words to rebuild confidence.",
            "severity": "info",
        }));
    }
    if user_state.attention < iv.attention_alert_threshold {
        suggestions.push(serde_json::json!({
            "type": "focus",
            "message": "Your attention seems low. Try a shorter study session.",
            "severity": "warning",
        }));
    }
    if suggestions.is_empty() {
        suggestions.push(serde_json::json!({
            "type": "continue",
            "message": "You're doing great! Keep going.",
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
    state.amas().reset_user_state(&auth.user_id)?;
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
    let word_state = state
        .store()
        .get_word_learning_state(&auth.user_id, &q.word_id)?;

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
