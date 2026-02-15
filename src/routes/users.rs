use std::collections::BTreeSet;

use axum::extract::State;
use axum::routing::{get, put};
use axum::Router;

use crate::extractors::JsonBody;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::auth::{hash_password, verify_password, AuthUser};
use crate::response::{ok, AppError};
use crate::routes::auth::UserProfile;
use crate::state::AppState;
use crate::store::operations::records::LearningRecord;
use crate::validation::{validate_password, validate_username};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me", get(get_profile).put(update_profile))
        .route("/me/password", put(change_password))
        .route("/me/stats", get(get_stats))
}

async fn get_profile(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user = state
        .store()
        .get_user_by_id(&auth.user_id)?
        .ok_or_else(|| AppError::not_found("用户不存在"))?;
    Ok(ok(UserProfile::from(&user)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateProfileRequest {
    username: Option<String>,
}

async fn update_profile(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<UpdateProfileRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let mut user = state
        .store()
        .get_user_by_id(&auth.user_id)?
        .ok_or_else(|| AppError::not_found("用户不存在"))?;

    if let Some(username) = req.username {
        let trimmed = username.trim();
        if let Err(msg) = validate_username(trimmed) {
            return Err(AppError::bad_request("USER_INVALID_USERNAME", msg));
        }
        user.username = trimmed.to_string();
    }

    user.updated_at = Utc::now();
    state.store().update_user(&user)?;

    Ok(ok(UserProfile::from(&user)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
}

async fn change_password(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<ChangePasswordRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if let Err(msg) = validate_password(&req.new_password) {
        return Err(AppError::bad_request("AUTH_WEAK_PASSWORD", msg));
    }

    let mut user = state
        .store()
        .get_user_by_id(&auth.user_id)?
        .ok_or_else(|| AppError::not_found("用户不存在"))?;

    if !verify_password(&req.current_password, &user.password_hash)? {
        return Err(AppError::unauthorized("当前密码不正确"));
    }

    user.password_hash = hash_password(&req.new_password)?;
    user.updated_at = Utc::now();
    state.store().update_user(&user)?;
    let _ = state.store().delete_user_sessions(&auth.user_id)?;

    Ok(ok(serde_json::json!({"passwordChanged": true})))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UserStats {
    total_words_learned: u64,
    total_sessions: u64,
    total_records: u64,
    streak_days: u32,
    accuracy_rate: f64,
}

async fn get_stats(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let agg = state.store().get_user_stats_agg(&auth.user_id)?;

    if agg.total_records > 0 {
        // Use pre-aggregated stats
        let accuracy_rate = agg.correct_records as f64 / agg.total_records as f64;

        // Streak still requires date-based scan (lightweight: just keys, not full deser)
        let records = state.store().get_user_records(&auth.user_id, state.config().limits.max_records_fetch)?;

        Ok(ok(UserStats {
            total_words_learned: agg.word_ids.len() as u64,
            total_sessions: agg.session_ids.len() as u64,
            total_records: agg.total_records,
            streak_days: compute_streak_days(&records),
            accuracy_rate,
        }))
    } else {
        // Fallback for users without aggregated stats (pre-migration data)
        let records = state.store().get_user_records(&auth.user_id, state.config().limits.max_records_fetch)?;
        let total_records = records.len() as u64;
        let correct = records.iter().filter(|r| r.is_correct).count() as u64;

        let accuracy_rate = if total_records == 0 {
            0.0
        } else {
            correct as f64 / total_records as f64
        };

        Ok(ok(UserStats {
            total_words_learned: records
                .iter()
                .map(|r| r.word_id.clone())
                .collect::<std::collections::HashSet<_>>()
                .len() as u64,
            total_sessions: records
                .iter()
                .filter_map(|r| r.session_id.clone())
                .collect::<std::collections::HashSet<_>>()
                .len() as u64,
            total_records,
            streak_days: compute_streak_days(&records),
            accuracy_rate,
        }))
    }
}

pub fn compute_streak_days(records: &[LearningRecord]) -> u32 {
    if records.is_empty() {
        return 0;
    }

    let dates: BTreeSet<chrono::NaiveDate> =
        records.iter().map(|r| r.created_at.date_naive()).collect();

    compute_streak_from_dates(&dates)
}

pub fn compute_streak_from_dates(dates: &BTreeSet<chrono::NaiveDate>) -> u32 {
    if dates.is_empty() {
        return 0;
    }

    let today = Utc::now().date_naive();
    let mut streak = 0u32;
    let mut current = today;

    if !dates.contains(&current) {
        match current.pred_opt() {
            Some(yesterday) if dates.contains(&yesterday) => current = yesterday,
            _ => return 0,
        }
    }

    while dates.contains(&current) {
        streak += 1;
        current = match current.pred_opt() {
            Some(d) => d,
            None => break,
        };
    }

    streak
}
