use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sled::Transactional;
use std::collections::{HashMap, HashSet};

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

fn due_index_key_for_state(wls: &WordLearningState) -> Result<Option<String>, StoreError> {
    match wls.next_review_date {
        Some(next_review_date) => Ok(Some(keys::word_due_index_key(
            &wls.user_id,
            next_review_date.timestamp_millis(),
            &wls.word_id,
        )?)),
        None => Ok(None),
    }
}

impl Store {
    pub fn get_word_learning_state(
        &self,
        user_id: &str,
        word_id: &str,
    ) -> Result<Option<WordLearningState>, StoreError> {
        let key = keys::word_learning_state_key(user_id, word_id)?;
        match self.word_learning_states.get(key.as_bytes())? {
            Some(raw) => Ok(Some(Self::deserialize(&raw)?)),
            None => Ok(None),
        }
    }

    pub fn set_word_learning_state(&self, wls: &WordLearningState) -> Result<(), StoreError> {
        let key = keys::word_learning_state_key(&wls.user_id, &wls.word_id)?;
        let value = Self::serialize(wls)?;
        let next_due_index_key = due_index_key_for_state(wls)?;

        (&self.word_learning_states, &self.word_due_index)
            .transaction(|(tx_states, tx_due_index)| {
                if let Some(old_raw) = tx_states.get(key.as_bytes())? {
                    let old_state: WordLearningState =
                        serde_json::from_slice(&old_raw).map_err(|error| {
                            sled::transaction::ConflictableTransactionError::Abort(
                                StoreError::Serialization(error),
                            )
                        })?;
                    if let Some(old_due_index_key) = due_index_key_for_state(&old_state)
                        .map_err(sled::transaction::ConflictableTransactionError::Abort)?
                    {
                        tx_due_index.remove(old_due_index_key.as_bytes())?;
                    }
                }

                tx_states.insert(key.as_bytes(), value.as_slice())?;

                if let Some(due_index_key) = &next_due_index_key {
                    tx_due_index.insert(due_index_key.as_bytes(), &[])?;
                }

                Ok(())
            })
            .map_err(
                |error: sled::transaction::TransactionError<StoreError>| match error {
                    sled::transaction::TransactionError::Abort(store_error) => store_error,
                    sled::transaction::TransactionError::Storage(storage_error) => {
                        StoreError::Sled(storage_error)
                    }
                },
            )?;

        Ok(())
    }

    pub fn get_word_states_batch(
        &self,
        user_id: &str,
        word_ids: &[String],
    ) -> Result<Vec<WordLearningState>, StoreError> {
        let mut state_by_word_id: HashMap<&str, Option<WordLearningState>> =
            HashMap::with_capacity(word_ids.len());

        for wid in word_ids {
            if state_by_word_id.contains_key(wid.as_str()) {
                continue;
            }

            let key = keys::word_learning_state_key(user_id, wid)?;
            let state = self
                .word_learning_states
                .get(key.as_bytes())?
                .map(|raw| Self::deserialize(&raw))
                .transpose()?;
            state_by_word_id.insert(wid.as_str(), state);
        }

        let mut states = Vec::with_capacity(word_ids.len());
        for wid in word_ids {
            if let Some(Some(state)) = state_by_word_id.get(wid.as_str()) {
                states.push(state.clone());
            }
        }

        Ok(states)
    }

    pub fn get_due_words(
        &self,
        user_id: &str,
        limit: usize,
    ) -> Result<Vec<WordLearningState>, StoreError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let prefix = keys::word_due_index_prefix(user_id)?;
        let now = Utc::now().timestamp_millis().max(0);
        let mut due = Vec::with_capacity(limit);
        let mut seen_word_ids = HashSet::new();

        for item in self.word_due_index.scan_prefix(prefix.as_bytes()) {
            let (key, _) = item?;
            let Some((due_ts_ms, word_id)) = keys::parse_due_index_item_key(&key) else {
                continue;
            };

            if due_ts_ms > now {
                break;
            }

            if let Some(state) = self.get_word_learning_state(user_id, &word_id)? {
                if let Some(next_review_date) = state.next_review_date {
                    let state_due_ts_ms = next_review_date.timestamp_millis().max(0);
                    if state_due_ts_ms == due_ts_ms
                        && state_due_ts_ms <= now
                        && seen_word_ids.insert(word_id)
                    {
                        due.push(state);
                        if due.len() >= limit {
                            break;
                        }
                    }
                }
            }
        }

