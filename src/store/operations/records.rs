use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    pub fn create_record(&self, record: &LearningRecord) -> Result<(), StoreError> {
        let ts = record.created_at.timestamp_millis();
        let key = keys::record_key(&record.user_id, ts, &record.id);
        self.records
            .insert(key.as_bytes(), Self::serialize(record)?)?;
        Ok(())
    }

    pub fn get_user_records(
        &self,
        user_id: &str,
        limit: usize,
    ) -> Result<Vec<LearningRecord>, StoreError> {
        let prefix = keys::record_prefix(user_id);
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
        let prefix = keys::record_prefix(user_id);
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
    pub fn count_user_records_stats(
        &self,
        user_id: &str,
    ) -> Result<(usize, usize), StoreError> {
        let prefix = keys::record_prefix(user_id);
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
        let prefix = keys::record_prefix(user_id);
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

    pub fn get_user_word_records(
        &self,
        user_id: &str,
        word_id: &str,
        limit: usize,
    ) -> Result<Vec<LearningRecord>, StoreError> {
        let prefix = keys::record_prefix(user_id);
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
