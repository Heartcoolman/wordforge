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

impl Store {
    pub fn get_study_config(&self, user_id: &str) -> Result<UserStudyConfig, StoreError> {
        let key = keys::study_config_key(user_id)?;
        match self.study_configs.get(key.as_bytes())? {
            Some(raw) => Ok(Self::deserialize(&raw)?),
            None => {
                Ok(UserStudyConfig {
                    user_id: user_id.to_string(),
                    ..Default::default()
                })
            }
        }
    }

    pub fn set_study_config(&self, config: &UserStudyConfig) -> Result<(), StoreError> {
        let key = keys::study_config_key(&config.user_id)?;
        self.study_configs
            .insert(key.as_bytes(), Self::serialize(config)?)?;
        Ok(())
    }
}
