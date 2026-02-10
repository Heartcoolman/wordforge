use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::auth::AdminAuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::keys;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(get_settings).put(update_settings))
}

// B64: System-level settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SystemSettings {
    max_users: u64,
    registration_enabled: bool,
    maintenance_mode: bool,
    default_daily_words: u32,
}

impl Default for SystemSettings {
    fn default() -> Self {
        Self {
            max_users: 10000,
            registration_enabled: true,
            maintenance_mode: false,
            default_daily_words: 20,
        }
    }
}

async fn get_settings(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let key = keys::config_latest_key("system_settings");
    let settings = match state.store().config_versions.get(key.as_bytes())
        .map_err(|e| AppError::internal(&e.to_string()))?
    {
        Some(raw) => serde_json::from_slice::<SystemSettings>(&raw)
            .unwrap_or_default(),
        None => SystemSettings::default(),
    };
    Ok(ok(settings))
}

async fn update_settings(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    Json(req): Json<SystemSettings>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let key = keys::config_latest_key("system_settings");
    state.store().config_versions.insert(
        key.as_bytes(),
        serde_json::to_vec(&req).map_err(|e| AppError::internal(&e.to_string()))?,
    ).map_err(|e| AppError::internal(&e.to_string()))?;
    Ok(ok(req))
}
