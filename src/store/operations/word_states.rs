use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::store::keys;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordLearningState {
    pub user_id: String,
    pub word_id: String,
    pub state: WordState,
    pub mastery_level: f64,
    pub next_review_date: Option<DateTime<Utc>>,
    pub half_life: f64,
    pub correct_streak: u32,
    pub total_attempts: u32,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WordState {
    New,
    Learning,
    Reviewing,
    Mastered,
    Forgotten,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WordStateStats {
    pub new_count: u64,
    pub learning: u64,
    pub reviewing: u64,
    pub mastered: u64,
    pub forgotten: u64,
}

impl Store {
    pub fn get_word_learning_state(
        &self,
        user_id: &str,
        word_id: &str,
    ) -> Result<Option<WordLearningState>, StoreError> {
        let key = keys::word_learning_state_key(user_id, word_id);
        match self.word_learning_states.get(key.as_bytes())? {
            Some(raw) => Ok(Some(Self::deserialize(&raw)?)),
            None => Ok(None),
        }
    }

    pub fn set_word_learning_state(
        &self,
        wls: &WordLearningState,
    ) -> Result<(), StoreError> {
        let key = keys::word_learning_state_key(&wls.user_id, &wls.word_id);
        self.word_learning_states
            .insert(key.as_bytes(), Self::serialize(wls)?)?;
        Ok(())
    }

    pub fn get_word_states_batch(
        &self,
        user_id: &str,
        word_ids: &[String],
    ) -> Result<Vec<WordLearningState>, StoreError> {
        let mut states = Vec::new();
        for wid in word_ids {
            if let Some(s) = self.get_word_learning_state(user_id, wid)? {
                states.push(s);
            }
        }
        Ok(states)
    }

    pub fn get_due_words(
        &self,
        user_id: &str,
        limit: usize,
    ) -> Result<Vec<WordLearningState>, StoreError> {
        let prefix = keys::word_learning_state_prefix(user_id);
        let now = Utc::now();
        let mut due = Vec::new();
        for item in self.word_learning_states.scan_prefix(prefix.as_bytes()) {
            let (_, v) = item?;
            let wls: WordLearningState = Self::deserialize(&v)?;
            if let Some(review_date) = wls.next_review_date {
                if review_date <= now {
                    due.push(wls);
                }
            }
        }
        due.sort_by(|a, b| a.next_review_date.cmp(&b.next_review_date));
        due.truncate(limit);
        Ok(due)
    }

    pub fn get_word_state_stats(
        &self,
        user_id: &str,
    ) -> Result<WordStateStats, StoreError> {
        let prefix = keys::word_learning_state_prefix(user_id);
        let mut stats = WordStateStats::default();
        for item in self.word_learning_states.scan_prefix(prefix.as_bytes()) {
            let (_, v) = item?;
            let wls: WordLearningState = Self::deserialize(&v)?;
            match wls.state {
                WordState::New => stats.new_count += 1,
                WordState::Learning => stats.learning += 1,
                WordState::Reviewing => stats.reviewing += 1,
                WordState::Mastered => stats.mastered += 1,
                WordState::Forgotten => stats.forgotten += 1,
            }
        }
        Ok(stats)
    }

    pub fn delete_word_learning_state(
        &self,
        user_id: &str,
        word_id: &str,
    ) -> Result<(), StoreError> {
        let key = keys::word_learning_state_key(user_id, word_id);
        self.word_learning_states.remove(key.as_bytes())?;
        Ok(())
    }

    pub fn list_user_word_states(
        &self,
        user_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<WordLearningState>, StoreError> {
        let prefix = keys::word_learning_state_prefix(user_id);
        let mut states = Vec::new();
        for item in self.word_learning_states.scan_prefix(prefix.as_bytes()) {
            let (_, v) = item?;
            states.push(Self::deserialize::<WordLearningState>(&v)?);
        }
        Ok(states.into_iter().skip(offset).take(limit).collect())
    }
}
