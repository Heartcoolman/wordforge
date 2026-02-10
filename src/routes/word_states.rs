use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use serde::Deserialize;

use crate::auth::AuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::operations::word_states::{WordLearningState, WordState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/batch", post(batch_query))
        .route("/due/list", get(due_list))
        .route("/stats/overview", get(stats_overview))
        .route("/batch-update", post(batch_update))
        .route("/:word_id", get(get_word_state))
        .route("/:word_id/mark-mastered", post(mark_mastered))
        .route("/:word_id/reset", post(reset_word))
}

async fn get_word_state(
    auth: AuthUser,
    Path(word_id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let wls = state
        .store()
        .get_word_learning_state(&auth.user_id, &word_id)?;

    match wls {
        Some(s) => Ok(ok(s)),
        None => Ok(ok(WordLearningState {
            user_id: auth.user_id,
            word_id,
            state: WordState::New,
            mastery_level: 0.0,
            next_review_date: None,
            half_life: 24.0,
            correct_streak: 0,
            total_attempts: 0,
            updated_at: Utc::now(),
        })),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchQueryRequest {
    word_ids: Vec<String>,
}

async fn batch_query(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<BatchQueryRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if req.word_ids.len() > 200 {
        return Err(AppError::bad_request(
            "BATCH_TOO_LARGE",
            "batch_query accepts at most 200 word_ids",
        ));
    }
    let states = state
        .store()
        .get_word_states_batch(&auth.user_id, &req.word_ids)?;
    Ok(ok(states))
}

#[derive(Debug, Deserialize)]
struct DueListQuery {
    limit: Option<usize>,
}

async fn due_list(
    auth: AuthUser,
    Query(q): Query<DueListQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let due = state.store().get_due_words(&auth.user_id, limit)?;
    Ok(ok(due))
}

async fn stats_overview(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let stats = state.store().get_word_state_stats(&auth.user_id)?;
    Ok(ok(stats))
}

async fn mark_mastered(
    auth: AuthUser,
    Path(word_id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let mut wls = state
        .store()
        .get_word_learning_state(&auth.user_id, &word_id)?
        .unwrap_or_else(|| WordLearningState {
            user_id: auth.user_id.clone(),
            word_id: word_id.clone(),
            state: WordState::New,
            mastery_level: 0.0,
            next_review_date: None,
            half_life: 24.0,
            correct_streak: 0,
            total_attempts: 0,
            updated_at: Utc::now(),
        });

    wls.state = WordState::Mastered;
    wls.mastery_level = 1.0;
    wls.updated_at = Utc::now();
    state.store().set_word_learning_state(&wls)?;

    Ok(ok(wls))
}

async fn reset_word(
    auth: AuthUser,
    Path(word_id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let wls = WordLearningState {
        user_id: auth.user_id,
        word_id,
        state: WordState::New,
        mastery_level: 0.0,
        next_review_date: None,
        half_life: 24.0,
        correct_streak: 0,
        total_attempts: 0,
        updated_at: Utc::now(),
    };

    state.store().set_word_learning_state(&wls)?;
    Ok(ok(wls))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchUpdateItem {
    word_id: String,
    state: Option<WordState>,
    mastery_level: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchUpdateRequest {
    updates: Vec<BatchUpdateItem>,
}

async fn batch_update(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<BatchUpdateRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if req.updates.len() > 500 {
        return Err(AppError::bad_request(
            "BATCH_TOO_LARGE",
            "batch_update accepts at most 500 updates",
        ));
    }
    let mut updated = 0usize;
    for item in &req.updates {
        let mut wls = state
            .store()
            .get_word_learning_state(&auth.user_id, &item.word_id)?
            .unwrap_or_else(|| WordLearningState {
                user_id: auth.user_id.clone(),
                word_id: item.word_id.clone(),
                state: WordState::New,
                mastery_level: 0.0,
                next_review_date: None,
                half_life: 24.0,
                correct_streak: 0,
                total_attempts: 0,
                updated_at: Utc::now(),
            });

        if let Some(ref s) = item.state {
            wls.state = s.clone();
        }
        if let Some(level) = item.mastery_level {
            wls.mastery_level = level.clamp(0.0, 1.0);
        }
        wls.updated_at = Utc::now();
        state.store().set_word_learning_state(&wls)?;
        updated += 1;
    }

    Ok(ok(serde_json::json!({"updated": updated})))
}
