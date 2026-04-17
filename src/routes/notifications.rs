use std::collections::BTreeSet;

use axum::extract::{Path, Query, State};
use axum::routing::{get, post, put};
use axum::Router;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::constants::{DEFAULT_LANGUAGE, DEFAULT_THEME};
use crate::extractors::JsonBody;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::operations::extras::Badge as StoreBadge;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_notifications))
        .route("/unread-count", get(get_unread_count))
        .route("/:id/read", put(mark_read))
        .route("/read-all", post(mark_all_read))
        .route("/badges", get(list_badges))
        .route("/preferences", get(get_preferences).put(set_preferences))
}

// B57: Notification CRUD
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NotificationQuery {
    limit: Option<usize>,
    unread_only: Option<bool>,
}

async fn list_notifications(
    auth: AuthUser,
    Query(q): Query<NotificationQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let unread_only = q.unread_only.unwrap_or(false);

    let notifications = state
        .store()
        .list_notifications(&auth.user_id, limit, unread_only)?;

    Ok(ok(notifications))
}

async fn get_unread_count(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let unread_count = state.store().count_unread_notifications(&auth.user_id)?;
    Ok(ok(serde_json::json!({"unreadCount": unread_count})))
}

async fn mark_read(
    auth: AuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let notification = state.store().mark_notification_read(&auth.user_id, &id)?;

    match notification {
        Some(notification) => Ok(ok(notification)),
        None => Err(AppError::not_found("通知不存在")),
    }
}

async fn mark_all_read(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let count = state.store().mark_all_notifications_read(&auth.user_id)?;

    Ok(ok(serde_json::json!({"markedRead": count})))
}

// B58: Badges
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Badge {
    id: String,
    name: String,
    description: String,
    unlocked: bool,
    progress: f64,
    unlocked_at: Option<chrono::DateTime<Utc>>,
}

async fn list_badges(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let store = state.store();

    // first_word: check if user has any learning records
    let record_count = store
        .count_user_records(&auth.user_id)
        .map_err(|e| AppError::internal(&e.to_string()))?;
    let first_word_unlocked = record_count > 0;

    // streak_7: compute streak days from records
    let records = store
        .get_user_records(&auth.user_id, state.config().limits.max_records_fetch)
        .map_err(|e| AppError::internal(&e.to_string()))?;
    let streak = compute_streak_days(&records);
    let streak_progress = (streak as f64 / 7.0).min(1.0);
    let streak_unlocked = streak >= 7;

    // mastered_100: count mastered words
    let word_stats = store
        .get_word_state_stats(&auth.user_id)
        .map_err(|e| AppError::internal(&e.to_string()))?;
    let mastered = word_stats.mastered;
    let mastered_progress = (mastered as f64 / 100.0).min(1.0);
    let mastered_unlocked = mastered >= 100;

    let now = Utc::now();

    // Load persisted unlock timestamps
    let load_badge =
        |badge_id: &str| -> Option<StoreBadge> { store.get_badge(&auth.user_id, badge_id).ok()? };

    let persisted_first = load_badge("first_word");
    let persisted_streak = load_badge("streak_7");
    let persisted_mastered = load_badge("mastered_100");

    let make_badge = |id: &str,
                      name: &str,
                      desc: &str,
                      computed_unlocked: bool,
                      progress: f64,
                      persisted: &Option<StoreBadge>| {
        let unlocked = computed_unlocked || persisted.as_ref().is_some_and(|b| b.unlocked);
        let unlocked_at: Option<chrono::DateTime<Utc>> = if unlocked {
            persisted
                .as_ref()
                .and_then(|b| b.unlocked_at.as_deref())
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .or(Some(now))
        } else {
            None
        };
        Badge {
            id: id.to_string(),
            name: name.to_string(),
            description: desc.to_string(),
            unlocked,
            progress: if computed_unlocked {
                progress.max(1.0)
            } else {
                progress
            },
            unlocked_at,
        }
    };

    let badges = vec![
        make_badge(
            "first_word",
            "First Word",
            "Learn your first word",
            first_word_unlocked,
            if first_word_unlocked { 1.0 } else { 0.0 },
            &persisted_first,
        ),
        make_badge(
            "streak_7",
            "Week Streak",
            "Study for 7 consecutive days",
            streak_unlocked,
            streak_progress,
            &persisted_streak,
        ),
        make_badge(
            "mastered_100",
            "Century Club",
            "Master 100 words",
            mastered_unlocked,
            mastered_progress,
            &persisted_mastered,
        ),
    ];

    // Persist newly unlocked badges
    for badge in &badges {
        if badge.unlocked {
            store
                .save_badge(&StoreBadge {
                    user_id: auth.user_id.clone(),
                    id: badge.id.clone(),
                    name: badge.name.clone(),
                    description: badge.description.clone(),
                    unlocked: badge.unlocked,
                    progress: badge.progress,
                    unlocked_at: badge.unlocked_at.map(|dt| dt.to_rfc3339()),
                })
                .map_err(|e| AppError::internal(&e.to_string()))?;
        }
    }

    Ok(ok(badges))
}

