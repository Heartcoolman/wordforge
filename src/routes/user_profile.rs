use axum::extract::State;
use axum::routing::{get, post};
use axum::Router;
use std::path::PathBuf;

use crate::auth::AuthUser;
use crate::constants::DEFAULT_PREFERRED_HOURS;
use crate::extractors::JsonBody;
use serde::{Deserialize, Serialize};

use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::keys;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/reward",
            get(get_reward_preference).put(set_reward_preference),
        )
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
    let key = keys::user_profile_key(&auth.user_id)?;
    let pref = match state
        .store()
        .user_profiles
        .get(key.as_bytes())
        .map_err(|e| AppError::internal(&e.to_string()))?
    {
        Some(raw) => serde_json::from_slice::<RewardPreference>(&raw).unwrap_or(RewardPreference {
            reward_type: "standard".to_string(),
        }),
        None => RewardPreference {
            reward_type: "standard".to_string(),
        },
    };
    Ok(ok(pref))
}

async fn set_reward_preference(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<RewardPreference>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    const VALID_REWARD_TYPES: &[&str] = &["standard", "explorer", "achiever", "social"];
    if !VALID_REWARD_TYPES.contains(&req.reward_type.as_str()) {
        return Err(AppError::bad_request(
            "INVALID_REWARD_TYPE",
            "奖励类型必须是以下之一：standard、explorer、achiever、social",
        ));
    }

    let key = keys::user_profile_key(&auth.user_id)?;
    state
        .store()
        .user_profiles
        .insert(
            key.as_bytes(),
            serde_json::to_vec(&req).map_err(|e| AppError::internal(&e.to_string()))?,
        )
        .map_err(|e| AppError::internal(&e.to_string()))?;
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

// B48: Learning style — expose raw cognitive profile data
async fn get_learning_style(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_state = state.amas().get_user_state(&auth.user_id)?;
    let cp = &user_state.cognitive_profile;

    Ok(ok(serde_json::json!({
        "processingSpeed": cp.processing_speed,
        "memoryCapacity": cp.memory_capacity,
        "stability": cp.stability,
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
    let key = keys::habit_profile_key(&auth.user_id)?;
    let profile = match state
        .store()
        .habit_profiles
        .get(key.as_bytes())
        .map_err(|e| AppError::internal(&e.to_string()))?
    {
        Some(raw) => {
            serde_json::from_slice::<serde_json::Value>(&raw).unwrap_or(serde_json::json!({}))
        }
        None => {
            let user_state = state.amas().get_user_state(&auth.user_id)?;
            serde_json::to_value(&user_state.habit_profile).unwrap_or(serde_json::json!({}))
        }
    };
    Ok(ok(profile))
}

async fn set_habit_profile(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<HabitProfileRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // Validate preferred_hours: each value must be 0-23
    if let Some(ref hours) = req.preferred_hours {
        if hours.iter().any(|h| *h > 23) {
            return Err(AppError::bad_request(
                "INVALID_PREFERRED_HOURS",
                "偏好时段的值必须在0到23之间",
            ));
        }
    }

    // Validate sessions_per_day: 1-20
    if let Some(spd) = req.sessions_per_day {
        if !(1.0..=20.0).contains(&spd) {
            return Err(AppError::bad_request(
                "INVALID_SESSIONS_PER_DAY",
                "每日学习次数必须在1到20之间",
            ));
        }
    }

    // Validate median_session_length_mins: 1-480
    if let Some(msl) = req.median_session_length_mins {
        if !(1.0..=480.0).contains(&msl) {
            return Err(AppError::bad_request(
                "INVALID_SESSION_LENGTH",
                "单次学习时长（分钟）必须在1到480之间",
            ));
        }
    }

    let key = keys::habit_profile_key(&auth.user_id)?;
    let profile = serde_json::json!({
        "preferredHours": req.preferred_hours.unwrap_or_else(|| DEFAULT_PREFERRED_HOURS.to_vec()),
        "medianSessionLengthMins": req.median_session_length_mins.unwrap_or(15.0),
        "sessionsPerDay": req.sessions_per_day.unwrap_or(1.0),
    });
    state
        .store()
        .habit_profiles
        .insert(
            key.as_bytes(),
            serde_json::to_vec(&profile).map_err(|e| AppError::internal(&e.to_string()))?,
        )
        .map_err(|e| AppError::internal(&e.to_string()))?;
    Ok(ok(profile))
}

// B51: Avatar upload
fn resolve_avatar_dir() -> PathBuf {
    let cwd_static_dir = PathBuf::from("static");
    if cwd_static_dir.is_dir() {
        return cwd_static_dir.join("avatars");
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("static")
        .join("avatars")
}

async fn upload_avatar(
    auth: AuthUser,
    State(state): State<AppState>,
    body: axum::body::Bytes,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if body.is_empty() {
        return Err(AppError::bad_request("AVATAR_EMPTY", "未上传文件"));
    }

    // 限制头像大小为 512KB
    const MAX_AVATAR_SIZE: usize = 512 * 1024;
    if body.len() > MAX_AVATAR_SIZE {
        return Err(AppError::bad_request(
            "AVATAR_TOO_LARGE",
            "头像文件大小不能超过512KB",
        ));
    }

    // 验证文件类型（通过 magic bytes）
    let extension = match body.get(..4) {
        Some(b"\x89PNG") => "png",
        Some(b"\xFF\xD8\xFF\xE0") | Some(b"\xFF\xD8\xFF\xE1") | Some(b"\xFF\xD8\xFF\xDB") => "jpg",
        Some(bytes) if bytes.starts_with(b"GIF8") => "gif",
        Some(bytes) if bytes.starts_with(b"RIFF") && body.len() > 12 && &body[8..12] == b"WEBP" => {
            "webp"
        }
        _ => {
            return Err(AppError::bad_request(
                "AVATAR_INVALID_TYPE",
                "仅支持 PNG、JPEG、GIF 和 WebP 格式的图片",
            ))
        }
    };

    let avatar_dir = resolve_avatar_dir();
    tokio::fs::create_dir_all(&avatar_dir)
        .await
        .map_err(|e| AppError::internal(&format!("Failed to create avatar directory: {e}")))?;
    // 确保 user_id 不包含路径遍历字符
    let safe_id = auth.user_id.replace(['/', '\\', '.', '\0'], "_");
    let filename = format!("{}.{}", safe_id, extension);
    let path = avatar_dir.join(&filename);

    tokio::fs::write(&path, &body)
        .await
        .map_err(|e| {
            AppError::internal(&format!("Failed to save avatar to {}: {e}", path.display()))
        })?;

    let avatar_url = format!("/avatars/{}", filename);
    let avatar_key = keys::user_avatar_key(&auth.user_id)?;
    let avatar_metadata = serde_json::json!({
        "avatarUrl": avatar_url,
        "filename": filename,
        "extension": extension,
        "sizeBytes": body.len(),
    });
    state
        .store()
        .user_profiles
        .insert(
            avatar_key.as_bytes(),
            serde_json::to_vec(&avatar_metadata).map_err(|e| AppError::internal(&e.to_string()))?,
        )
        .map_err(|e| AppError::internal(&e.to_string()))?;

    Ok(ok(serde_json::json!({
        "avatarUrl": avatar_metadata["avatarUrl"],
    })))
}
