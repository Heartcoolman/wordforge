use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::constants::MAX_CAS_RETRIES;
use crate::store::keys;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningSession {
    pub id: String,
    pub user_id: String,
    pub status: SessionStatus,
    pub target_mastery_count: u32,
    pub total_questions: u32,
    pub actual_mastery_count: u32,
    pub context_shifts: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub summary: Option<SessionSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub accuracy: f64,
    pub avg_response_time_ms: i64,
    pub mastered_word_ids: Vec<String>,
    pub error_prone_word_ids: Vec<String>,
    pub duration_secs: i64,
    pub hour_of_day: u8,
    pub final_difficulty: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Active,
    Completed,
    Abandoned,
}

impl Store {
    pub fn create_learning_session(&self, session: &LearningSession) -> Result<(), StoreError> {
        let key = keys::learning_session_key(&session.id)?;
        let index_key = keys::learning_session_user_index(&session.user_id, &session.id)?;
        let session_bytes = Self::serialize(session)?;

        let key_bytes = key.as_bytes().to_vec();
        let index_key_bytes = index_key.as_bytes().to_vec();
        self.learning_sessions
            .transaction(move |tx| {
                tx.insert(key_bytes.as_slice(), session_bytes.as_slice())?;
                tx.insert(index_key_bytes.as_slice(), &[] as &[u8])?;
                Ok(())
            })
            .map_err(|e: sled::transaction::TransactionError<()>| match e {
                sled::transaction::TransactionError::Abort(()) => {
                    StoreError::Sled(sled::Error::Unsupported("transaction aborted".into()))
                }
                sled::transaction::TransactionError::Storage(se) => StoreError::Sled(se),
            })?;
        Ok(())
    }

    pub fn get_learning_session(
        &self,
        session_id: &str,
    ) -> Result<Option<LearningSession>, StoreError> {
        let key = keys::learning_session_key(session_id)?;
        match self.learning_sessions.get(key.as_bytes())? {
            Some(raw) => Ok(Some(Self::deserialize(&raw)?)),
            None => Ok(None),
        }
    }

    /// 使用 CAS 保护学习会话更新，防止并发写入丢失。
    pub fn update_learning_session(&self, session: &LearningSession) -> Result<(), StoreError> {
        let key = keys::learning_session_key(&session.id)?;
        let new_bytes = Self::serialize(session)?;

        for _ in 0..MAX_CAS_RETRIES {
            let old_raw = self.learning_sessions.get(key.as_bytes())?;
            match self.learning_sessions.compare_and_swap(
                key.as_bytes(),
                old_raw,
                Some(new_bytes.as_slice()),
            )? {
                Ok(()) => return Ok(()),
                Err(_) => continue,
            }
        }
        Err(StoreError::CasRetryExhausted {
            entity: "learning_session".to_string(),
            key: session.id.clone(),
            attempts: MAX_CAS_RETRIES,
        })
    }

    pub fn get_active_sessions_for_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<LearningSession>, StoreError> {
        let prefix = keys::learning_session_user_index_prefix(user_id)?;
        let mut sessions = Vec::new();
        for item in self.learning_sessions.scan_prefix(prefix.as_bytes()) {
            let (k, _) = item?;
            let key_str = String::from_utf8(k.to_vec()).unwrap_or_default();
            if let Some(session_id) = key_str.rsplit(':').next() {
                if let Some(session) = self.get_learning_session(session_id)? {
                    if session.status == SessionStatus::Active {
                        sessions.push(session);
                    }
                }
            }
        }
        Ok(sessions)
    }

    pub fn close_active_sessions_for_user(&self, user_id: &str) -> Result<u32, StoreError> {
        let active = self.get_active_sessions_for_user(user_id)?;
        let mut count = 0u32;
        for mut session in active {
            session.status = SessionStatus::Abandoned;
            session.updated_at = Utc::now();
            self.update_learning_session(&session)?;
            count += 1;
        }
        Ok(count)
    }

    pub fn get_recent_completed_sessions(
        &self,
        user_id: &str,
        max_age_secs: i64,
    ) -> Result<Vec<LearningSession>, StoreError> {
        let prefix = keys::learning_session_user_index_prefix(user_id)?;
        let cutoff = Utc::now() - chrono::Duration::seconds(max_age_secs);
        let mut sessions = Vec::new();
        for item in self.learning_sessions.scan_prefix(prefix.as_bytes()) {
            let (k, _) = item?;
            let key_str = String::from_utf8(k.to_vec()).unwrap_or_default();
            if let Some(session_id) = key_str.rsplit(':').next() {
                if let Some(session) = self.get_learning_session(session_id)? {
                    if session.status == SessionStatus::Completed && session.updated_at >= cutoff {
                        sessions.push(session);
                    }
                }
            }
        }
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(sessions)
    }
}
