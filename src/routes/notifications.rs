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
use crate::store::keys;

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
    let notification = state
        .store()
        .mark_notification_read(&auth.user_id, &id)?;

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
    let record_count = store.count_user_records(&auth.user_id)
        .map_err(|e| AppError::internal(&e.to_string()))?;
    let first_word_unlocked = record_count > 0;

    // streak_7: compute streak days from records
    let records = store.get_user_records(&auth.user_id, state.config().limits.max_records_fetch)
        .map_err(|e| AppError::internal(&e.to_string()))?;
    let streak = compute_streak_days(&records);
    let streak_progress = (streak as f64 / 7.0).min(1.0);
    let streak_unlocked = streak >= 7;

    // mastered_100: count mastered words
    let word_stats = store.get_word_state_stats(&auth.user_id)
        .map_err(|e| AppError::internal(&e.to_string()))?;
    let mastered = word_stats.mastered;
    let mastered_progress = (mastered as f64 / 100.0).min(1.0);
    let mastered_unlocked = mastered >= 100;

    let now = Utc::now();

    // Load persisted unlock timestamps
    let load_badge = |badge_id: &str| -> Option<Badge> {
        let key = keys::badge_key(&auth.user_id, badge_id).ok()?;
        let raw = store.badges.get(key.as_bytes()).ok()??;
        serde_json::from_slice::<Badge>(&raw).ok()
    };

    let persisted_first = load_badge("first_word");
    let persisted_streak = load_badge("streak_7");
    let persisted_mastered = load_badge("mastered_100");

    let badges = vec![
        Badge {
            id: "first_word".to_string(),
            name: "First Word".to_string(),
            description: "Learn your first word".to_string(),
            unlocked: first_word_unlocked || persisted_first.as_ref().map_or(false, |b| b.unlocked),
            progress: if first_word_unlocked { 1.0 } else { 0.0 },
            unlocked_at: if first_word_unlocked || persisted_first.as_ref().map_or(false, |b| b.unlocked) {
                persisted_first.as_ref().and_then(|b| b.unlocked_at).or(Some(now))
            } else {
                None
            },
        },
        Badge {
            id: "streak_7".to_string(),
            name: "Week Streak".to_string(),
            description: "Study for 7 consecutive days".to_string(),
            unlocked: streak_unlocked || persisted_streak.as_ref().map_or(false, |b| b.unlocked),
            progress: streak_progress,
            unlocked_at: if streak_unlocked || persisted_streak.as_ref().map_or(false, |b| b.unlocked) {
                persisted_streak.as_ref().and_then(|b| b.unlocked_at).or(Some(now))
            } else {
                None
            },
        },
        Badge {
            id: "mastered_100".to_string(),
            name: "Century Club".to_string(),
            description: "Master 100 words".to_string(),
            unlocked: mastered_unlocked || persisted_mastered.as_ref().map_or(false, |b| b.unlocked),
            progress: mastered_progress,
            unlocked_at: if mastered_unlocked || persisted_mastered.as_ref().map_or(false, |b| b.unlocked) {
                persisted_mastered.as_ref().and_then(|b| b.unlocked_at).or(Some(now))
            } else {
                None
            },
        },
    ];

    // Persist newly unlocked badges
    for badge in &badges {
        if badge.unlocked {
            let key = keys::badge_key(&auth.user_id, &badge.id)
                .map_err(|e| AppError::internal(&e.to_string()))?;
            store
                .badges
                .insert(
                    key.as_bytes(),
                    serde_json::to_vec(badge).map_err(|e| AppError::internal(&e.to_string()))?,
                )
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
    let key = keys::user_preferences_key(&auth.user_id)?;
    let prefs = match state
        .store()
        .user_preferences
        .get(key.as_bytes())
        .map_err(|e| AppError::internal(&e.to_string()))?
    {
        Some(raw) => serde_json::from_slice::<UserPreferences>(&raw).unwrap_or_default(),
        None => UserPreferences::default(),
    };
    Ok(ok(prefs))
}

async fn set_preferences(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<UpdateUserPreferences>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let key = keys::user_preferences_key(&auth.user_id)?;
    let mut prefs = match state
        .store()
        .user_preferences
        .get(key.as_bytes())
        .map_err(|e| AppError::internal(&e.to_string()))?
    {
        Some(raw) => serde_json::from_slice::<UserPreferences>(&raw).unwrap_or_default(),
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

    state
        .store()
        .user_preferences
        .insert(
            key.as_bytes(),
            serde_json::to_vec(&prefs).map_err(|e| AppError::internal(&e.to_string()))?,
        )
        .map_err(|e| AppError::internal(&e.to_string()))?;
    Ok(ok(prefs))
}