        Ok(due)
    }

    pub fn get_word_state_stats(&self, user_id: &str) -> Result<WordStateStats, StoreError> {
        let prefix = keys::word_learning_state_prefix(user_id)?;
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
        let key = keys::word_learning_state_key(user_id, word_id)?;

        (&self.word_learning_states, &self.word_due_index)
            .transaction(|(tx_states, tx_due_index)| {
                let removed = tx_states.remove(key.as_bytes())?;

                if let Some(raw) = removed {
                    let removed_state: WordLearningState =
                        serde_json::from_slice(&raw).map_err(|error| {
                            sled::transaction::ConflictableTransactionError::Abort(
                                StoreError::Serialization(error),
                            )
                        })?;
                    if let Some(due_index_key) = due_index_key_for_state(&removed_state)
                        .map_err(sled::transaction::ConflictableTransactionError::Abort)?
                    {
                        tx_due_index.remove(due_index_key.as_bytes())?;
                    }
                }

                Ok(())
            })
            .map_err(
                |error: sled::transaction::TransactionError<StoreError>| match error {
                    sled::transaction::TransactionError::Abort(store_error) => store_error,
                    sled::transaction::TransactionError::Storage(storage_error) => {
                        StoreError::Sled(storage_error)
                    }
                },
            )?;

        Ok(())
    }

    pub fn list_user_word_states(
        &self,
        user_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<WordLearningState>, StoreError> {
        let prefix = keys::word_learning_state_prefix(user_id)?;
        let mut states = Vec::new();
        for item in self.word_learning_states.scan_prefix(prefix.as_bytes()) {
            let (_, v) = item?;
            states.push(Self::deserialize::<WordLearningState>(&v)?);
        }
        Ok(states.into_iter().skip(offset).take(limit).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::{WordLearningState, WordState};
    use crate::store::Store;
    use chrono::{Duration, Utc};
    use tempfile::tempdir;

    fn mock_word_learning_state(
        user_id: &str,
        word_id: &str,
        total_attempts: u32,
    ) -> WordLearningState {
        WordLearningState {
            user_id: user_id.to_string(),
            word_id: word_id.to_string(),
            state: WordState::Learning,
            mastery_level: 0.42,
            next_review_date: None,
            half_life: 24.0,
            correct_streak: 1,
            total_attempts,
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn get_word_states_batch_preserves_order_duplicates_and_skips_missing() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path().join("db").to_str().unwrap()).unwrap();

        let w1 = mock_word_learning_state("u1", "w1", 3);
        let w3 = mock_word_learning_state("u1", "w3", 7);
        store.set_word_learning_state(&w1).unwrap();
        store.set_word_learning_state(&w3).unwrap();

        let results = store
            .get_word_states_batch(
                "u1",
                &[
                    "w3".to_string(),
                    "missing".to_string(),
                    "w1".to_string(),
                    "w3".to_string(),
                ],
            )
            .unwrap();

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].word_id, "w3");
        assert_eq!(results[1].word_id, "w1");
        assert_eq!(results[2].word_id, "w3");
        assert_eq!(results[0].total_attempts, 7);
        assert_eq!(results[1].total_attempts, 3);
        assert_eq!(results[2].total_attempts, 7);
    }

    #[test]
    fn get_due_words_returns_asc_order_and_respects_limit() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path().join("db-due-order").to_str().unwrap()).unwrap();

        let now = Utc::now();
        let mut w1 = mock_word_learning_state("u1", "w1", 1);
        w1.next_review_date = Some(now - Duration::minutes(5));
        let mut w2 = mock_word_learning_state("u1", "w2", 1);
        w2.next_review_date = Some(now - Duration::minutes(1));
        let mut w3 = mock_word_learning_state("u1", "w3", 1);
        w3.next_review_date = Some(now - Duration::minutes(3));
        let mut w4 = mock_word_learning_state("u1", "w4", 1);
        w4.next_review_date = Some(now + Duration::minutes(1));

        store.set_word_learning_state(&w1).unwrap();
        store.set_word_learning_state(&w2).unwrap();
        store.set_word_learning_state(&w3).unwrap();
        store.set_word_learning_state(&w4).unwrap();

        let due = store.get_due_words("u1", 2).unwrap();

        assert_eq!(due.len(), 2);
        assert_eq!(due[0].word_id, "w1");
        assert_eq!(due[1].word_id, "w3");
    }

    #[test]
    fn get_due_words_uses_latest_review_date_after_update() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path().join("db-due-update").to_str().unwrap()).unwrap();

        let now = Utc::now();
        let mut state = mock_word_learning_state("u1", "w1", 1);
        state.next_review_date = Some(now - Duration::minutes(5));
        store.set_word_learning_state(&state).unwrap();

        state.next_review_date = Some(now - Duration::minutes(1));
        store.set_word_learning_state(&state).unwrap();

        let due = store.get_due_words("u1", 10).unwrap();

        assert_eq!(due.len(), 1);
        assert_eq!(due[0].word_id, "w1");
        assert_eq!(due[0].next_review_date, state.next_review_date);
    }

    #[test]
    fn deleted_word_state_disappears_from_due_words() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path().join("db-due-delete").to_str().unwrap()).unwrap();

        let now = Utc::now();
        let mut state = mock_word_learning_state("u1", "w1", 1);
        state.next_review_date = Some(now - Duration::minutes(2));
        store.set_word_learning_state(&state).unwrap();

        assert_eq!(store.get_due_words("u1", 10).unwrap().len(), 1);

        store.delete_word_learning_state("u1", "w1").unwrap();

        assert!(store.get_due_words("u1", 10).unwrap().is_empty());
    }
}
