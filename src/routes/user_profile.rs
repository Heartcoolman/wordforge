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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RewardPreference {
    reward_type: String,
}

async fn get_reward_preference(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let pref = match state
        .run_store_task("user_profile.get_reward_preference", move |store| {
            store.get_reward_preference(&auth.user_id)
        })
        .await??
    {
        Some(val) => RewardPreference {
            reward_type: val
                .get("reward_type")
                .and_then(|v| v.as_str())
                .unwrap_or("standard")
                .to_string(),
        },
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

    let val = serde_json::json!({"reward_type": req.reward_type});
    state
        .run_store_task("user_profile.set_reward_preference", move |store| {
            store.set_reward_preference(&auth.user_id, &val)
        })
        .await??;
    Ok(ok(req))
}

async fn get_cognitive_profile(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_state = state.amas().get_user_state_async(&auth.user_id).await?;
    Ok(ok(user_state.cognitive_profile))
}

async fn get_learning_style(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_state = state.amas().get_user_state_async(&auth.user_id).await?;
    let cp = &user_state.cognitive_profile;
    Ok(ok(serde_json::json!({
        "processingSpeed": cp.processing_speed,
        "memoryCapacity": cp.memory_capacity,
        "stability": cp.stability,
    })))
}

async fn get_chronotype(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_state = state.amas().get_user_state_async(&auth.user_id).await?;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HabitProfileRequest {
    preferred_hours: Option<Vec<u8>>,
    median_session_length_mins: Option<f64>,
    sessions_per_day: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HabitProfileResponse {
    preferred_hours: Vec<u8>,
    median_session_length_mins: f64,
    sessions_per_day: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    temporal_performance: Option<TemporalPerformanceResponse>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TemporalPerformanceResponse {
    hourly_stats: Vec<HourlyStatsResponse>,
    total_sessions: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HourlyStatsResponse {
    session_count: u32,
    avg_accuracy: f64,
    avg_response_time_ms: f64,
    mastery_efficiency: f64,
}

fn value_u8_array(value: Option<&serde_json::Value>) -> Option<Vec<u8>> {
    value.and_then(|items| items.as_array()).map(|items| {
        items
            .iter()
            .filter_map(|item| item.as_u64())
            .filter_map(|item| u8::try_from(item).ok())
            .collect()
    })
}

fn habit_profile_from_value(value: &serde_json::Value) -> HabitProfileResponse {
    let preferred_hours = value_u8_array(
        value
            .get("preferredHours")
            .or_else(|| value.get("preferred_hours")),
    )
    .unwrap_or_else(|| DEFAULT_PREFERRED_HOURS.to_vec());
    let median_session_length_mins = value
        .get("medianSessionLengthMins")
        .or_else(|| value.get("median_session_length_mins"))
        .and_then(|v| v.as_f64())
        .unwrap_or(15.0);
    let sessions_per_day = value
        .get("sessionsPerDay")
        .or_else(|| value.get("sessions_per_day"))
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);

    let temporal_performance = if let Some(tp) = value.get("temporalPerformance") {
        let hourly_stats = tp
            .get("hourlyStats")
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .map(|item| HourlyStatsResponse {
                        session_count: item
                            .get("sessionCount")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as u32,
                        avg_accuracy: item
                            .get("avgAccuracy")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0),
                        avg_response_time_ms: item
                            .get("avgResponseTimeMs")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0),
                        mastery_efficiency: item
                            .get("masteryEfficiency")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(TemporalPerformanceResponse {
            hourly_stats,
            total_sessions: tp
                .get("totalSessions")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
        })
    } else if value.get("temporal_hourly_stats").is_some()
        || value.get("temporal_total_sessions").is_some()
    {
        let hourly_stats = value
            .get("temporal_hourly_stats")
            .and_then(|v| v.as_array())
            .map(|items| {
                items
                    .iter()
                    .map(|item| HourlyStatsResponse {
                        session_count: item
                            .get("session_count")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as u32,
                        avg_accuracy: item
                            .get("avg_accuracy")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0),
                        avg_response_time_ms: item
                            .get("avg_response_time_ms")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0),
                        mastery_efficiency: item
                            .get("mastery_efficiency")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(TemporalPerformanceResponse {
            hourly_stats,
            total_sessions: value
                .get("temporal_total_sessions")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32,
        })
    } else {
        None
    };

    HabitProfileResponse {
        preferred_hours,
        median_session_length_mins,
        sessions_per_day,
        temporal_performance,
    }
}

async fn get_habit_profile(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id.clone();
    let profile = match state
        .run_store_task("user_profile.get_habit_profile", move |store| {
            store.get_habit_profile(&user_id)
        })
        .await??
    {
        Some(val) => val,
        None => {
            let user_state = state.amas().get_user_state_async(&auth.user_id).await?;
            serde_json::to_value(&user_state.habit_profile).unwrap_or(serde_json::json!({}))
        }
    };
    Ok(ok(habit_profile_from_value(&profile)))
}

async fn set_habit_profile(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<HabitProfileRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if let Some(ref hours) = req.preferred_hours {
        if hours.iter().any(|h| *h > 23) {
            return Err(AppError::bad_request(
                "INVALID_PREFERRED_HOURS",
                "偏好时段的值必须在0到23之间",
            ));
        }
    }
    if let Some(spd) = req.sessions_per_day {
        if !(1.0..=20.0).contains(&spd) {
            return Err(AppError::bad_request(
                "INVALID_SESSIONS_PER_DAY",
                "每日学习次数必须在1到20之间",
            ));
        }
    }
    if let Some(msl) = req.median_session_length_mins {
        if !(1.0..=480.0).contains(&msl) {
            return Err(AppError::bad_request(
                "INVALID_SESSION_LENGTH",
                "单次学习时长（分钟）必须在1到480之间",
            ));
        }
    }

    let profile = serde_json::json!({
        "preferred_hours": req.preferred_hours.unwrap_or_else(|| DEFAULT_PREFERRED_HOURS.to_vec()),
        "median_session_length_mins": req.median_session_length_mins.unwrap_or(15.0),
        "sessions_per_day": req.sessions_per_day.unwrap_or(1.0),
    });
    let profile_for_store = profile.clone();
    state
        .run_store_task("user_profile.set_habit_profile", move |store| {
            store.set_habit_profile(&auth.user_id, &profile_for_store)
        })
        .await??;
    Ok(ok(habit_profile_from_value(&profile)))
}

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

    const MAX_AVATAR_SIZE: usize = 512 * 1024;
    if body.len() > MAX_AVATAR_SIZE {
        return Err(AppError::bad_request(
            "AVATAR_TOO_LARGE",
            "头像文件大小不能超过512KB",
        ));
    }

    let extension = match body.get(..4) {
        Some(b"\x89PNG") => "png",
        // 所有 JPEG 均以 SOI(FF D8) + FF + 任意 APPn/标记字节起始；仅校验前三字节
        // 以接纳 E0/E1/E2(ICC)/EE(Adobe)/DB 等全部合法变体。
        Some(bytes) if bytes.starts_with(b"\xFF\xD8\xFF") => "jpg",
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
    let safe_id = auth.user_id.replace(['/', '\\', '.', '\0'], "_");
    let filename = format!("{}.{}", safe_id, extension);
    let path = avatar_dir.join(&filename);

    // 顺序：先写新文件 → 再更新 DB → 最后才 best-effort 清理旧格式文件。
    // 保证 DB avatarUrl 指向的文件在任意时刻都已落盘存在，避免并发换格式上传时
    // 「删除步骤先于另一请求的 DB 写入」导致 DB 指向已被删除文件（404）的不一致。
    tokio::fs::write(&path, &body).await.map_err(|e| {
        AppError::internal(&format!("Failed to save avatar to {}: {e}", path.display()))
    })?;

    let avatar_url = format!("/avatars/{}", filename);
    let avatar_metadata = serde_json::json!({
        "avatarUrl": avatar_url,
        "filename": filename,
        "extension": extension,
        "sizeBytes": body.len(),
    });
    let avatar_metadata_for_store = avatar_metadata.clone();
    // set_user_avatar 在同事务内返回被替换掉的旧 filename（若与新文件名不同）。
    let prev_filename = state
        .run_store_task("user_profile.upload_avatar", move |store| {
            store.set_user_avatar(&auth.user_id, &avatar_metadata_for_store)
        })
        .await??;

    // 仅删除 DB 中确切记录的前一份文件，而非盲删所有其他扩展名——后者在同用户并发换
    // 格式上传时会删掉另一请求刚落盘并写入 DB 的文件，导致 DB.avatarUrl 指向 404。
    if let Some(old) = prev_filename {
        // 防御性约束：只允许形如 "<safe_id>.<ext>" 的本用户文件名，杜绝路径穿越。
        if old != filename && !old.contains('/') && !old.contains('\\') && !old.contains("..") {
            let _ = tokio::fs::remove_file(avatar_dir.join(&old)).await;
        }
    }

    Ok(ok(serde_json::json!({
        "avatarUrl": avatar_metadata["avatarUrl"],
    })))
}
