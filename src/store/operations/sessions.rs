use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::store::keys;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub token_hash: String,
    pub user_id: String,
    pub token_type: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked: bool,
}

impl Store {
    pub fn create_session(&self, session: &Session) -> Result<(), StoreError> {
        let key = keys::session_key(&session.token_hash);
        let index_key = keys::session_user_index_key(&session.user_id, &session.token_hash);
        let session_bytes = Self::serialize(session)?;

        let key_bytes = key.as_bytes().to_vec();
        let index_key_bytes = index_key.as_bytes().to_vec();
        self.sessions
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

    pub fn get_session(&self, token_hash: &str) -> Result<Option<Session>, StoreError> {
        let key = keys::session_key(token_hash);
        let Some(raw) = self.sessions.get(key.as_bytes())? else {
            return Ok(None);
        };

        let session = Self::deserialize::<Session>(&raw)?;
        if session.revoked || session.expires_at <= Utc::now() {
            if let Err(e) = self.delete_session(token_hash) {
                tracing::warn!(token_hash, error = %e, "Failed to delete expired session");
            }
            return Ok(None);
        }

        Ok(Some(session))
    }

    pub fn delete_session(&self, token_hash: &str) -> Result<(), StoreError> {
        let key = keys::session_key(token_hash);
        let raw = self.sessions.get(key.as_bytes())?;

        let session_key_bytes = key.as_bytes().to_vec();
        let index_key_bytes = raw
            .as_ref()
            .and_then(|r| Self::deserialize::<Session>(r).ok())
            .map(|session| {
                keys::session_user_index_key(&session.user_id, token_hash)
                    .as_bytes()
                    .to_vec()
            });

        self.sessions
            .transaction(move |tx| {
                if let Some(ref idx_key) = index_key_bytes {
                    tx.remove(idx_key.as_slice())?;
                }
                tx.remove(session_key_bytes.as_slice())?;
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

    pub fn delete_user_sessions(&self, user_id: &str) -> Result<u32, StoreError> {
        let prefix = keys::session_user_index_key(user_id, "");
        let mut hashes = Vec::new();

        for item in self.sessions.scan_prefix(prefix.as_bytes()) {
            let (k, _) = item?;
            let key_str = String::from_utf8(k.to_vec()).unwrap_or_default();
            if let Some(hash) = key_str.rsplit(':').next() {
                hashes.push(hash.to_string());
            }
        }

        let count = hashes.len() as u32;
        for hash in hashes {
            self.delete_session(&hash)?;
        }
        Ok(count)
    }

    pub fn cleanup_expired_sessions(&self) -> Result<u32, StoreError> {
        let mut expired = Vec::new();
        for item in self.sessions.iter() {
            let (k, v) = item?;
            let key_str = String::from_utf8_lossy(&k);
            if key_str.starts_with("user:") {
                continue;
            }
            let session: Session = Self::deserialize(&v)?;
            if session.expires_at <= Utc::now() || session.revoked {
                expired.push(session.token_hash);
            }
        }

        let count = expired.len() as u32;
        for token_hash in expired {
            self.delete_session(&token_hash)?;
        }

        Ok(count)
    }

    pub fn create_admin_session(&self, session: &Session) -> Result<(), StoreError> {
        let key = keys::session_key(&session.token_hash);
        let index_key = keys::session_user_index_key(&session.user_id, &session.token_hash);
        let session_bytes = Self::serialize(session)?;

        let key_bytes = key.as_bytes().to_vec();
        let index_key_bytes = index_key.as_bytes().to_vec();
        self.admin_sessions
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

    pub fn get_admin_session(&self, token_hash: &str) -> Result<Option<Session>, StoreError> {
        let key = keys::session_key(token_hash);
        let Some(raw) = self.admin_sessions.get(key.as_bytes())? else {
            return Ok(None);
        };

        let session = Self::deserialize::<Session>(&raw)?;
        if session.revoked || session.expires_at <= Utc::now() {
            if let Err(e) = self.delete_admin_session(token_hash) {
                tracing::warn!(token_hash, error = %e, "Failed to delete expired admin session");
            }
            return Ok(None);
        }

        Ok(Some(session))
    }

    pub fn delete_admin_session(&self, token_hash: &str) -> Result<(), StoreError> {
        let key = keys::session_key(token_hash);
        let session_key_bytes = key.as_bytes().to_vec();
        let index_key_bytes = self
            .admin_sessions
            .get(key.as_bytes())?
            .and_then(|raw| Self::deserialize::<Session>(&raw).ok())
            .map(|session| {
                keys::session_user_index_key(&session.user_id, token_hash)
                    .as_bytes()
                    .to_vec()
            });

        self.admin_sessions
            .transaction(move |tx| {
                if let Some(ref idx_key) = index_key_bytes {
                    tx.remove(idx_key.as_slice())?;
                }
                tx.remove(session_key_bytes.as_slice())?;
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
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use tempfile::tempdir;

    use super::*;

    fn sample_session(token_hash: &str, user_id: &str, expires_in_hours: i64) -> Session {
        Session {
            token_hash: token_hash.to_string(),
            user_id: user_id.to_string(),
            token_type: "user".to_string(),
            created_at: Utc::now(),
            expires_at: Utc::now() + Duration::hours(expires_in_hours),
            revoked: false,
        }
    }

    #[test]
    fn create_and_get_session() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("sessions-db");
        let store = Store::open(db_path.to_str().unwrap()).unwrap();

        let session = sample_session("h1", "u1", 1);
        store.create_session(&session).unwrap();

        let got = store.get_session("h1").unwrap().unwrap();
        assert_eq!(got.user_id, "u1");
    }

    #[test]
    fn cleanup_expired() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("sessions-db2");
        let store = Store::open(db_path.to_str().unwrap()).unwrap();

        store
            .create_session(&sample_session("h_expired", "u1", -1))
            .unwrap();
        store
            .create_session(&sample_session("h_alive", "u1", 1))
            .unwrap();

        let cleaned = store.cleanup_expired_sessions().unwrap();
        assert_eq!(cleaned, 1);
        assert!(store.get_session("h_expired").unwrap().is_none());
        assert!(store.get_session("h_alive").unwrap().is_some());
    }
}
