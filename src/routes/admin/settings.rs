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
    amas_auto_apply_enabled: Option<bool>,
    amas_auto_apply_max_per_day: Option<u32>,
    amas_auto_apply_min_confidence: Option<f64>,
}

impl UpdateSystemSettings {
    fn validate(&self) -> Result<(), AppError> {
        if let Some(v) = self.max_users {
            if !(1..=1_000_000).contains(&v) {
                return Err(AppError::bad_request(
                    "INVALID_MAX_USERS",
                    "最大用户数必须在1到1000000之间",
                ));
            }
        }
        if let Some(v) = self.default_daily_words {
            if !(1..=500).contains(&v) {
                return Err(AppError::bad_request(
                    "INVALID_DAILY_WORDS",
                    "每日默认单词数必须在1到500之间",
                ));
            }
        }
        if let Some(v) = self.amas_auto_apply_max_per_day {
            if v > 20 {
                return Err(AppError::bad_request(
                    "INVALID_AUTO_APPLY_LIMIT",
                    "每日自动应用上限必须 ≤ 20",
                ));
            }
        }
        if let Some(v) = self.amas_auto_apply_min_confidence {
            if !(0.0..=1.0).contains(&v) {
                return Err(AppError::bad_request(
                    "INVALID_CONFIDENCE",
                    "最低置信度必须在 0-1 之间",
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
    let settings = state
        .run_store_task("admin.settings.get_settings", |store| {
            store.get_system_settings()
        })
        .await??;
    Ok(ok(settings))
}

async fn update_settings(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<UpdateSystemSettings>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    req.validate()?;
    let maintenance_mode_changed = req.maintenance_mode.is_some();
    let settings = state
        .run_store_task(
            "admin.settings.update_settings",
            move |store| -> Result<_, AppError> {
                let mut settings = store.get_system_settings()?;

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
                    settings.wordbook_center_url =
                        if v.is_empty() { None } else { Some(v.clone()) };
                }
                if let Some(v) = req.amas_auto_apply_enabled {
                    settings.amas_auto_apply_enabled = v;
                }
                if let Some(v) = req.amas_auto_apply_max_per_day {
                    settings.amas_auto_apply_max_per_day = v;
                }
                if let Some(v) = req.amas_auto_apply_min_confidence {
                    settings.amas_auto_apply_min_confidence = v;
                }

                store.save_system_settings(&settings)?;
                Ok(settings)
            },
        )
        .await??;

    if maintenance_mode_changed {
        state.set_maintenance(settings.maintenance_mode);
    }

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
        .map_err(|e| AppError::bad_request("INVALID_AMAS_CONFIG", &e))?;
    let config = state.amas().get_config();

    tracing::info!(
        admin_id = %admin.admin_id,
        action = "reload_amas_config",
        "管理员重载 AMAS 配置"
    );

    Ok(ok(config))
}
