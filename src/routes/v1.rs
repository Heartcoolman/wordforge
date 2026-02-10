//! B79: V1 compatibility routes
//! Thin wrappers mapping /api/v1/* to existing handlers.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use crate::auth::AuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/words", get(list_words))
        .route("/words/:id", get(get_word))
        .route("/records", get(list_records).post(create_record))
        .route("/study-config", get(get_study_config))
        .route("/learning/session", post(create_session))
}

#[derive(Debug, Deserialize)]
struct PaginationQuery {
    limit: Option<usize>,
    offset: Option<usize>,
}

async fn list_words(
    Query(q): Query<PaginationQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(20).clamp(1, 100);
    let offset = q.offset.unwrap_or(0);
    let items = state.store().list_words(limit, offset)?;
    let total = state.store().count_words()?;
    Ok(ok(serde_json::json!({
        "items": items,
        "total": total,
        "limit": limit,
        "offset": offset,
    })))
}

async fn get_word(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let word = state.store().get_word(&id)?
        .ok_or_else(|| AppError::not_found("Word not found"))?;
    Ok(ok(word))
}

async fn list_records(
    auth: AuthUser,
    Query(q): Query<PaginationQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0);
    let records = state.store().get_user_records_with_offset(&auth.user_id, limit, offset)?;
    Ok(ok(records))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct V1RecordRequest {
    word_id: String,
    is_correct: bool,
    response_time_ms: i64,
}

async fn create_record(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<V1RecordRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let record = crate::store::operations::records::LearningRecord {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: auth.user_id.clone(),
        word_id: req.word_id.clone(),
        is_correct: req.is_correct,
        response_time_ms: req.response_time_ms,
        session_id: None,
        created_at: chrono::Utc::now(),
    };
    state.store().create_record(&record)?;
    Ok(ok(record))
}

async fn get_study_config(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let config = state.store().get_study_config(&auth.user_id)?;
    Ok(ok(config))
}

async fn create_session(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let active = state.store().get_active_sessions_for_user(&auth.user_id)?;
    if let Some(existing) = active.into_iter().next() {
        return Ok(ok(serde_json::json!({
            "sessionId": existing.id,
            "resumed": true,
        })));
    }

    let config = state.store().get_study_config(&auth.user_id)?;

    let session = crate::store::operations::learning_sessions::LearningSession {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: auth.user_id,
        status: crate::store::operations::learning_sessions::SessionStatus::Active,
        target_mastery_count: config.daily_mastery_target,
        total_questions: 0,
        actual_mastery_count: 0,
        context_shifts: 0,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    state.store().create_learning_session(&session)?;
    Ok(ok(serde_json::json!({
        "sessionId": session.id,
        "resumed": false,
    })))
}
