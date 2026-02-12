use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sled::Transactional;
use std::collections::HashSet;

use crate::store::keys;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningRecord {
    pub id: String,
    pub user_id: String,
    pub word_id: String,
    pub is_correct: bool,
    pub response_time_ms: i64,
    pub session_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Store {
    /// 统计自指定时间以来的活跃用户数。
    /// 注意：record_key 格式为 `{user_id}:{reverse_timestamp}:{record_id}`，
    /// 按字典序排列时同一用户的记录按时间倒序排列。
    /// 但由于需要跨所有用户统计，无法通过单一前缀扫描来利用时间戳有序性。
    /// TODO: 考虑维护一个按全局时间戳排序的二级索引（如 `{reverse_timestamp}:{user_id}:{record_id}`），
    /// 以支持高效的时间范围扫描。
    pub fn count_active_users_since(&self, since: DateTime<Utc>) -> Result<usize, StoreError> {
        let mut active_users: HashSet<String> = HashSet::new();
        for item in self.records.iter() {
            let (_, value) = item?;
            let record: LearningRecord = Self::deserialize(&value)?;
            if record.created_at >= since {
                active_users.insert(record.user_id);
            }
        }
        Ok(active_users.len())
    }

    /// 统计自指定时间以来的学习记录数。
    /// TODO: 同 count_active_users_since，需要全局时间戳索引来避免全表扫描。
    pub fn count_records_since(&self, since: DateTime<Utc>) -> Result<usize, StoreError> {
        let mut count = 0usize;
        for item in self.records.iter() {
            let (_, value) = item?;
            let record: LearningRecord = Self::deserialize(&value)?;
            if record.created_at >= since {
                count += 1;
            }
        }
        Ok(count)
    }

    pub fn create_record(&self, record: &LearningRecord) -> Result<(), StoreError> {
        let ts = record.created_at.timestamp_millis();
        let key = keys::record_key(&record.user_id, ts, &record.id)?;
        self.records
            .insert(key.as_bytes(), Self::serialize(record)?)?;
        Ok(())
    }

    pub fn create_record_with_updates(
        &self,
        record: &LearningRecord,
        word_state: Option<&crate::store::operations::word_states::WordLearningState>,
        learning_session: Option<&crate::store::operations::learning_sessions::LearningSession>,
    ) -> Result<(), StoreError> {
        let ts = record.created_at.timestamp_millis();
        let record_key = keys::record_key(&record.user_id, ts, &record.id)?;
        let record_bytes = Self::serialize(record)?;

        let word_state_payload = if let Some(state) = word_state {
            Some((
                keys::word_learning_state_key(&state.user_id, &state.word_id)?,
                Self::serialize(state)?,
                state
                    .next_review_date
                    .map(|next_review_date| {
                        keys::word_due_index_key(
                            &state.user_id,
                            next_review_date.timestamp_millis(),
                            &state.word_id,
                        )
                    })
                    .transpose()?,
            ))
        } else {
            None
        };

        let session_payload = if let Some(session) = learning_session {
            Some((
                keys::learning_session_key(&session.id)?,
                Self::serialize(session)?,
            ))
        } else {
            None
        };

        (
            &self.records,
            &self.word_learning_states,
            &self.word_due_index,
            &self.learning_sessions,
        )
            .transaction(|(tx_records, tx_word_states, tx_due_index, tx_sessions)| {
                tx_records.insert(record_key.as_bytes(), record_bytes.as_slice())?;

                if let Some((key, bytes, due_index_key)) = &word_state_payload {
                    if let Some(old_raw) = tx_word_states.get(key.as_bytes())? {
                        let old_state: crate::store::operations::word_states::WordLearningState =
                            serde_json::from_slice(&old_raw).map_err(|error| {
                                sled::transaction::ConflictableTransactionError::Abort(
                                    StoreError::Serialization(error),
                                )
                            })?;

                        if let Some(old_due_index_key) = old_state
                            .next_review_date
                            .map(|next_review_date| {
                                keys::word_due_index_key(
                                    &old_state.user_id,
                                    next_review_date.timestamp_millis(),
                                    &old_state.word_id,
                                )
                            })
                            .transpose()
                            .map_err(sled::transaction::ConflictableTransactionError::Abort)?
                        {
                            tx_due_index.remove(old_due_index_key.as_bytes())?;
                        }
                    }

                    tx_word_states.insert(key.as_bytes(), bytes.as_slice())?;

                    if let Some(due_index_key) = due_index_key {
                        tx_due_index.insert(due_index_key.as_bytes(), &[])?;
                    }
                }

                if let Some((key, bytes)) = &session_payload {
                    tx_sessions.insert(key.as_bytes(), bytes.as_slice())?;
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

    pub fn get_user_record_by_id(
        &self,
        user_id: &str,
        record_id: &str,
    ) -> Result<Option<LearningRecord>, StoreError> {
        let prefix = keys::record_prefix(user_id)?;
        let suffix = format!(":{record_id}");

        for item in self.records.scan_prefix(prefix.as_bytes()) {
            let (key, value) = item?;
            let key_text = String::from_utf8_lossy(&key);
            if key_text.ends_with(&suffix) {
                return Ok(Some(Self::deserialize::<LearningRecord>(&value)?));
            }
        }

        Ok(None)
    }

    pub fn get_user_records(
        &self,
        user_id: &str,
        limit: usize,
    ) -> Result<Vec<LearningRecord>, StoreError> {
        let prefix = keys::record_prefix(user_id)?;
        let mut records = Vec::new();
        for item in self.records.scan_prefix(prefix.as_bytes()) {
            let (_, value) = item?;
            records.push(Self::deserialize::<LearningRecord>(&value)?);
            if records.len() >= limit {
                break;
            }
        }
        Ok(records)
    }

    pub fn get_user_records_with_offset(
        &self,
        user_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<LearningRecord>, StoreError> {
        let prefix = keys::record_prefix(user_id)?;
        let mut records = Vec::new();
        let mut skipped = 0usize;
        for item in self.records.scan_prefix(prefix.as_bytes()) {
            let (_, value) = item?;
            if skipped < offset {
                skipped += 1;
                continue;
            }
            records.push(Self::deserialize::<LearningRecord>(&value)?);
            if records.len() >= limit {
                break;
            }
        }
        Ok(records)
    }

    /// Count total and correct records without loading all data into memory.
    pub fn count_user_records_stats(&self, user_id: &str) -> Result<(usize, usize), StoreError> {
        let prefix = keys::record_prefix(user_id)?;
        let mut total = 0usize;
        let mut correct = 0usize;
        for item in self.records.scan_prefix(prefix.as_bytes()) {
            let (_, value) = item?;
            let record: LearningRecord = Self::deserialize(&value)?;
            total += 1;
            if record.is_correct {
                correct += 1;
            }
        }
        Ok((total, correct))
    }

    pub fn count_user_records(&self, user_id: &str) -> Result<usize, StoreError> {
        let prefix = keys::record_prefix(user_id)?;
        let mut count = 0usize;
        for item in self.records.scan_prefix(prefix.as_bytes()) {
            let _ = item?;
            count += 1;
        }
        Ok(count)
    }

    pub fn count_all_records(&self) -> Result<usize, StoreError> {
        let mut count = 0usize;
        for item in self.records.iter() {
            let _ = item?;
            count += 1;
        }
        Ok(count)
    }

    /// 统计所有 is_correct=true 的记录数，避免逐用户统计
    pub fn count_all_correct_records(&self) -> Result<usize, StoreError> {
        let mut count = 0usize;
        for item in self.records.iter() {
            let (_, value) = item?;
            let record: LearningRecord = Self::deserialize(&value)?;
            if record.is_correct {
                count += 1;
            }
        }
        Ok(count)
    }

    pub fn get_user_word_records(
        &self,
        user_id: &str,
        word_id: &str,
        limit: usize,
    ) -> Result<Vec<LearningRecord>, StoreError> {
        let prefix = keys::record_prefix(user_id)?;
        let mut records = Vec::new();
        for item in self.records.scan_prefix(prefix.as_bytes()) {
            let (_, value) = item?;
            let record: LearningRecord = Self::deserialize(&value)?;
            if record.word_id == word_id {
                records.push(record);
                if records.len() >= limit {
                    break;
                }
            }
        }
        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use tempfile::tempdir;

    use super::*;

    fn sample_record(
        id: &str,
        user_id: &str,
        word_id: &str,
        created_at: DateTime<Utc>,
    ) -> LearningRecord {
        LearningRecord {
            id: id.to_string(),
            user_id: user_id.to_string(),
            word_id: word_id.to_string(),
            is_correct: true,
            response_time_ms: 1000,
            session_id: Some("s1".to_string()),
            created_at,
        }
    }

    #[test]
    fn records_are_returned_in_desc_time_order() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("records-db");
        let store = Store::open(db_path.to_str().unwrap()).unwrap();

        let now = Utc::now();
        let old = sample_record("r1", "u1", "w1", now - Duration::seconds(30));
        let new = sample_record("r2", "u1", "w1", now);

        store.create_record(&old).unwrap();
        store.create_record(&new).unwrap();

        let list = store.get_user_records("u1", 10).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "r2");
        assert_eq!(list[1].id, "r1");
    }
}
