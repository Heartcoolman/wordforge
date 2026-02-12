use axum::extract::State;
use axum::routing::{get, post};
use axum::Router;

use crate::extractors::JsonBody;
use serde::Deserialize;

use crate::amas::config::AMASConfig;
use crate::auth::AdminAuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_settings).put(update_settings))
        .route("/reload-amas", post(reload_amas_config))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateSystemSettings {
    max_users: Option<u64>,
    registration_enabled: Option<bool>,
    maintenance_mode: Option<bool>,
    default_daily_words: Option<u32>,
    wordbook_center_url: Option<String>,
}

impl UpdateSystemSettings {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(v) = self.max_users {
            if !(1..=1_000_000).contains(&v) {
                return Err(AppError::bad_request(
                    "INVALID_MAX_USERS",
                    "max_users must be 1-1000000",
                ));
            }
        }
        if let Some(v) = self.default_daily_words {
            if !(1..=500).contains(&v) {
                return Err(AppError::bad_request(
                    "INVALID_DAILY_WORDS",
                    "default_daily_words must be 1-500",
                ));
            }
        }
        Ok(())
    }
}

async fn get_settings(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let settings = state.store().get_system_settings()?;
    Ok(ok(settings))
}

async fn update_settings(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<UpdateSystemSettings>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    req.validate()?;

    let mut settings = state.store().get_system_settings()?;

    if let Some(v) = req.max_users {
        settings.max_users = v;
    }
    if let Some(v) = req.registration_enabled {
        settings.registration_enabled = v;
    }
    if let Some(v) = req.maintenance_mode {
        settings.maintenance_mode = v;
    }
    if let Some(v) = req.default_daily_words {
        settings.default_daily_words = v;
    }
    if let Some(ref v) = req.wordbook_center_url {
        settings.wordbook_center_url = if v.is_empty() { None } else { Some(v.clone()) };
    }

    state.store().save_system_settings(&settings)?;

    tracing::info!(
        admin_id = %admin.admin_id,
        action = "update_settings",
        "管理员更新系统设置: max_users={}, registration={}, maintenance={}, daily_words={}",
        settings.max_users, settings.registration_enabled, settings.maintenance_mode, settings.default_daily_words
    );

    Ok(ok(settings))
}

async fn reload_amas_config(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(new_config): JsonBody<AMASConfig>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    new_config
        .validate()
        .map_err(|e| AppError::bad_request("INVALID_AMAS_CONFIG", &e))?;
    state
        .amas()
        .reload_config(new_config)
        .await
        .map_err(|e| AppError::bad_request("INVALID_AMAS_CONFIG", &e))?;
    let config = state.amas().get_config().await;

    tracing::info!(
        admin_id = %admin.admin_id,
        action = "reload_amas_config",
        "管理员重载 AMAS 配置"
    );

    Ok(ok(config))
}
