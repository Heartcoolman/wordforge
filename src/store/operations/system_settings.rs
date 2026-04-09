use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::constants::{DEFAULT_DAILY_WORDS, DEFAULT_MAX_USERS};
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

impl Default for SystemSettings {
    fn default() -> Self {
        Self {
            max_users: DEFAULT_MAX_USERS,
            registration_enabled: true,
            maintenance_mode: false,
            default_daily_words: DEFAULT_DAILY_WORDS,
            wordbook_center_url: Some(
                "https://cdn.jsdelivr.net/gh/Heartcoolman/wordbook-center@main".into(),
            ),
        }
    }
}

impl Store {
    pub fn get_system_settings(&self) -> Result<SystemSettings, StoreError> {
        let conn = self.conn()?;
        let result = conn
            .query_row(
                "SELECT max_users, registration_enabled, maintenance_mode, default_daily_words, wordbook_center_url
                 FROM system_settings WHERE singleton_id=1",
                [],
                |r| {
                    Ok(SystemSettings {
                        max_users: r.get::<_, i64>(0)? as u64,
                        registration_enabled: r.get::<_, i64>(1)? != 0,
                        maintenance_mode: r.get::<_, i64>(2)? != 0,
                        default_daily_words: r.get::<_, i64>(3)? as u32,
                        wordbook_center_url: r.get(4)?,
                    })
                },
            )
            .optional()?;
        let mut settings = result.unwrap_or_default();
        if settings.wordbook_center_url.is_none() {
            settings.wordbook_center_url = Some(
                "https://cdn.jsdelivr.net/gh/Heartcoolman/wordbook-center@main".into(),
            );
        }
        Ok(settings)
    }

    pub fn save_system_settings(&self, settings: &SystemSettings) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO system_settings (singleton_id, max_users, registration_enabled, maintenance_mode, default_daily_words, wordbook_center_url)
             VALUES (1, ?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(singleton_id) DO UPDATE SET
                max_users=?1, registration_enabled=?2, maintenance_mode=?3, default_daily_words=?4, wordbook_center_url=?5",
            params![
                settings.max_users as i64,
                settings.registration_enabled as i64,
                settings.maintenance_mode as i64,
                settings.default_daily_words as i64,
                settings.wordbook_center_url.as_deref(),
            ],
        )?;
        Ok(())
    }
}
