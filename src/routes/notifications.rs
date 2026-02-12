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
        None => Err(AppError::not_found("Notification not found")),
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

fn default_badges() -> Vec<Badge> {
    vec![
        Badge {
            id: "first_word".to_string(),
            name: "First Word".to_string(),
            description: "Learn your first word".to_string(),
            unlocked: false,
            progress: 0.0,
            unlocked_at: None,
        },
        Badge {
            id: "streak_7".to_string(),
            name: "Week Streak".to_string(),
            description: "Study for 7 consecutive days".to_string(),
            unlocked: false,
            progress: 0.0,
            unlocked_at: None,
        },
        Badge {
            id: "mastered_100".to_string(),
            name: "Century Club".to_string(),
            description: "Master 100 words".to_string(),
            unlocked: false,
            progress: 0.0,
            unlocked_at: None,
        },
    ]
}

async fn list_badges(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let prefix = keys::badge_prefix(&auth.user_id)?;
    let mut badges = Vec::new();

    for item in state.store().badges.scan_prefix(prefix.as_bytes()) {
        let (_, v) = match item {
            Ok(kv) => kv,
            Err(_) => continue,
        };
        if let Ok(b) = serde_json::from_slice::<Badge>(&v) {
            badges.push(b);
        }
    }

    if badges.is_empty() {
        badges = default_badges();
    }

    Ok(ok(badges))
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
                "theme must be one of: light, dark, system",
            ));
        }
        prefs.theme = v.clone();
    }
    if let Some(ref v) = req.language {
        const VALID_LANGUAGES: &[&str] = &["en", "zh", "ja", "ko", "fr", "de", "es"];
        if !VALID_LANGUAGES.contains(&v.as_str()) {
            return Err(AppError::bad_request(
                "INVALID_LANGUAGE",
                "language must be one of: en, zh, ja, ko, fr, de, es",
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
