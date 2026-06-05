pub mod pick;
pub mod progress;
pub mod session;
pub mod study;

const SYNC_PROGRESS_EVENTS_LIMIT: usize = 100;
const COMPLETE_SESSION_EVENTS_LIMIT: usize = 200;
const EVENT_RESPONSE_TIME_MAX_MS: i64 = 300_000;
const EVENT_CLIENT_TS_BACKFILL_LIMIT_MIN: i64 = 30;
const EVENT_CLIENT_TS_FUTURE_TOLERANCE_MIN: i64 = 1;
const EVENT_CLIENT_EVENT_ID_MAX_LEN: usize = 128;

use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/session", post(session::create_or_resume_session))
        .route("/sessions", get(session::list_sessions))
        .route("/sessions/:id", get(session::get_session))
        .route("/study-words", get(study::get_study_words))
        .route("/next-words", post(study::next_words))
        .route("/adjust-words", post(study::adjust_words))
        .route("/sync-progress", post(progress::sync_progress))
        .route("/complete-session", post(progress::complete_session))
        .route("/pick-next-word", post(pick::pick_next_word))
        .route("/generate-options", post(pick::generate_options))
}
