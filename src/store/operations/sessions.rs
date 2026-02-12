use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::store::keys;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
        let key = keys::session_key(&session.token_hash)?;
        let index_key = keys::session_user_index_key(&session.user_id, &session.token_hash)?;
        let session_bytes = Self::serialize(session)?;

        let key_bytes = key.as_bytes().to_vec();
        let index_key_bytes = index_key.as_bytes().to_vec();
        self.sessions
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

    /// 获取会话，如果已过期或已撤销则返回 None。
    /// 不产生删除副作用——过期会话的清理由专用后台任务 cleanup_expired_sessions 负责。
    pub fn get_session(&self, token_hash: &str) -> Result<Option<Session>, StoreError> {
        let key = keys::session_key(token_hash)?;
        let Some(raw) = self.sessions.get(key.as_bytes())? else {
            return Ok(None);
        };

        let session = Self::deserialize::<Session>(&raw)?;
        if session.revoked || session.expires_at <= Utc::now() {
            return Ok(None);
        }

        Ok(Some(session))
    }

    pub fn delete_session(&self, token_hash: &str) -> Result<(), StoreError> {
        let key = keys::session_key(token_hash)?;
        let raw = self.sessions.get(key.as_bytes())?;

        let session_key_bytes = key.as_bytes().to_vec();
        let index_key_bytes = raw
            .as_ref()
            .and_then(|r| Self::deserialize::<Session>(r).ok())
            .and_then(|session| {
                keys::session_user_index_key(&session.user_id, token_hash)
                    .ok()
                    .map(|k| k.as_bytes().to_vec())
            });

        self.sessions
            .transaction(move |tx| {
                if let Some(ref idx_key) = index_key_bytes {
                    tx.remove(idx_key.as_slice())?;
                }
                tx.remove(session_key_bytes.as_slice())?;
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

    /// 原子性删除会话：如果存在则删除并返回 true，不存在则返回 false。
    /// 用于 refresh token 轮换时防止竞态重放攻击。
    /// 在事务内读取并删除，保证原子性。
    pub fn delete_session_if_exists(&self, token_hash: &str) -> Result<bool, StoreError> {
        let key = keys::session_key(token_hash)?;
        let session_key_bytes = key.as_bytes().to_vec();
        let token_hash_owned = token_hash.to_string();

        self.sessions
            .transaction(move |tx| {
                let Some(raw) = tx.remove(session_key_bytes.as_slice())? else {
                    return Ok(false);
                };

                // 尝试删除用户索引
                if let Ok(session) = serde_json::from_slice::<Session>(&raw) {
                    if let Ok(idx_key) =
                        keys::session_user_index_key(&session.user_id, &token_hash_owned)
                    {
                        tx.remove(idx_key.as_bytes())?;
                    }
                }

                Ok(true)
            })
            .map_err(|e: sled::transaction::TransactionError<()>| match e {
                sled::transaction::TransactionError::Abort(()) => {
                    StoreError::Sled(sled::Error::Unsupported("transaction aborted".into()))
                }
                sled::transaction::TransactionError::Storage(se) => StoreError::Sled(se),
            })
    }

    pub fn delete_user_sessions(&self, user_id: &str) -> Result<u32, StoreError> {
        let prefix = keys::session_user_index_prefix(user_id)?;
        let mut hashes = Vec::new();

        for item in self.sessions.scan_prefix(prefix.as_bytes()) {
            let (k, _) = item?;
            let key_str = match String::from_utf8(k.to_vec()) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(error = %e, "Skipping session index key with invalid UTF-8");
                    continue;
                }
            };
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

    /// 统计指定用户的当前会话数
    pub fn count_user_sessions(&self, user_id: &str) -> Result<usize, StoreError> {
        let prefix = keys::session_user_index_prefix(user_id)?;
        let mut count = 0usize;
        for item in self.sessions.scan_prefix(prefix.as_bytes()) {
            let _ = item?;
            count += 1;
        }
        Ok(count)
    }

    /// 如果用户会话数超过 max_sessions，按创建时间从旧到新清理多余会话
    pub fn cleanup_oldest_user_sessions(
        &self,
        user_id: &str,
        max_sessions: usize,
    ) -> Result<(), StoreError> {
        let prefix = keys::session_user_index_prefix(user_id)?;
        let mut sessions: Vec<(String, chrono::DateTime<Utc>)> = Vec::new();

        for item in self.sessions.scan_prefix(prefix.as_bytes()) {
            let (k, _) = item?;
            let key_str = match String::from_utf8(k.to_vec()) {
                Ok(s) => s,
                Err(_) => continue,
            };
            if let Some(hash) = key_str.rsplit(':').next() {
                if let Ok(session_key) = keys::session_key(hash) {
                    if let Some(raw) = self.sessions.get(session_key.as_bytes())? {
                        if let Ok(session) = Self::deserialize::<Session>(&raw) {
                            sessions.push((hash.to_string(), session.created_at));
                        }
                    }
                }
            }
        }

        if sessions.len() <= max_sessions {
            return Ok(());
        }

        // 按创建时间升序排列（最旧的在前）
        sessions.sort_by_key(|(_, created_at)| *created_at);

        let to_remove = sessions.len() - max_sessions;
        for (hash, _) in sessions.into_iter().take(to_remove) {
            self.delete_session(&hash)?;
        }

        Ok(())
    }

    /// 清理过期会话，每批最多处理 1000 条，避免长时间阻塞。
    /// 返回本批次实际删除的会话数。
    pub fn cleanup_expired_sessions(&self) -> Result<u32, StoreError> {
        const MAX_BATCH_SIZE: usize = 1000;

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
                if expired.len() >= MAX_BATCH_SIZE {
                    break;
                }
            }
        }

        let count = expired.len() as u32;
        for token_hash in expired {
            self.delete_session(&token_hash)?;
        }

        Ok(count)
    }

    pub fn create_admin_session(&self, session: &Session) -> Result<(), StoreError> {
        let key = keys::session_key(&session.token_hash)?;
        let index_key = keys::session_user_index_key(&session.user_id, &session.token_hash)?;
        let session_bytes = Self::serialize(session)?;

        let key_bytes = key.as_bytes().to_vec();
        let index_key_bytes = index_key.as_bytes().to_vec();
        self.admin_sessions
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

    /// 获取管理员会话，如果已过期或已撤销则返回 None。
    /// 不产生删除副作用——过期会话的清理由专用后台任务负责。
    pub fn get_admin_session(&self, token_hash: &str) -> Result<Option<Session>, StoreError> {
        let key = keys::session_key(token_hash)?;
        let Some(raw) = self.admin_sessions.get(key.as_bytes())? else {
            return Ok(None);
        };

        let session = Self::deserialize::<Session>(&raw)?;
        if session.revoked || session.expires_at <= Utc::now() {
            return Ok(None);
        }

        Ok(Some(session))
    }

    pub fn delete_admin_session(&self, token_hash: &str) -> Result<(), StoreError> {
        let key = keys::session_key(token_hash)?;
        let session_key_bytes = key.as_bytes().to_vec();
        let index_key_bytes = self
            .admin_sessions
            .get(key.as_bytes())?
            .and_then(|raw| Self::deserialize::<Session>(&raw).ok())
            .and_then(|session| {
                keys::session_user_index_key(&session.user_id, token_hash)
                    .ok()
                    .map(|k| k.as_bytes().to_vec())
            });

        self.admin_sessions
            .transaction(move |tx| {
                if let Some(ref idx_key) = index_key_bytes {
                    tx.remove(idx_key.as_slice())?;
                }
                tx.remove(session_key_bytes.as_slice())?;
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
