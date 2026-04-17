use axum::extract::State;
use axum::routing::get;
use axum::Router;

use crate::extractors::JsonBody;
use chrono::Utc;
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
    let user_id = auth.user_id.clone();
    let config = state
        .run_store_task("study_config.get_config", move |store| {
            store.get_study_config(&user_id)
        })
        .await??;
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
    JsonBody(req): JsonBody<UpdateStudyConfigRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id.clone();
    let config = state
        .run_store_task(
            "study_config.update_config",
            move |store| -> Result<_, AppError> {
                let mut config = store.get_study_config(&user_id)?;

                if let Some(ids) = req.selected_wordbook_ids {
                    for id in &ids {
                        if store.get_wordbook(id)?.is_none() {
                            return Err(AppError::bad_request(
                                "WORDBOOK_NOT_FOUND",
                                &format!("词书 '{}' 不存在", id),
                            ));
                        }
                    }
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

                store.set_study_config(&config)?;
                Ok(config)
            },
        )
        .await??;
    Ok(ok(config))
}

async fn get_today_words(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id.clone();
    let payload = state
        .run_store_task(
            "study_config.get_today_words",
            move |store| -> Result<_, AppError> {
                let config = store.get_study_config(&user_id)?;
                let mut all_word_ids = Vec::new();

                for book_id in &config.selected_wordbook_ids {
                    let word_ids =
                        store.list_wordbook_words(book_id, config.daily_word_count as usize, 0)?;
                    all_word_ids.extend(word_ids);
                }

                if all_word_ids.is_empty() {
                    let words = store.list_words(config.daily_word_count as usize, 0)?;
                    for w in &words {
                        all_word_ids.push(w.id.clone());
                    }
                }

                all_word_ids.sort();
                all_word_ids.dedup();

                let today = Utc::now().date_naive();
                let recent_records = store.get_user_records(&user_id, 500)?;
                let studied_today: std::collections::HashSet<&str> = recent_records
                    .iter()
                    .filter(|r| r.created_at.date_naive() == today)
                    .map(|r| r.word_id.as_str())
                    .collect();
                all_word_ids.retain(|wid| !studied_today.contains(wid.as_str()));
                all_word_ids.truncate(config.daily_word_count as usize);

                let mut words = Vec::new();
                for wid in &all_word_ids {
                    if let Some(word) = store.get_word(wid)? {
                        words.push(word);
                    }
                }

                Ok(serde_json::json!({
                    "words": words,
                    "target": config.daily_word_count,
                }))
            },
        )
        .await??;

    Ok(ok(payload))
}

async fn get_progress(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id.clone();
    let payload = state
        .run_store_task(
            "study_config.get_progress",
            move |store| -> Result<_, AppError> {
                let config = store.get_study_config(&user_id)?;
                let stats = store.get_word_state_stats(&user_id)?;

                Ok(serde_json::json!({
                    "studied": stats.mastered + stats.reviewing,
                    "target": config.daily_mastery_target,
                    "new": stats.new_count,
                    "learning": stats.learning,
                    "reviewing": stats.reviewing,
                    "mastered": stats.mastered,
                }))
            },
        )
        .await??;
    Ok(ok(payload))
}
