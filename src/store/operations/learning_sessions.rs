use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Active,
    Completed,
    Abandoned,
}

impl Store {
    pub fn create_learning_session(
        &self,
        session: &LearningSession,
    ) -> Result<(), StoreError> {
        let key = keys::learning_session_key(&session.id);
        let index_key =
            keys::learning_session_user_index(&session.user_id, &session.id);
        let session_bytes = Self::serialize(session)?;

        let key_bytes = key.as_bytes().to_vec();
        let index_key_bytes = index_key.as_bytes().to_vec();
        self.learning_sessions
            .transaction(move |tx| {
                tx.insert(key_bytes.as_slice(), session_bytes.as_slice())?;
                tx.insert(index_key_bytes.as_slice(), &[] as &[u8])?;
                Ok(())
            })
            .map_err(
                |e: sled::transaction::TransactionError<()>| match e {
                    sled::transaction::TransactionError::Abort(()) => {
                        StoreError::Sled(sled::Error::Unsupported(
                            "transaction aborted".into(),
                        ))
                    }
                    sled::transaction::TransactionError::Storage(se) => {
                        StoreError::Sled(se)
                    }
                },
            )?;
        Ok(())
    }

    pub fn get_learning_session(
        &self,
        session_id: &str,
    ) -> Result<Option<LearningSession>, StoreError> {
        let key = keys::learning_session_key(session_id);
        match self.learning_sessions.get(key.as_bytes())? {
            Some(raw) => Ok(Some(Self::deserialize(&raw)?)),
            None => Ok(None),
        }
    }

    pub fn update_learning_session(
        &self,
        session: &LearningSession,
    ) -> Result<(), StoreError> {
        let key = keys::learning_session_key(&session.id);
        self.learning_sessions
            .insert(key.as_bytes(), Self::serialize(session)?)?;
        Ok(())
    }

    pub fn get_active_sessions_for_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<LearningSession>, StoreError> {
        let prefix = keys::learning_session_user_index(user_id, "");
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

    pub fn close_active_sessions_for_user(
        &self,
        user_id: &str,
    ) -> Result<u32, StoreError> {
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
}
