use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::constants::{LOCKOUT_DURATION_MINUTES, MAX_CAS_RETRIES, MAX_FAILED_LOGIN_ATTEMPTS};
use crate::store::keys;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub email: String,
    pub username: String,
    /// 安全提示：此字段仅用于内部存储和密码验证，不得通过 API 返回。
    /// API 层应使用 UserProfile 或 AdminUserView 等安全视图类型。
    pub password_hash: String,
    pub is_banned: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub failed_login_count: u32,
    #[serde(default)]
    pub locked_until: Option<DateTime<Utc>>,
}

impl Store {
    /// 统计用户数量。
    /// 利用 users tree 的 len() 和 email 索引前缀扫描来高效计算：
    /// 用户数 = 总条目数 - email 索引条目数。
    /// 这避免了全表反序列化，但仍需遍历 email 前缀来计数索引条目。
    /// TODO: 如果性能仍不够，可维护单独的原子计数器。
    pub fn count_users(&self) -> Result<usize, StoreError> {
        let total = self.users.len();
        let mut email_index_count = 0usize;
        for item in self.users.scan_prefix(b"email:") {
            let _ = item?;
            email_index_count += 1;
        }
        Ok(total - email_index_count)
    }

    pub fn create_user(&self, user: &User) -> Result<(), StoreError> {
        let email_key = keys::user_email_index_key(&user.email)?;
        let user_key = keys::user_key(&user.id)?;
        let uid_bytes = user.id.as_bytes().to_vec();
        let user_bytes = Self::serialize(user)?;

        self.users
            .transaction(move |tx| {
                // Check email uniqueness inside the transaction
                if tx.get(email_key.as_bytes())?.is_some() {
                    return sled::transaction::abort(());
                }
                tx.insert(email_key.as_bytes(), uid_bytes.as_slice())?;
                tx.insert(user_key.as_bytes(), user_bytes.as_slice())?;
                Ok(())
            })
            .map_err(|e: sled::transaction::TransactionError<()>| match e {
                sled::transaction::TransactionError::Abort(()) => StoreError::Conflict {
                    entity: "user_email".to_string(),
                    key: user.email.clone(),
                },
                sled::transaction::TransactionError::Storage(se) => StoreError::Sled(se),
            })?;

        // Maintain users_by_created_at index
        let idx_key = keys::users_by_created_at_key(
            user.created_at.timestamp_millis(),
            &user.id,
        )?;
        self.users_by_created_at
            .insert(idx_key.as_bytes(), user.id.as_bytes())?;

        Ok(())
    }

    pub fn get_user_by_id(&self, user_id: &str) -> Result<Option<User>, StoreError> {
        let key = keys::user_key(user_id)?;
        match self.users.get(key.as_bytes())? {
            Some(raw) => Ok(Some(Self::deserialize(&raw)?)),
            None => Ok(None),
        }
    }

