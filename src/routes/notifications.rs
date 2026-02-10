use axum::extract::{Path, Query, State};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::keys;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_notifications))
        .route("/:id/read", put(mark_read))
        .route("/read-all", post(mark_all_read))
        .route("/badges", get(list_badges))
        .route("/preferences", get(get_preferences).put(set_preferences))
}

// B57: Notification CRUD
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub id: String,
    pub user_id: String,
    #[serde(rename = "type")]
    pub notification_type: String,
    pub title: String,
    pub message: String,
    pub read: bool,
    pub created_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct NotificationQuery {
    limit: Option<usize>,
    unread_only: Option<bool>,
}

async fn list_notifications(
    auth: AuthUser,
    Query(q): Query<NotificationQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let prefix = keys::notification_prefix(&auth.user_id);
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let unread_only = q.unread_only.unwrap_or(false);

    let mut notifications = Vec::new();
    for item in state.store().notifications.scan_prefix(prefix.as_bytes()) {
        let (_, v) = match item {
            Ok(kv) => kv,
            Err(_) => continue,
        };
        if let Ok(n) = serde_json::from_slice::<Notification>(&v) {
            if unread_only && n.read {
                continue;
            }
            notifications.push(n);
        }
        if notifications.len() >= limit {
            break;
        }
    }

    notifications.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(ok(notifications))
}

async fn mark_read(
    auth: AuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let key = keys::notification_key(&auth.user_id, &id);
    if let Some(raw) = state.store().notifications.get(key.as_bytes())
        .map_err(|e| AppError::internal(&e.to_string()))?
    {
        let mut n: Notification = serde_json::from_slice(&raw)
            .map_err(|e| AppError::internal(&e.to_string()))?;
        n.read = true;
        state.store().notifications.insert(
            key.as_bytes(),
            serde_json::to_vec(&n).map_err(|e| AppError::internal(&e.to_string()))?,
        ).map_err(|e| AppError::internal(&e.to_string()))?;
        return Ok(ok(n));
    }
    Err(AppError::not_found("Notification not found"))
}

async fn mark_all_read(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let prefix = keys::notification_prefix(&auth.user_id);
    let mut count = 0u32;

    let mut updates = Vec::new();
    for item in state.store().notifications.scan_prefix(prefix.as_bytes()) {
        let (k, v) = match item {
            Ok(kv) => kv,
            Err(_) => continue,
        };
        if let Ok(mut n) = serde_json::from_slice::<Notification>(&v) {
            if !n.read {
                n.read = true;
                if let Ok(bytes) = serde_json::to_vec(&n) {
                    updates.push((k.to_vec(), bytes));
                    count += 1;
                }
            }
        }
    }

    for (k, v) in updates {
        let _ = state.store().notifications.insert(k, v);
    }

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
    let prefix = keys::badge_prefix(&auth.user_id);
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

    // If no badges exist, return badge definitions with default state
    if badges.is_empty() {
        badges = vec![
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
        ];
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
            theme: "light".to_string(),
            language: "en".to_string(),
            notification_enabled: true,
            sound_enabled: true,
        }
    }
}

async fn get_preferences(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let key = keys::user_preferences_key(&auth.user_id);
    let prefs = match state.store().user_preferences.get(key.as_bytes())
        .map_err(|e| AppError::internal(&e.to_string()))?
    {
        Some(raw) => serde_json::from_slice::<UserPreferences>(&raw)
            .unwrap_or_default(),
        None => UserPreferences::default(),
    };
    Ok(ok(prefs))
}

async fn set_preferences(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<UserPreferences>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let key = keys::user_preferences_key(&auth.user_id);
    state.store().user_preferences.insert(
        key.as_bytes(),
        serde_json::to_vec(&req).map_err(|e| AppError::internal(&e.to_string()))?,
    ).map_err(|e| AppError::internal(&e.to_string()))?;
    Ok(ok(req))
}
