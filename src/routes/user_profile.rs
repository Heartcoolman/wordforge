use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::keys;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/reward", get(get_reward_preference).put(set_reward_preference))
        .route("/cognitive", get(get_cognitive_profile))
        .route("/learning-style", get(get_learning_style))
        .route("/chronotype", get(get_chronotype))
        .route("/habit", get(get_habit_profile).post(set_habit_profile))
        .route("/avatar", post(upload_avatar))
}

// B46: Reward preference
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RewardPreference {
    reward_type: String, // standard, explorer, achiever, social
}

async fn get_reward_preference(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let key = keys::user_profile_key(&auth.user_id);
    let pref = match state.store().user_profiles.get(key.as_bytes())
        .map_err(|e| AppError::internal(&e.to_string()))?
    {
        Some(raw) => serde_json::from_slice::<RewardPreference>(&raw)
            .unwrap_or(RewardPreference { reward_type: "standard".to_string() }),
        None => RewardPreference { reward_type: "standard".to_string() },
    };
    Ok(ok(pref))
}

async fn set_reward_preference(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<RewardPreference>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    const VALID_REWARD_TYPES: &[&str] = &["standard", "explorer", "achiever", "social"];
    if !VALID_REWARD_TYPES.contains(&req.reward_type.as_str()) {
        return Err(AppError::bad_request(
            "INVALID_REWARD_TYPE",
            "reward_type must be one of: standard, explorer, achiever, social",
        ));
    }

    let key = keys::user_profile_key(&auth.user_id);
    state.store().user_profiles.insert(
        key.as_bytes(),
        serde_json::to_vec(&req).map_err(|e| AppError::internal(&e.to_string()))?,
    ).map_err(|e| AppError::internal(&e.to_string()))?;
    Ok(ok(req))
}

// B47: Cognitive profile from AMAS
async fn get_cognitive_profile(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_state = state.amas().get_user_state(&auth.user_id)?;
    Ok(ok(user_state.cognitive_profile))
}

// B48: Learning style (VARK)
async fn get_learning_style(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_state = state.amas().get_user_state(&auth.user_id)?;
    let cp = &user_state.cognitive_profile;

    // Simple VARK classification based on cognitive profile
    let style = if cp.processing_speed > 0.7 {
        "visual"
    } else if cp.memory_capacity > 0.7 {
        "auditory"
    } else if cp.stability > 0.7 {
        "reading"
    } else {
        "kinesthetic"
    };

    Ok(ok(serde_json::json!({
        "style": style,
        "scores": {
            "visual": cp.processing_speed,
            "auditory": cp.memory_capacity,
            "reading": cp.stability,
            "kinesthetic": 1.0 - cp.stability,
        }
    })))
}

// B49: Chronotype
async fn get_chronotype(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_state = state.amas().get_user_state(&auth.user_id)?;
    let preferred = &user_state.habit_profile.preferred_hours;

    let chronotype = if preferred.iter().any(|h| *h < 10) {
        "morning"
    } else if preferred.iter().any(|h| *h > 20) {
        "evening"
    } else {
        "neutral"
    };

    Ok(ok(serde_json::json!({
        "chronotype": chronotype,
        "preferredHours": preferred,
    })))
}

// B50: Habit profile
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HabitProfileRequest {
    preferred_hours: Option<Vec<u8>>,
    median_session_length_mins: Option<f64>,
    sessions_per_day: Option<f64>,
}

async fn get_habit_profile(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let key = keys::habit_profile_key(&auth.user_id);
    let profile = match state.store().habit_profiles.get(key.as_bytes())
        .map_err(|e| AppError::internal(&e.to_string()))?
    {
        Some(raw) => serde_json::from_slice::<serde_json::Value>(&raw)
            .unwrap_or(serde_json::json!({})),
        None => {
            let user_state = state.amas().get_user_state(&auth.user_id)?;
            serde_json::to_value(&user_state.habit_profile)
                .unwrap_or(serde_json::json!({}))
        }
    };
    Ok(ok(profile))
}

async fn set_habit_profile(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<HabitProfileRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // Validate preferred_hours: each value must be 0-23
    if let Some(ref hours) = req.preferred_hours {
        if hours.iter().any(|h| *h > 23) {
            return Err(AppError::bad_request(
                "INVALID_PREFERRED_HOURS",
                "preferred_hours values must be between 0 and 23",
            ));
        }
    }

    // Validate sessions_per_day: 1-20
    if let Some(spd) = req.sessions_per_day {
        if !(1.0..=20.0).contains(&spd) {
            return Err(AppError::bad_request(
                "INVALID_SESSIONS_PER_DAY",
                "sessions_per_day must be between 1 and 20",
            ));
        }
    }

    // Validate median_session_length_mins: 1-480
    if let Some(msl) = req.median_session_length_mins {
        if !(1.0..=480.0).contains(&msl) {
            return Err(AppError::bad_request(
                "INVALID_SESSION_LENGTH",
                "median_session_length_mins must be between 1 and 480",
            ));
        }
    }

    let key = keys::habit_profile_key(&auth.user_id);
    let profile = serde_json::json!({
        "preferredHours": req.preferred_hours.unwrap_or(vec![9, 14, 20]),
        "medianSessionLengthMins": req.median_session_length_mins.unwrap_or(15.0),
        "sessionsPerDay": req.sessions_per_day.unwrap_or(1.0),
    });
    state.store().habit_profiles.insert(
        key.as_bytes(),
        serde_json::to_vec(&profile).map_err(|e| AppError::internal(&e.to_string()))?,
    ).map_err(|e| AppError::internal(&e.to_string()))?;
    Ok(ok(profile))
}

// B51: Avatar upload
async fn upload_avatar(
    auth: AuthUser,
    State(_state): State<AppState>,
    body: axum::body::Bytes,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if body.is_empty() {
        return Err(AppError::bad_request("AVATAR_EMPTY", "No file uploaded"));
    }

    // Store avatar to filesystem
    let avatar_dir = "static/avatars";
    let _ = std::fs::create_dir_all(avatar_dir);
    let filename = format!("{}.bin", auth.user_id);
    let path = format!("{}/{}", avatar_dir, filename);

    std::fs::write(&path, &body)
        .map_err(|e| AppError::internal(&format!("Failed to save avatar: {e}")))?;

    Ok(ok(serde_json::json!({
        "avatarUrl": format!("/avatars/{}", filename),
    })))
}
