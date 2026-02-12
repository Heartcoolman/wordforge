use serde::{Deserialize, Serialize};

use crate::constants::{DEFAULT_DAILY_WORDS, DEFAULT_MAX_USERS};
use crate::store::keys;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSettings {
    pub max_users: u64,
    pub registration_enabled: bool,
    pub maintenance_mode: bool,
    pub default_daily_words: u32,
    #[serde(default)]
    pub wordbook_center_url: Option<String>,
}

fn default_wordbook_center_url() -> Option<String> {
    Some("https://cdn.jsdelivr.net/gh/Heartcoolman/wordbook-center@main".to_string())
}

impl Default for SystemSettings {
    fn default() -> Self {
        Self {
            max_users: DEFAULT_MAX_USERS,
            registration_enabled: true,
            maintenance_mode: false,
            default_daily_words: DEFAULT_DAILY_WORDS,
            wordbook_center_url: Some("https://cdn.jsdelivr.net/gh/Heartcoolman/wordbook-center@main".to_string()),
        }
    }
}

impl Store {
    pub fn get_system_settings(&self) -> Result<SystemSettings, StoreError> {
        let key = keys::config_latest_key("system_settings")?;
        let mut settings = match self.config_versions.get(key.as_bytes())? {
            Some(raw) => match serde_json::from_slice::<SystemSettings>(&raw) {
                Ok(parsed) => parsed,
                Err(error) => {
                    tracing::error!(
                        error = %error,
                        "Failed to deserialize system settings"
                    );
                    return Err(StoreError::Serialization(error));
                }
            },
            None => SystemSettings::default(),
        };
        if settings.wordbook_center_url.is_none() {
            settings.wordbook_center_url = default_wordbook_center_url();
        }
        Ok(settings)
    }

    pub fn save_system_settings(&self, settings: &SystemSettings) -> Result<(), StoreError> {
        let key = keys::config_latest_key("system_settings")?;
        self.config_versions
            .insert(key.as_bytes(), Self::serialize(settings)?)?;
        Ok(())
    }
}
