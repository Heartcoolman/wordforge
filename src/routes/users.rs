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
        .route(
            "/me",
            get(get_profile).put(update_profile).delete(delete_me),
        )
        .route("/me/password", put(change_password))
        .route("/me/stats", get(get_stats))
}

async fn get_profile(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id.clone();
    let user = state
        .run_store_task("users.get_profile", move |store| {
            store.get_user_by_id(&user_id)
        })
        .await??
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
    if let Some(username) = req.username {
        let trimmed = username.trim();
        if let Err(msg) = validate_username(trimmed) {
            return Err(AppError::bad_request("USER_INVALID_USERNAME", msg));
        }
        let user_id = auth.user_id.clone();
        let username = trimmed.to_string();
        let user = state
            .run_store_task(
                "users.update_profile",
                move |store| -> Result<_, AppError> {
                    let mut user = store
                        .get_user_by_id(&user_id)?
                        .ok_or_else(|| AppError::not_found("用户不存在"))?;
                    user.username = username;
                    user.updated_at = Utc::now();
                    store.update_user(&user)?;
                    Ok(user)
                },
            )
            .await??;
        return Ok(ok(UserProfile::from(&user)));
    }

    let user_id = auth.user_id.clone();
    let user = state
        .run_store_task(
            "users.get_profile_passthrough",
            move |store| -> Result<_, AppError> {
                store
                    .get_user_by_id(&user_id)?
                    .ok_or_else(|| AppError::not_found("用户不存在"))
            },
        )
        .await??;
    Ok(ok(UserProfile::from(&user)))
}

async fn delete_me(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id.clone();
    state
        .run_store_task("users.delete_me", move |store| store.delete_user(&user_id))
        .await??;
    Ok(ok(serde_json::json!({ "deleted": true })))
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
    let store = state.store().clone();
    let user_id = auth.user_id.clone();
    let current_password = req.current_password;
    let new_password = req.new_password;
    crate::blocking::run_blocking("users.change_password", move || -> Result<(), AppError> {
        let mut user = store
            .get_user_by_id(&user_id)?
            .ok_or_else(|| AppError::not_found("用户不存在"))?;

        if !verify_password(&current_password, &user.password_hash)? {
            return Err(AppError::unauthorized("当前密码不正确"));
        }

        user.password_hash = hash_password(&new_password)?;
        user.updated_at = Utc::now();
        store.update_user(&user)?;
        let _ = store.delete_user_sessions(&user_id)?;
        Ok(())
    })
    .await??;

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
    let user_id = auth.user_id.clone();
    let max_records_fetch = state.config().limits.max_records_fetch;
    let stats = state
        .run_store_task("users.get_stats", move |store| -> Result<_, AppError> {
            let agg = store.get_user_stats_agg(&user_id)?;
            if agg.total_records > 0 {
                let accuracy_rate = agg.correct_records as f64 / agg.total_records as f64;
                let records = store.get_user_records(&user_id, max_records_fetch)?;
                return Ok(UserStats {
                    total_words_learned: agg.word_ids.len() as u64,
                    total_sessions: agg.session_ids.len() as u64,
                    total_records: agg.total_records,
                    streak_days: compute_streak_days(&records),
                    accuracy_rate,
                });
            }

            let records = store.get_user_records(&user_id, max_records_fetch)?;
            let total_records = records.len() as u64;
            let correct = records.iter().filter(|r| r.is_correct).count() as u64;
            let accuracy_rate = if total_records == 0 {
                0.0
            } else {
                correct as f64 / total_records as f64
            };

            Ok(UserStats {
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
            })
        })
        .await??;
    Ok(ok(stats))
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
