use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use crate::auth::AuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::operations::study_configs::StudyMode;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_config).put(update_config))
        .route("/today-words", get(get_today_words))
        .route("/progress", get(get_progress))
}

async fn get_config(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let config = state.store().get_study_config(&auth.user_id)?;
    Ok(ok(config))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateStudyConfigRequest {
    selected_wordbook_ids: Option<Vec<String>>,
    daily_word_count: Option<u32>,
    study_mode: Option<StudyMode>,
    daily_mastery_target: Option<u32>,
}

async fn update_config(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<UpdateStudyConfigRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let mut config = state.store().get_study_config(&auth.user_id)?;

    if let Some(ids) = req.selected_wordbook_ids {
        config.selected_wordbook_ids = ids;
    }
    if let Some(count) = req.daily_word_count {
        config.daily_word_count = count.clamp(1, 200);
    }
    if let Some(mode) = req.study_mode {
        config.study_mode = mode;
    }
    if let Some(target) = req.daily_mastery_target {
        config.daily_mastery_target = target.clamp(1, 100);
    }

    state.store().set_study_config(&config)?;
    Ok(ok(config))
}

async fn get_today_words(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let config = state.store().get_study_config(&auth.user_id)?;
    let mut all_word_ids = Vec::new();

    for book_id in &config.selected_wordbook_ids {
        let word_ids = state
            .store()
            .list_wordbook_words(book_id, config.daily_word_count as usize, 0)?;
        all_word_ids.extend(word_ids);
    }

    // If no wordbooks selected, fall back to general word list
    if all_word_ids.is_empty() {
        let words = state
            .store()
            .list_words(config.daily_word_count as usize, 0)?;
        for w in &words {
            all_word_ids.push(w.id.clone());
        }
    }

    // Deduplicate and limit
    all_word_ids.sort();
    all_word_ids.dedup();
    all_word_ids.truncate(config.daily_word_count as usize);

    let mut words = Vec::new();
    for wid in &all_word_ids {
        if let Some(word) = state.store().get_word(wid)? {
            words.push(word);
        }
    }

    Ok(ok(serde_json::json!({
        "words": words,
        "target": config.daily_word_count,
    })))
}

async fn get_progress(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let config = state.store().get_study_config(&auth.user_id)?;
    let stats = state.store().get_word_state_stats(&auth.user_id)?;

    Ok(ok(serde_json::json!({
        "studied": stats.mastered + stats.reviewing,
        "target": config.daily_mastery_target,
        "new": stats.new_count,
        "learning": stats.learning,
        "reviewing": stats.reviewing,
        "mastered": stats.mastered,
    })))
}
