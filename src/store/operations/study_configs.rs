use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::constants::{DEFAULT_DAILY_MASTERY_TARGET, DEFAULT_DAILY_WORDS};
use crate::store::keys;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserStudyConfig {
    pub user_id: String,
    pub selected_wordbook_ids: Vec<String>,
    pub daily_word_count: u32,
    pub study_mode: StudyMode,
    pub daily_mastery_target: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StudyMode {
    Normal,
    Intensive,
    Review,
    Casual,
}

impl Default for UserStudyConfig {
    fn default() -> Self {
        Self {
            user_id: String::new(),
            selected_wordbook_ids: Vec::new(),
            daily_word_count: DEFAULT_DAILY_WORDS,
            study_mode: StudyMode::Normal,
            daily_mastery_target: DEFAULT_DAILY_MASTERY_TARGET,
        }
    }
}

fn study_mode_to_str(mode: &StudyMode) -> &'static str {
    match mode {
        StudyMode::Normal => "normal",
        StudyMode::Intensive => "intensive",
        StudyMode::Review => "review",
        StudyMode::Casual => "casual",
    }
}

fn study_mode_from_str(s: &str) -> StudyMode {
    match s {
        "intensive" => StudyMode::Intensive,
        "review" => StudyMode::Review,
        "casual" => StudyMode::Casual,
        _ => StudyMode::Normal,
    }
}

impl Store {
    pub fn get_study_config(&self, user_id: &str) -> Result<UserStudyConfig, StoreError> {
        let conn = self.conn()?;
        Self::get_study_config_conn(&conn, user_id)
    }

    /// 同 [`get_study_config`],但在调用方提供的连接/事务上执行,供 read-modify-write
    /// 收进单个 `with_user_tx` 事务、规避跨连接读旧快照的并发丢更新。
    pub fn get_study_config_conn(
        conn: &rusqlite::Connection,
        user_id: &str,
    ) -> Result<UserStudyConfig, StoreError> {
        keys::validate_id(user_id)?;
        let row = conn.query_row(
            "SELECT selected_wordbook_ids_json, daily_word_count, study_mode, daily_mastery_target
                 FROM study_configs WHERE user_id = ?1",
            params![user_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        );
        match row {
            Ok((ids_json, daily, mode, target)) => Ok(UserStudyConfig {
                user_id: user_id.to_string(),
                selected_wordbook_ids: Self::deserialize_json(&ids_json)?,
                daily_word_count: daily as u32,
                study_mode: study_mode_from_str(&mode),
                daily_mastery_target: target as u32,
            }),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(UserStudyConfig {
                user_id: user_id.to_string(),
                ..Default::default()
            }),
            Err(e) => Err(e.into()),
        }
    }

    pub fn set_study_config(&self, config: &UserStudyConfig) -> Result<(), StoreError> {
        let conn = self.conn()?;
        Self::set_study_config_conn(&conn, config)
    }

    /// 同 [`set_study_config`],但在调用方提供的连接/事务上执行(见 `get_study_config_conn`)。
    pub fn set_study_config_conn(
        conn: &rusqlite::Connection,
        config: &UserStudyConfig,
    ) -> Result<(), StoreError> {
        keys::validate_id(&config.user_id)?;
        let ids_json = Self::serialize_json(&config.selected_wordbook_ids)?;
        conn.execute(
            "INSERT INTO study_configs (user_id, selected_wordbook_ids_json, daily_word_count, study_mode, daily_mastery_target)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(user_id) DO UPDATE SET
               selected_wordbook_ids_json = excluded.selected_wordbook_ids_json,
               daily_word_count = excluded.daily_word_count,
               study_mode = excluded.study_mode,
               daily_mastery_target = excluded.daily_mastery_target",
            params![
                &config.user_id,
                &ids_json,
                config.daily_word_count as i64,
                study_mode_to_str(&config.study_mode),
                config.daily_mastery_target as i64,
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> Store {
        Store::open(":memory:", 5000, 1).unwrap()
    }

    #[test]
    fn get_default_when_not_set() {
        let store = test_store();
        let config = store.get_study_config("u1").unwrap();
        assert_eq!(config.user_id, "u1");
        assert_eq!(config.study_mode, StudyMode::Normal);
        assert!(config.selected_wordbook_ids.is_empty());
    }

    #[test]
    fn set_and_get() {
        let store = test_store();
        let config = UserStudyConfig {
            user_id: "u1".into(),
            selected_wordbook_ids: vec!["wb1".into(), "wb2".into()],
            daily_word_count: 30,
            study_mode: StudyMode::Intensive,
            daily_mastery_target: 15,
        };
        store.set_study_config(&config).unwrap();
        let got = store.get_study_config("u1").unwrap();
        assert_eq!(got.daily_word_count, 30);
        assert_eq!(got.study_mode, StudyMode::Intensive);
        assert_eq!(got.selected_wordbook_ids, vec!["wb1", "wb2"]);
        assert_eq!(got.daily_mastery_target, 15);
    }

    #[test]
    fn upsert_overwrites() {
        let store = test_store();
        let mut config = UserStudyConfig {
            user_id: "u1".into(),
            daily_word_count: 10,
            ..Default::default()
        };
        store.set_study_config(&config).unwrap();
        config.daily_word_count = 50;
        store.set_study_config(&config).unwrap();
        assert_eq!(store.get_study_config("u1").unwrap().daily_word_count, 50);
    }
}