    pub fn get_user_by_email(&self, email: &str) -> Result<Option<User>, StoreError> {
        let index_key = keys::user_email_index_key(email)?;
        let Some(user_id_raw) = self.users.get(index_key.as_bytes())? else {
            return Ok(None);
        };
        let user_id = match String::from_utf8(user_id_raw.to_vec()) {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!(error = %e, "Invalid UTF-8 in user email index");
                return Ok(None);
            }
        };
        self.get_user_by_id(&user_id)
    }

    /// 使用 CAS（Compare-And-Swap）更新用户，防止并发写入丢失。
    /// 邮箱变更时使用事务保证索引一致性。
    pub fn update_user(&self, user: &User) -> Result<(), StoreError> {
        let user_key = keys::user_key(&user.id)?;

        for _ in 0..MAX_CAS_RETRIES {
            let old_raw =
                self.users
                    .get(user_key.as_bytes())?
                    .ok_or_else(|| StoreError::NotFound {
                        entity: "user".to_string(),
                        key: user.id.clone(),
                    })?;
            let existing: User = Self::deserialize(&old_raw)?;
            let user_bytes = Self::serialize(user)?;

            if existing.email.to_lowercase() != user.email.to_lowercase() {
                // 邮箱变更：使用事务保证原子性
                let old_email_key = keys::user_email_index_key(&existing.email)?;
                let new_email_key = keys::user_email_index_key(&user.email)?;
                let uid_bytes = user.id.as_bytes().to_vec();
                let ub = user_bytes;
                let uk = user_key.clone();
                let old_raw_clone = old_raw.to_vec();
                self.users
                    .transaction(move |tx| {
                        // 先验证数据未被并发修改（CAS 语义）
                        let current = tx.get(uk.as_bytes())?;
                        if current.as_ref().map(|v| v.as_ref()) != Some(old_raw_clone.as_slice()) {
                            return sled::transaction::abort(());
                        }
                        // 检查新邮箱唯一性
                        if let Some(existing_uid) = tx.get(new_email_key.as_bytes())? {
                            if existing_uid.as_ref() != uid_bytes.as_slice() {
                                return sled::transaction::abort(());
                            }
                        }
                        tx.remove(old_email_key.as_bytes())?;
                        tx.insert(new_email_key.as_bytes(), uid_bytes.as_slice())?;
                        tx.insert(uk.as_bytes(), ub.as_slice())?;
                        Ok(())
                    })
                    .map_err(|e: sled::transaction::TransactionError<()>| match e {
                        sled::transaction::TransactionError::Abort(()) => StoreError::Conflict {
                            entity: "user_email".to_string(),
                            key: user.email.clone(),
                        },
                        sled::transaction::TransactionError::Storage(se) => StoreError::Sled(se),
                    })?;
                return Ok(());
            } else {
                // 邮箱未变更：使用 CAS 保护
                match self.users.compare_and_swap(
                    user_key.as_bytes(),
                    Some(old_raw),
                    Some(user_bytes),
                )? {
                    Ok(()) => return Ok(()),
                    Err(_) => continue, // 数据已被其他操作修改，重试
                }
            }
        }
        Err(StoreError::CasRetryExhausted {
            entity: "user".to_string(),
            key: user.id.clone(),
            attempts: MAX_CAS_RETRIES,
        })
    }

    /// 原子性地封禁用户，使用 compare_and_swap 避免竞态条件
    pub fn ban_user(&self, user_id: &str) -> Result<(), StoreError> {
        let user_key = keys::user_key(user_id)?;
        for _ in 0..MAX_CAS_RETRIES {
            let old_raw =
                self.users
                    .get(user_key.as_bytes())?
                    .ok_or_else(|| StoreError::NotFound {
                        entity: "user".to_string(),
                        key: user_id.to_string(),
                    })?;
            let mut user: User = Self::deserialize(&old_raw)?;
            if user.is_banned {
                return Ok(()); // 已封禁，幂等返回
            }
            user.is_banned = true;
            user.updated_at = Utc::now();
            let new_raw = Self::serialize(&user)?;
            match self
                .users
                .compare_and_swap(user_key.as_bytes(), Some(old_raw), Some(new_raw))?
            {
                Ok(()) => return Ok(()),
                Err(_) => continue, // 数据已被其他操作修改，重试
            }
        }
        Err(StoreError::CasRetryExhausted {
            entity: "user".to_string(),
            key: user_id.to_string(),
            attempts: MAX_CAS_RETRIES,
        })
    }

    /// 原子性地解封用户，使用 compare_and_swap 避免竞态条件
    pub fn unban_user(&self, user_id: &str) -> Result<(), StoreError> {
        let user_key = keys::user_key(user_id)?;
        for _ in 0..MAX_CAS_RETRIES {
            let old_raw =
                self.users
                    .get(user_key.as_bytes())?
                    .ok_or_else(|| StoreError::NotFound {
                        entity: "user".to_string(),
                        key: user_id.to_string(),
                    })?;
            let mut user: User = Self::deserialize(&old_raw)?;
            if !user.is_banned {
                return Ok(()); // 未封禁，幂等返回
            }
            user.is_banned = false;
            user.updated_at = Utc::now();
            let new_raw = Self::serialize(&user)?;
            match self
                .users
                .compare_and_swap(user_key.as_bytes(), Some(old_raw), Some(new_raw))?
            {
                Ok(()) => return Ok(()),
                Err(_) => continue, // 数据已被其他操作修改，重试
            }
        }
        Err(StoreError::CasRetryExhausted {
            entity: "user".to_string(),
            key: user_id.to_string(),
            attempts: MAX_CAS_RETRIES,
        })
    }

    /// 仅列出用户 ID，跳过 email 索引条目，避免不必要的用户对象反序列化和排序。
    pub fn list_user_ids(&self) -> Result<Vec<String>, StoreError> {
        let mut user_ids = Vec::new();
        for item in self.users.iter() {
            let (key, _) = item?;
            if key.starts_with(b"email:") {
                continue;
            }

            match String::from_utf8(key.to_vec()) {
                Ok(user_id) => user_ids.push(user_id),
                Err(e) => {
                    tracing::warn!(error = %e, "Invalid UTF-8 in user key while listing user IDs");
                }
            }
        }
        Ok(user_ids)
    }

    pub fn list_users(&self, limit: usize, offset: usize) -> Result<Vec<User>, StoreError> {
        // Use users_by_created_at index (reverse timestamp = newest first)
        if !self.users_by_created_at.is_empty() {
            let mut users = Vec::new();
            let mut skipped = 0usize;
            for item in self.users_by_created_at.iter() {
                let (_, value) = item?;
                let user_id = String::from_utf8(value.to_vec()).unwrap_or_default();
                if skipped < offset {
                    skipped += 1;
                    continue;
                }
                if let Some(user) = self.get_user_by_id(&user_id)? {
                    users.push(user);
                }
                if users.len() >= limit {
                    break;
                }
            }
            return Ok(users);
        }

        // Fallback: full scan (only if index not yet built)
        let mut users = Vec::new();
        for item in self.users.iter() {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);
            if key_str.starts_with("email:") {
                continue;
            }
            users.push(Self::deserialize::<User>(&value)?);
        }

        users.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(users.into_iter().skip(offset).take(limit).collect())
    }

    /// 记录一次登录失败，返回账户是否因此被锁定
    pub fn record_failed_login(&self, user_id: &str) -> Result<bool, StoreError> {
        let user_key = keys::user_key(user_id)?;
        for _ in 0..MAX_CAS_RETRIES {
            let old_raw =
                self.users
                    .get(user_key.as_bytes())?
                    .ok_or_else(|| StoreError::NotFound {
                        entity: "user".to_string(),
                        key: user_id.to_string(),
                    })?;
            let mut user: User = Self::deserialize(&old_raw)?;
            user.failed_login_count += 1;
            let locked = user.failed_login_count >= MAX_FAILED_LOGIN_ATTEMPTS;
            if locked {
                user.locked_until = Some(Utc::now() + Duration::minutes(LOCKOUT_DURATION_MINUTES));
            }
            user.updated_at = Utc::now();
            let new_raw = Self::serialize(&user)?;
            match self
                .users
                .compare_and_swap(user_key.as_bytes(), Some(old_raw), Some(new_raw))?
            {
                Ok(()) => return Ok(locked),
                Err(_) => continue,
            }
        }
        Err(StoreError::CasRetryExhausted {
            entity: "user".to_string(),
            key: user_id.to_string(),
            attempts: MAX_CAS_RETRIES,
        })
    }

    /// 重置登录失败计数（登录成功时调用）
    pub fn reset_login_attempts(&self, user_id: &str) -> Result<(), StoreError> {
        let user_key = keys::user_key(user_id)?;
        for _ in 0..MAX_CAS_RETRIES {
            let old_raw =
                self.users
                    .get(user_key.as_bytes())?
                    .ok_or_else(|| StoreError::NotFound {
                        entity: "user".to_string(),
                        key: user_id.to_string(),
                    })?;
            let mut user: User = Self::deserialize(&old_raw)?;
            if user.failed_login_count == 0 && user.locked_until.is_none() {
                return Ok(()); // 无需更新
            }
            user.failed_login_count = 0;
            user.locked_until = None;
            user.updated_at = Utc::now();
            let new_raw = Self::serialize(&user)?;
            match self
                .users
                .compare_and_swap(user_key.as_bytes(), Some(old_raw), Some(new_raw))?
            {
                Ok(()) => return Ok(()),
                Err(_) => continue,
            }
        }
        Err(StoreError::CasRetryExhausted {
            entity: "user".to_string(),
            key: user_id.to_string(),
            attempts: MAX_CAS_RETRIES,
        })
    }

    /// 检查账户是否处于锁定状态
    pub fn is_account_locked(&self, user_id: &str) -> Result<bool, StoreError> {
        let user = self
            .get_user_by_id(user_id)?
            .ok_or_else(|| StoreError::NotFound {
                entity: "user".to_string(),
                key: user_id.to_string(),
            })?;
        if let Some(locked_until) = user.locked_until {
            if locked_until > Utc::now() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// 删除用户及其所有关联数据。
    /// 跨多个 tree 清理用户数据（会话、学习记录、单词状态、单词本、引擎状态等）。
    /// 注意：由于 sled 事务仅支持同一组 tree 的原子操作，此方法采用尽力删除策略，
    /// 先删除认证相关数据（用户记录和邮箱索引使用事务保证原子性），
    /// 再逐步清理关联数据。如果中途失败，用户主记录已被删除，
    /// 残留的关联数据不会影响系统正确性（因为用户已不存在）。
    pub fn delete_user(&self, user_id: &str) -> Result<(), StoreError> {
        let user_key = keys::user_key(user_id)?;

        // 1. 原子删除用户记录和邮箱索引
        let user = self
            .get_user_by_id(user_id)?
            .ok_or_else(|| StoreError::NotFound {
                entity: "user".to_string(),
                key: user_id.to_string(),
            })?;
        let email_key = keys::user_email_index_key(&user.email)?;
        let uk = user_key.clone();
        let ek = email_key.clone();
        self.users
            .transaction(move |tx| {
                tx.remove(uk.as_bytes())?;
                tx.remove(ek.as_bytes())?;
                Ok(())
            })
            .map_err(|e: sled::transaction::TransactionError<()>| match e {
                sled::transaction::TransactionError::Abort(()) => {
                    StoreError::Sled(sled::Error::Unsupported("transaction aborted".into()))
                }
                sled::transaction::TransactionError::Storage(se) => StoreError::Sled(se),
            })?;

        // Clean up users_by_created_at index
        if let Ok(idx_key) = keys::users_by_created_at_key(
            user.created_at.timestamp_millis(),
            user_id,
        ) {
            let _ = self.users_by_created_at.remove(idx_key.as_bytes());
        }

        // Clean up user_stats
        if let Ok(stats_key) = keys::user_stats_key(user_id) {
            let _ = self.user_stats.remove(stats_key.as_bytes());
        }

        // 2. 删除用户会话
        if let Err(e) = self.delete_user_sessions(user_id) {
            tracing::warn!(user_id, error = %e, "删除用户会话失败");
        }

        // 3. 删除学习记录
        let record_prefix = keys::record_prefix(user_id)?;
        for (key, _) in self.records.scan_prefix(record_prefix.as_bytes()).flatten() {
            let _ = self.records.remove(&key);
        }

        // 4. 删除单词学习状态及到期索引
        let wls_prefix = keys::word_learning_state_prefix(user_id)?;
        for (key, value) in self.word_learning_states.scan_prefix(wls_prefix.as_bytes()).flatten() {
            let _ = self.word_learning_states.remove(&key);
            // 清理对应的 due index
            if let Ok(state) = Self::deserialize::<
                crate::store::operations::word_states::WordLearningState,
            >(&value)
            {
                if let Some(next_review_date) = state.next_review_date {
                    if let Ok(due_key) = keys::word_due_index_key(
                        user_id,
                        next_review_date.timestamp_millis(),
                        &state.word_id,
                    ) {
                        let _ = self.word_due_index.remove(due_key.as_bytes());
                    }
                }
            }
        }

        // 5. 删除学习配置
        if let Ok(config_key) = keys::study_config_key(user_id) {
            let _ = self.study_configs.remove(config_key.as_bytes());
        }

        // 6. 删除引擎用户状态
        if let Err(e) = self.delete_engine_user_state(user_id) {
            tracing::warn!(user_id, error = %e, "删除引擎用户状态失败");
        }

        // 7. 删除用户画像
        if let Ok(profile_key) = keys::user_profile_key(user_id) {
            let _ = self.user_profiles.remove(profile_key.as_bytes());
        }
        if let Ok(habit_key) = keys::habit_profile_key(user_id) {
            let _ = self.habit_profiles.remove(habit_key.as_bytes());
        }

        // 8. 删除通知
        let notif_prefix = keys::notification_prefix(user_id)?;
        for (key, _) in self.notifications.scan_prefix(notif_prefix.as_bytes()).flatten() {
            let _ = self.notifications.remove(&key);
        }

        // 9. 删除徽章
        let badge_prefix = keys::badge_prefix(user_id)?;
        for (key, _) in self.badges.scan_prefix(badge_prefix.as_bytes()).flatten() {
            let _ = self.badges.remove(&key);
        }

        // 10. 删除用户偏好设置
        if let Ok(pref_key) = keys::user_preferences_key(user_id) {
            let _ = self.user_preferences.remove(pref_key.as_bytes());
        }

        // 11. 删除学习会话索引
        let ls_prefix = keys::learning_session_user_index_prefix(user_id)?;
        for (key, _) in self.learning_sessions.scan_prefix(ls_prefix.as_bytes()).flatten() {
            let key_str = String::from_utf8(key.to_vec()).unwrap_or_default();
            if let Some(session_id) = key_str.rsplit(':').next() {
                if let Ok(sk) = keys::learning_session_key(session_id) {
                    let _ = self.learning_sessions.remove(sk.as_bytes());
                }
            }
            let _ = self.learning_sessions.remove(&key);
        }

        tracing::info!(user_id, "用户及关联数据已删除");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use tempfile::tempdir;

    use super::*;

    fn sample_user(id: &str, email: &str) -> User {
        User {
            id: id.to_string(),
            email: email.to_string(),
            username: "demo".to_string(),
            password_hash: "hash".to_string(),
            is_banned: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            failed_login_count: 0,
            locked_until: None,
        }
    }

    #[test]
    fn create_and_get_user() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("users-db");
        let store = Store::open(db_path.to_str().unwrap()).unwrap();

        let user = sample_user("u1", "u1@test.com");
        store.create_user(&user).unwrap();
        let got = store.get_user_by_id("u1").unwrap().unwrap();
        assert_eq!(got.email, "u1@test.com");
    }

    #[test]
    fn duplicate_email_conflicts() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("users-db2");
        let store = Store::open(db_path.to_str().unwrap()).unwrap();

        let u1 = sample_user("u1", "dup@test.com");
        let u2 = sample_user("u2", "dup@test.com");
        store.create_user(&u1).unwrap();
        let err = store.create_user(&u2).unwrap_err();
        assert!(matches!(err, StoreError::Conflict { .. }));
    }

    #[test]
    fn list_user_ids_ignores_email_index_entries() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("users-db3");
        let store = Store::open(db_path.to_str().unwrap()).unwrap();

        store
            .create_user(&sample_user("u1", "u1@test.com"))
            .unwrap();
        store
            .create_user(&sample_user("u2", "u2@test.com"))
            .unwrap();

        let mut ids = store.list_user_ids().unwrap();
        ids.sort();

        assert_eq!(ids, vec!["u1".to_string(), "u2".to_string()]);
    }
}
