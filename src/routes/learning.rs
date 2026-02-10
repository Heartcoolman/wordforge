use axum::extract::State;
use axum::routing::post;
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::operations::learning_sessions::{LearningSession, SessionStatus};
use crate::store::operations::words::Word;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/session", post(create_or_resume_session))
        .route("/study-words", post(get_study_words))
        .route("/next-words", post(next_words))
        .route("/adjust-words", post(adjust_words))
        .route("/sync-progress", post(sync_progress))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionResponse {
    session_id: String,
    status: SessionStatus,
    resumed: bool,
}

async fn create_or_resume_session(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // Check for existing active session
    let active = state
        .store()
        .get_active_sessions_for_user(&auth.user_id)?;

    if let Some(existing) = active.into_iter().next() {
        return Ok(ok(SessionResponse {
            session_id: existing.id,
            status: SessionStatus::Active,
            resumed: true,
        }));
    }

    let config = state.store().get_study_config(&auth.user_id)?;

    let session = LearningSession {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: auth.user_id,
        status: SessionStatus::Active,
        target_mastery_count: config.daily_mastery_target,
        total_questions: 0,
        actual_mastery_count: 0,
        context_shifts: 0,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    state.store().create_learning_session(&session)?;

    Ok(ok(SessionResponse {
        session_id: session.id,
        status: SessionStatus::Active,
        resumed: false,
    }))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MasteryStudyWordsResponse {
    words: Vec<Word>,
    strategy: StudyStrategy,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StudyStrategy {
    difficulty_range: (f64, f64),
    new_ratio: f64,
    batch_size: u32,
}

async fn get_study_words(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let config = state.store().get_study_config(&auth.user_id)?;

    // Get AMAS strategy if available
    let amas_state = state.amas().get_user_state(&auth.user_id)?;
    let strategy_params = state.amas().compute_strategy_from_state(&amas_state);

    let batch_size = strategy_params.batch_size as usize;
    let new_ratio = strategy_params.new_ratio;
    let difficulty = strategy_params.difficulty;

    // Collect words from selected wordbooks
    let mut candidate_word_ids = Vec::new();
    for book_id in &config.selected_wordbook_ids {
        let wids = state.store().list_wordbook_words(book_id, 500, 0)?;
        candidate_word_ids.extend(wids);
    }

    // Fallback to general word pool
    if candidate_word_ids.is_empty() {
        let words = state.store().list_words(500, 0)?;
        for w in &words {
            candidate_word_ids.push(w.id.clone());
        }
    }

    candidate_word_ids.sort();
    candidate_word_ids.dedup();

    // Filter by difficulty range and new_ratio using word states
    let new_count = (batch_size as f64 * new_ratio).ceil() as usize;
    let review_count = batch_size.saturating_sub(new_count);

    let mut new_words = Vec::new();
    let mut review_words = Vec::new();

    for wid in &candidate_word_ids {
        let word_state = state.store().get_word_learning_state(&auth.user_id, wid)?;

        if let Some(word) = state.store().get_word(wid)? {
            let diff_ok = (word.difficulty - difficulty).abs() < 0.4;

            match word_state {
                None if new_words.len() < new_count && diff_ok => {
                    new_words.push(word);
                }
                Some(ws)
                    if review_words.len() < review_count
                        && ws.state != crate::store::operations::word_states::WordState::Mastered =>
                {
                    review_words.push(word);
                }
                _ => {}
            }
        }

        if new_words.len() >= new_count && review_words.len() >= review_count {
            break;
        }
    }

    let mut words = new_words;
    words.extend(review_words);

    Ok(ok(MasteryStudyWordsResponse {
        words,
        strategy: StudyStrategy {
            difficulty_range: ((difficulty - 0.2).max(0.0), (difficulty + 0.2).min(1.0)),
            new_ratio,
            batch_size: batch_size as u32,
        },
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NextWordsRequest {
    exclude_word_ids: Vec<String>,
    mastered_word_ids: Option<Vec<String>>,
}

async fn next_words(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<NextWordsRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let config = state.store().get_study_config(&auth.user_id)?;
    let amas_state = state.amas().get_user_state(&auth.user_id)?;
    let strategy_params = state.amas().compute_strategy_from_state(&amas_state);

    let batch_size = strategy_params.batch_size as usize;

    // Mark mastered words
    if let Some(mastered_ids) = &req.mastered_word_ids {
        for wid in mastered_ids {
            if let Some(mut wls) = state.store().get_word_learning_state(&auth.user_id, wid)? {
                wls.state = crate::store::operations::word_states::WordState::Mastered;
                wls.updated_at = Utc::now();
                state.store().set_word_learning_state(&wls)?;
            }
        }
    }

    let mut candidate_word_ids = Vec::new();
    for book_id in &config.selected_wordbook_ids {
        let wids = state.store().list_wordbook_words(book_id, 500, 0)?;
        candidate_word_ids.extend(wids);
    }
    if candidate_word_ids.is_empty() {
        let words = state.store().list_words(500, 0)?;
        candidate_word_ids.extend(words.into_iter().map(|w| w.id));
    }

    candidate_word_ids.sort();
    candidate_word_ids.dedup();

    let exclude_set: std::collections::HashSet<&str> =
        req.exclude_word_ids.iter().map(|s| s.as_str()).collect();

    let mut words = Vec::new();
    for wid in &candidate_word_ids {
        if exclude_set.contains(wid.as_str()) {
            continue;
        }
        if let Some(word) = state.store().get_word(wid)? {
            words.push(word);
            if words.len() >= batch_size {
                break;
            }
        }
    }

    Ok(ok(serde_json::json!({
        "words": words,
        "batchSize": batch_size,
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdjustWordsRequest {
    #[allow(dead_code)]
    user_state: Option<String>,
    #[allow(dead_code)]
    recent_performance: Option<f64>,
}

async fn adjust_words(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(_req): Json<AdjustWordsRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let amas_state = state.amas().get_user_state(&auth.user_id)?;
    let strategy = state.amas().compute_strategy_from_state(&amas_state);

    Ok(ok(serde_json::json!({
        "adjustedStrategy": strategy,
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SyncProgressRequest {
    session_id: String,
    total_questions: Option<u32>,
    context_shifts: Option<u32>,
}

async fn sync_progress(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<SyncProgressRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let mut session = state
        .store()
        .get_learning_session(&req.session_id)?
        .ok_or_else(|| AppError::not_found("Session not found"))?;

    if session.user_id != auth.user_id {
        return Err(AppError::forbidden("Session belongs to another user"));
    }

    // Only increment, never decrease
    if let Some(tq) = req.total_questions {
        if tq > session.total_questions {
            session.total_questions = tq;
        }
    }
    if let Some(cs) = req.context_shifts {
        if cs > session.context_shifts {
            session.context_shifts = cs;
        }
    }

    session.updated_at = Utc::now();
    state.store().update_learning_session(&session)?;

    Ok(ok(session))
}