fn compute_streak_days(records: &[crate::store::operations::records::LearningRecord]) -> u32 {
    if records.is_empty() {
        return 0;
    }
    let today = Utc::now().date_naive();
    let dates: BTreeSet<chrono::NaiveDate> =
        records.iter().map(|r| r.created_at.date_naive()).collect();
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

// B59: User preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserPreferences {
    theme: String,
    language: String,
    notification_enabled: bool,
    sound_enabled: bool,
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self {
            theme: DEFAULT_THEME.to_string(),
            language: DEFAULT_LANGUAGE.to_string(),
            notification_enabled: true,
            sound_enabled: true,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateUserPreferences {
    theme: Option<String>,
    language: Option<String>,
    notification_enabled: Option<bool>,
    sound_enabled: Option<bool>,
}

async fn get_preferences(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let prefs = match state
        .store()
        .get_user_preferences(&auth.user_id)
        .map_err(|e| AppError::internal(&e.to_string()))?
    {
        Some(val) => UserPreferences {
            theme: val["theme"].as_str().unwrap_or(DEFAULT_THEME).to_string(),
            language: val["language"]
                .as_str()
                .unwrap_or(DEFAULT_LANGUAGE)
                .to_string(),
            notification_enabled: val["notification_enabled"].as_bool().unwrap_or(true),
            sound_enabled: val["sound_enabled"].as_bool().unwrap_or(true),
        },
        None => UserPreferences::default(),
    };
    Ok(ok(prefs))
}

async fn set_preferences(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<UpdateUserPreferences>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let existing = state
        .store()
        .get_user_preferences(&auth.user_id)
        .map_err(|e| AppError::internal(&e.to_string()))?;
    let existing_wbc_url = existing
        .as_ref()
        .and_then(|v| v["wordbook_center_url"].as_str())
        .map(|s| s.to_string());
    let mut prefs = match existing {
        Some(val) => UserPreferences {
            theme: val["theme"].as_str().unwrap_or(DEFAULT_THEME).to_string(),
            language: val["language"]
                .as_str()
                .unwrap_or(DEFAULT_LANGUAGE)
                .to_string(),
            notification_enabled: val["notification_enabled"].as_bool().unwrap_or(true),
            sound_enabled: val["sound_enabled"].as_bool().unwrap_or(true),
        },
        None => UserPreferences::default(),
    };

    if let Some(ref v) = req.theme {
        const VALID_THEMES: &[&str] = &["light", "dark", "system"];
        if !VALID_THEMES.contains(&v.as_str()) {
            return Err(AppError::bad_request(
                "INVALID_THEME",
                "主题必须是以下之一：light、dark、system",
            ));
        }
        prefs.theme = v.clone();
    }
    if let Some(ref v) = req.language {
        const VALID_LANGUAGES: &[&str] = &["en", "zh", "ja", "ko", "fr", "de", "es"];
        if !VALID_LANGUAGES.contains(&v.as_str()) {
            return Err(AppError::bad_request(
                "INVALID_LANGUAGE",
                "语言必须是以下之一：en、zh、ja、ko、fr、de、es",
            ));
        }
        prefs.language = v.clone();
    }
    if let Some(v) = req.notification_enabled {
        prefs.notification_enabled = v;
    }
    if let Some(v) = req.sound_enabled {
        prefs.sound_enabled = v;
    }

    let prefs_val = serde_json::json!({
        "theme": prefs.theme,
        "language": prefs.language,
        "notification_enabled": prefs.notification_enabled,
        "sound_enabled": prefs.sound_enabled,
        "wordbook_center_url": existing_wbc_url,
    });
    state
        .store()
        .set_user_preferences(&auth.user_id, &prefs_val)
        .map_err(|e| AppError::internal(&e.to_string()))?;
    Ok(ok(prefs))
}
