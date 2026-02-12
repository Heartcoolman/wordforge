use axum::extract::State;
use axum::routing::get;
use axum::Router;

use crate::auth::AdminAuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/engagement", get(user_engagement))
        .route("/learning", get(learning_metrics))
}

// B61: User engagement analytics
async fn user_engagement(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let total_users = state.store().count_users()?;

    let day_start = chrono::Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .map(|dt| chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(dt, chrono::Utc))
        .unwrap_or_else(chrono::Utc::now);
    let active_today = state.store().count_active_users_since(day_start)?;

    Ok(ok(serde_json::json!({
        "totalUsers": total_users,
        "activeToday": active_today,
        "retentionRate": if total_users > 0 { active_today as f64 / total_users as f64 } else { 0.0 },
    })))
}

// B61: Learning metrics
async fn learning_metrics(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let total_words = state.store().count_words()?;
    let total_records = state.store().count_all_records()? as u64;
    let total_correct = state.store().count_all_correct_records()? as u64;

    Ok(ok(serde_json::json!({
        "totalWords": total_words,
        "totalRecords": total_records,
        "totalCorrect": total_correct,
        "overallAccuracy": if total_records > 0 { total_correct as f64 / total_records as f64 } else { 0.0 },
    })))
}
