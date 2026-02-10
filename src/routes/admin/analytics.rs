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
    let users = state.store().list_users(usize::MAX, 0)?;
    let total_users = users.len();

    let mut active_today = 0usize;
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    for user in &users {
        let records = state.store().get_user_records(&user.id, 1)?;
        if let Some(r) = records.first() {
            if r.created_at.format("%Y-%m-%d").to_string() == today {
                active_today += 1;
            }
        }
    }

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
    let users = state.store().list_users(usize::MAX, 0)?;

    let mut total_records = 0u64;
    let mut total_correct = 0u64;
    for user in &users {
        let records = state.store().get_user_records(&user.id, usize::MAX)?;
        for r in &records {
            total_records += 1;
            if r.is_correct {
                total_correct += 1;
            }
        }
    }

    Ok(ok(serde_json::json!({
        "totalWords": total_words,
        "totalRecords": total_records,
        "totalCorrect": total_correct,
        "overallAccuracy": if total_records > 0 { total_correct as f64 / total_records as f64 } else { 0.0 },
    })))
}
