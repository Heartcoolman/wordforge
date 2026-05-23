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
    /// PR-6: AMAS LLM 调参建议的灰度自动应用开关
    #[serde(default)]
    pub amas_auto_apply_enabled: bool,
    #[serde(default = "default_auto_apply_max_per_day")]
    pub amas_auto_apply_max_per_day: u32,
    #[serde(default = "default_auto_apply_min_confidence")]
    pub amas_auto_apply_min_confidence: f64,
    #[serde(default = "default_llm_max_cost_per_month_yuan")]
    pub llm_advisor_max_cost_per_month_yuan: f64,
}

fn default_auto_apply_max_per_day() -> u32 {
    1
}

fn default_auto_apply_min_confidence() -> f64 {
    0.8
}

fn default_llm_max_cost_per_month_yuan() -> f64 {
    100.0
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
            amas_auto_apply_enabled: false,
            amas_auto_apply_max_per_day: 1,
            amas_auto_apply_min_confidence: 0.8,
            llm_advisor_max_cost_per_month_yuan: 100.0,
        }
    }
}

impl Store {
    pub fn get_system_settings(&self) -> Result<SystemSettings, StoreError> {
        let conn = self.conn()?;
        let result = conn
            .query_row(
                "SELECT max_users, registration_enabled, maintenance_mode, default_daily_words, wordbook_center_url,
                        amas_auto_apply_enabled, amas_auto_apply_max_per_day, amas_auto_apply_min_confidence,
                        llm_advisor_max_cost_per_month_yuan
                 FROM system_settings WHERE singleton_id=1",
                [],
                |r| {
                    Ok(SystemSettings {
                        max_users: r.get::<_, i64>(0)? as u64,
                        registration_enabled: r.get::<_, i64>(1)? != 0,
                        maintenance_mode: r.get::<_, i64>(2)? != 0,
                        default_daily_words: r.get::<_, i64>(3)? as u32,
                        wordbook_center_url: r.get(4)?,
                        amas_auto_apply_enabled: r.get::<_, i64>(5)? != 0,
                        amas_auto_apply_max_per_day: r.get::<_, i64>(6)? as u32,
                        amas_auto_apply_min_confidence: r.get::<_, f64>(7)?,
                        llm_advisor_max_cost_per_month_yuan: r.get::<_, f64>(8).unwrap_or(100.0),
                    })
                },
            )
            .optional()?;
        let mut settings = result.unwrap_or_default();
        if settings.wordbook_center_url.is_none() {
            settings.wordbook_center_url =
                Some("https://cdn.jsdelivr.net/gh/Heartcoolman/wordbook-center@main".into());
        }
        Ok(settings)
    }

