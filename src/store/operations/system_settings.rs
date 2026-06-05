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
    /// C2/C3:LLM 顾问运行时巡查开关(env ENABLE_LLM_ADVISOR_WORKER 与本列取或)。
    #[serde(default)]
    pub llm_advisor_enabled: bool,
    /// C3:灰度策略三档(20→60→100),存为逗号分隔字符串列,序列化为 [u32;3]。
    #[serde(default = "default_grayscale_steps")]
    pub amas_grayscale_steps: [u32; 3],
    /// E3:canary 自动回滚——reward 平均值降幅阈值(live 比 baseline 低超此绝对值 → 回滚)。
    #[serde(default = "default_canary_threshold")]
    pub canary_reward_drop_threshold: f64,
    /// E3:canary 自动回滚——anomaly 率升幅阈值(live 比 baseline 高超此绝对值 → 回滚)。
    #[serde(default = "default_canary_threshold")]
    pub canary_anomaly_rise_threshold: f64,
    /// D4:客户端最低版本门控——admin 运行时可配的最低 semver,优先于 env MIN_CLIENT_VERSION;
    /// None 表示未设置(回落 env)。
    #[serde(default)]
    pub min_client_version: Option<String>,
    /// D4:版本门控开关。开启后 strict-mode 即使 enabled=false 也按 min_client_version
    /// 对低于阈值客户端返回 CLIENT_OUTDATED(发布切流场景)。
    #[serde(default)]
    pub version_gate_enabled: bool,
}

fn default_grayscale_steps() -> [u32; 3] {
    [20, 60, 100]
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

fn default_canary_threshold() -> f64 {
    0.05
}

/// 解析逗号分隔的灰度档位字符串("20,60,100")为 [u32;3],缺项回落默认。
fn parse_steps(s: &str) -> [u32; 3] {
    let mut it = s.split(',').filter_map(|p| p.trim().parse::<u32>().ok());
    let a = it.next().unwrap_or(20);
    let b = it.next().unwrap_or(60);
    let c = it.next().unwrap_or(100);
    [a, b, c]
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
            llm_advisor_enabled: false,
            amas_grayscale_steps: [20, 60, 100],
            canary_reward_drop_threshold: 0.05,
            canary_anomaly_rise_threshold: 0.05,
            min_client_version: None,
            version_gate_enabled: false,
        }
    }
}

impl Store {
    pub fn get_system_settings(&self) -> Result<SystemSettings, StoreError> {
        let conn = self.conn()?;
        self.get_system_settings_tx(&conn)
    }

    /// `get_system_settings` 的事务内变体:在调用方提供的连接/事务上读取,
    /// 供「同 tx 多表写」复用,避免从 size=1 连接池二次借用导致死锁。
    pub(crate) fn get_system_settings_tx(
        &self,
        conn: &rusqlite::Connection,
    ) -> Result<SystemSettings, StoreError> {
        let result = conn
            .query_row(
                "SELECT max_users, registration_enabled, maintenance_mode, default_daily_words, wordbook_center_url,
                        amas_auto_apply_enabled, amas_auto_apply_max_per_day, amas_auto_apply_min_confidence,
                        llm_advisor_max_cost_per_month_yuan, llm_advisor_enabled, amas_grayscale_steps,
                        canary_reward_drop_threshold, canary_anomaly_rise_threshold,
                        min_client_version, version_gate_enabled
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
                        llm_advisor_enabled: r.get::<_, i64>(9).unwrap_or(0) != 0,
                        amas_grayscale_steps: parse_steps(
                            &r.get::<_, Option<String>>(10).ok().flatten().unwrap_or_default(),
                        ),
                        canary_reward_drop_threshold: r.get::<_, f64>(11).unwrap_or(0.05),
                        canary_anomaly_rise_threshold: r.get::<_, f64>(12).unwrap_or(0.05),
                        min_client_version: r
                            .get::<_, Option<String>>(13)
                            .ok()
                            .flatten()
                            .filter(|s| !s.is_empty()),
                        version_gate_enabled: r.get::<_, i64>(14).unwrap_or(0) != 0,
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
        self.save_system_settings_tx(&conn, settings)
    }

    /// `save_system_settings` 的事务内变体:在调用方提供的连接/事务上写入,
    /// 供「同 tx 多表写」复用。
    pub(crate) fn save_system_settings_tx(
        &self,
        conn: &rusqlite::Connection,
        settings: &SystemSettings,
    ) -> Result<(), StoreError> {
        conn.execute(
            "INSERT INTO system_settings
                (singleton_id, max_users, registration_enabled, maintenance_mode, default_daily_words, wordbook_center_url,
                 amas_auto_apply_enabled, amas_auto_apply_max_per_day, amas_auto_apply_min_confidence,
                 llm_advisor_max_cost_per_month_yuan, llm_advisor_enabled, amas_grayscale_steps,
                 canary_reward_drop_threshold, canary_anomaly_rise_threshold,
                 min_client_version, version_gate_enabled)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(singleton_id) DO UPDATE SET
                max_users=?1, registration_enabled=?2, maintenance_mode=?3, default_daily_words=?4, wordbook_center_url=?5,
                amas_auto_apply_enabled=?6, amas_auto_apply_max_per_day=?7, amas_auto_apply_min_confidence=?8,
                llm_advisor_max_cost_per_month_yuan=?9, llm_advisor_enabled=?10, amas_grayscale_steps=?11,
                canary_reward_drop_threshold=?12, canary_anomaly_rise_threshold=?13,
                min_client_version=?14, version_gate_enabled=?15",
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
                settings.llm_advisor_enabled as i64,
                format!(
                    "{},{},{}",
                    settings.amas_grayscale_steps[0],
                    settings.amas_grayscale_steps[1],
                    settings.amas_grayscale_steps[2]
                ),
                settings.canary_reward_drop_threshold,
                settings.canary_anomaly_rise_threshold,
                settings.min_client_version.as_deref().filter(|s| !s.is_empty()),
                settings.version_gate_enabled as i64,
            ],
        )?;
        Ok(())
    }