    pub fn save_system_settings(&self, settings: &SystemSettings) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO system_settings
                (singleton_id, max_users, registration_enabled, maintenance_mode, default_daily_words, wordbook_center_url,
                 amas_auto_apply_enabled, amas_auto_apply_max_per_day, amas_auto_apply_min_confidence,
                 llm_advisor_max_cost_per_month_yuan)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(singleton_id) DO UPDATE SET
                max_users=?1, registration_enabled=?2, maintenance_mode=?3, default_daily_words=?4, wordbook_center_url=?5,
                amas_auto_apply_enabled=?6, amas_auto_apply_max_per_day=?7, amas_auto_apply_min_confidence=?8,
                llm_advisor_max_cost_per_month_yuan=?9",
            params![
                settings.max_users as i64,
                settings.registration_enabled as i64,
                settings.maintenance_mode as i64,
                settings.default_daily_words as i64,
                settings.wordbook_center_url.as_deref(),
                settings.amas_auto_apply_enabled as i64,
                settings.amas_auto_apply_max_per_day as i64,
                settings.amas_auto_apply_min_confidence,
                settings.llm_advisor_max_cost_per_month_yuan,
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
// 测试用 `let mut cfg = X::default(); cfg.field = v` 易读，本 mod 豁免 field_reassign。
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    fn test_store() -> Store {
        Store::open(":memory:", 5000, 1).unwrap()
    }

    #[test]
    fn get_returns_default_when_empty() {
        let store = test_store();
        let s = store.get_system_settings().unwrap();
        assert_eq!(s.max_users, DEFAULT_MAX_USERS);
        assert!(s.registration_enabled);
        assert!(!s.maintenance_mode);
        assert_eq!(s.default_daily_words, DEFAULT_DAILY_WORDS);
        assert!(s.wordbook_center_url.is_some());
        assert!(!s.amas_auto_apply_enabled);
        assert_eq!(s.amas_auto_apply_max_per_day, 1);
        assert!((s.amas_auto_apply_min_confidence - 0.8).abs() < 1e-9);
    }

    #[test]
    fn save_and_get_roundtrip() {
        let store = test_store();
        let new = SystemSettings {
            max_users: 42,
            registration_enabled: false,
            maintenance_mode: true,
            default_daily_words: 5,
            wordbook_center_url: Some("https://example.com".into()),
            amas_auto_apply_enabled: true,
            amas_auto_apply_max_per_day: 9,
            amas_auto_apply_min_confidence: 0.55,
            llm_advisor_max_cost_per_month_yuan: 200.0,
        };
        store.save_system_settings(&new).unwrap();
        let got = store.get_system_settings().unwrap();
        assert_eq!(got.max_users, 42);
        assert!(!got.registration_enabled);
        assert!(got.maintenance_mode);
        assert_eq!(got.default_daily_words, 5);
        assert_eq!(
            got.wordbook_center_url.as_deref(),
            Some("https://example.com")
        );
        assert!(got.amas_auto_apply_enabled);
        assert_eq!(got.amas_auto_apply_max_per_day, 9);
        assert!((got.amas_auto_apply_min_confidence - 0.55).abs() < 1e-9);
        assert!((got.llm_advisor_max_cost_per_month_yuan - 200.0).abs() < 1e-9);
    }

    #[test]
    fn save_twice_overwrites_via_upsert() {
        let store = test_store();
        let mut s = SystemSettings::default();
        s.max_users = 100;
        store.save_system_settings(&s).unwrap();
        s.max_users = 200;
        store.save_system_settings(&s).unwrap();
        assert_eq!(store.get_system_settings().unwrap().max_users, 200);
    }

    #[test]
    fn get_replaces_null_wordbook_center_url_with_default() {
        let store = test_store();
        // 通过 raw SQL 写入 NULL，模拟 schema 默认场景
        {
            let conn = store.connection().unwrap();
            conn.execute(
                "INSERT INTO system_settings (singleton_id, max_users, registration_enabled, maintenance_mode, default_daily_words, wordbook_center_url, amas_auto_apply_enabled, amas_auto_apply_max_per_day, amas_auto_apply_min_confidence, llm_advisor_max_cost_per_month_yuan)
                 VALUES (1, 1, 1, 0, 1, NULL, 0, 1, 0.5, 100.0)",
                [],
            ).unwrap();
        }
        let got = store.get_system_settings().unwrap();
        assert!(got.wordbook_center_url.is_some());
    }

    #[test]
    fn default_helpers_return_expected_values() {
        assert_eq!(default_auto_apply_max_per_day(), 1);
        assert!((default_auto_apply_min_confidence() - 0.8).abs() < 1e-9);
        assert!((default_llm_max_cost_per_month_yuan() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn deserialize_missing_fields_uses_defaults() {
        let json = r#"{"maxUsers":1,"registrationEnabled":true,"maintenanceMode":false,"defaultDailyWords":1}"#;
        let parsed: SystemSettings = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.amas_auto_apply_max_per_day, 1);
        assert!((parsed.amas_auto_apply_min_confidence - 0.8).abs() < 1e-9);
        assert!(!parsed.amas_auto_apply_enabled);
        assert!(parsed.wordbook_center_url.is_none());
        assert!((parsed.llm_advisor_max_cost_per_month_yuan - 100.0).abs() < 1e-9);
    }
}