    /// 仅切换 llm_advisor_enabled,其余 settings 保留。
    pub fn set_llm_advisor_enabled(&self, enabled: bool) -> Result<(), StoreError> {
        let mut s = self.get_system_settings()?;
        s.llm_advisor_enabled = enabled;
        self.save_system_settings(&s)
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
            llm_advisor_enabled: true,
            amas_grayscale_steps: [10, 50, 100],
            canary_reward_drop_threshold: 0.07,
            canary_anomaly_rise_threshold: 0.09,
            min_client_version: Some("1.2.0".into()),
            version_gate_enabled: true,
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
        assert!(got.llm_advisor_enabled);
        assert_eq!(got.amas_grayscale_steps, [10, 50, 100]);
        assert!((got.canary_reward_drop_threshold - 0.07).abs() < 1e-9);
        assert!((got.canary_anomaly_rise_threshold - 0.09).abs() < 1e-9);
        assert_eq!(got.min_client_version.as_deref(), Some("1.2.0"));
        assert!(got.version_gate_enabled);
    }

    #[test]
    fn version_gate_default_and_roundtrip() {
        let store = test_store();
        // 默认未设置门控
        let s = store.get_system_settings().unwrap();
        assert!(s.min_client_version.is_none());
        assert!(!s.version_gate_enabled);
        // 写入后回读
        let mut s = store.get_system_settings().unwrap();
        s.min_client_version = Some("2.0.0".into());
        s.version_gate_enabled = true;
        store.save_system_settings(&s).unwrap();
        let got = store.get_system_settings().unwrap();
        assert_eq!(got.min_client_version.as_deref(), Some("2.0.0"));
        assert!(got.version_gate_enabled);
        // 清空 min_client_version(空串归一为 None)
        let mut s = store.get_system_settings().unwrap();
        s.min_client_version = Some(String::new());
        store.save_system_settings(&s).unwrap();
        assert!(store
            .get_system_settings()
            .unwrap()
            .min_client_version
            .is_none());
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
                "INSERT INTO system_settings (singleton_id, max_users, registration_enabled, maintenance_mode, default_daily_words, wordbook_center_url, amas_auto_apply_enabled, amas_auto_apply_max_per_day, amas_auto_apply_min_confidence, llm_advisor_max_cost_per_month_yuan, llm_advisor_enabled, amas_grayscale_steps)
                 VALUES (1, 1, 1, 0, 1, NULL, 0, 1, 0.5, 100.0, 0, '20,60,100')",
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

    #[test]
    fn advisor_enabled_default_false_then_set_true() {
        let store = test_store();
        assert!(!store.get_system_settings().unwrap().llm_advisor_enabled);
        store.set_llm_advisor_enabled(true).unwrap();
        assert!(store.get_system_settings().unwrap().llm_advisor_enabled);
        store.set_llm_advisor_enabled(false).unwrap();
        assert!(!store.get_system_settings().unwrap().llm_advisor_enabled);
    }

    #[test]
    fn grayscale_steps_default_and_roundtrip() {
        let store = test_store();
        // 默认 20/60/100
        assert_eq!(
            store.get_system_settings().unwrap().amas_grayscale_steps,
            [20, 60, 100]
        );
        let mut s = store.get_system_settings().unwrap();
        s.amas_grayscale_steps = [10, 50, 100];
        store.save_system_settings(&s).unwrap();
        assert_eq!(
            store.get_system_settings().unwrap().amas_grayscale_steps,
            [10, 50, 100]
        );
    }
}
