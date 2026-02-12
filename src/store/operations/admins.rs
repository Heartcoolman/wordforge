use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::constants::{LOCKOUT_DURATION_MINUTES, MAX_CAS_RETRIES, MAX_FAILED_LOGIN_ATTEMPTS};
use crate::store::keys;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Admin {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub created_at: DateTime<Utc>,
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub failed_login_count: u32,
    #[serde(default)]
    pub locked_until: Option<DateTime<Utc>>,
}

impl Store {
    pub fn create_admin(&self, admin: &Admin) -> Result<(), StoreError> {
        let email_key = keys::admin_email_index_key(&admin.email)?;
        let admin_key = keys::admin_key(&admin.id)?;
        let aid_bytes = admin.id.as_bytes().to_vec();
        let admin_bytes = Self::serialize(admin)?;

        self.admins
            .transaction(move |tx| {
                // Check email uniqueness inside the transaction
                if tx.get(email_key.as_bytes())?.is_some() {
                    return sled::transaction::abort(());
                }
                tx.insert(email_key.as_bytes(), aid_bytes.as_slice())?;
                tx.insert(admin_key.as_bytes(), admin_bytes.as_slice())?;
                Ok(())
            })
            .map_err(|e: sled::transaction::TransactionError<()>| match e {
                sled::transaction::TransactionError::Abort(()) => StoreError::Conflict {
                    entity: "admin_email".to_string(),
                    key: admin.email.clone(),
                },
                sled::transaction::TransactionError::Storage(se) => StoreError::Sled(se),
            })?;

        Ok(())
    }

    /// 原子性创建首个管理员，防止 TOCTOU 竞态。
    /// 在单个事务中同时设置哨兵键、邮箱索引和管理员记录，
    /// 确保不会因中途失败导致状态不一致。
    pub fn create_first_admin(&self, admin: &Admin) -> Result<(), StoreError> {
        let sentinel_key = b"__initialized";
        let email_key = keys::admin_email_index_key(&admin.email)?;
        let admin_key = keys::admin_key(&admin.id)?;
        let aid_bytes = admin.id.as_bytes().to_vec();
        let admin_bytes = Self::serialize(admin)?;

        self.admins
            .transaction(move |tx| {
                // 检查哨兵键：如果已存在则说明已有管理员
                if tx.get(sentinel_key)?.is_some() {
                    return sled::transaction::abort(());
                }
                // 检查邮箱唯一性
                if tx.get(email_key.as_bytes())?.is_some() {
                    return sled::transaction::abort(());
                }
                // 原子性地同时写入哨兵、邮箱索引和管理员记录
                tx.insert(sentinel_key, b"true" as &[u8])?;
                tx.insert(email_key.as_bytes(), aid_bytes.as_slice())?;
                tx.insert(admin_key.as_bytes(), admin_bytes.as_slice())?;
                Ok(())
            })
            .map_err(|e: sled::transaction::TransactionError<()>| match e {
                sled::transaction::TransactionError::Abort(()) => StoreError::Conflict {
                    entity: "admin".to_string(),
                    key: "already_exists".to_string(),
                },
                sled::transaction::TransactionError::Storage(se) => StoreError::Sled(se),
            })?;

        Ok(())
    }

    pub fn get_admin_by_id(&self, admin_id: &str) -> Result<Option<Admin>, StoreError> {
        let key = keys::admin_key(admin_id)?;
        match self.admins.get(key.as_bytes())? {
            Some(raw) => Ok(Some(Self::deserialize(&raw)?)),
            None => Ok(None),
        }
    }

    pub fn get_admin_by_email(&self, email: &str) -> Result<Option<Admin>, StoreError> {
        let index_key = keys::admin_email_index_key(email)?;
        let Some(admin_id_raw) = self.admins.get(index_key.as_bytes())? else {
            return Ok(None);
        };
        let admin_id = match String::from_utf8(admin_id_raw.to_vec()) {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!(error = %e, "Invalid UTF-8 in admin email index");
                return Ok(None);
            }
        };
        self.get_admin_by_id(&admin_id)
    }

    pub fn any_admin_exists(&self) -> Result<bool, StoreError> {
        // 快速路径：检查哨兵键
        if self.admins.contains_key(b"__initialized")? {
            return Ok(true);
        }
        for item in self.admins.iter() {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);
            if !key_str.starts_with("email:") && key_str.as_ref() != "__initialized" {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// 记录一次管理员登录失败，返回账户是否因此被锁定
    pub fn record_admin_failed_login(&self, admin_id: &str) -> Result<bool, StoreError> {
        let admin_key = keys::admin_key(admin_id)?;
        for _ in 0..MAX_CAS_RETRIES {
            let old_raw =
                self.admins
                    .get(admin_key.as_bytes())?
                    .ok_or_else(|| StoreError::NotFound {
                        entity: "admin".to_string(),
                        key: admin_id.to_string(),
                    })?;
            let mut admin: Admin = Self::deserialize(&old_raw)?;
            admin.failed_login_count += 1;
            let locked = admin.failed_login_count >= MAX_FAILED_LOGIN_ATTEMPTS;
            if locked {
                admin.locked_until = Some(Utc::now() + Duration::minutes(LOCKOUT_DURATION_MINUTES));
            }
            admin.updated_at = Utc::now();
            let new_raw = Self::serialize(&admin)?;
            match self.admins.compare_and_swap(
                admin_key.as_bytes(),
                Some(old_raw),
                Some(new_raw),
            )? {
                Ok(()) => return Ok(locked),
                Err(_) => continue,
            }
        }
        Err(StoreError::CasRetryExhausted {
            entity: "admin".to_string(),
            key: admin_id.to_string(),
            attempts: MAX_CAS_RETRIES,
        })
    }

    /// 重置管理员登录失败计数（登录成功时调用）
    pub fn reset_admin_login_attempts(&self, admin_id: &str) -> Result<(), StoreError> {
        let admin_key = keys::admin_key(admin_id)?;
        for _ in 0..MAX_CAS_RETRIES {
            let old_raw =
                self.admins
                    .get(admin_key.as_bytes())?
                    .ok_or_else(|| StoreError::NotFound {
                        entity: "admin".to_string(),
                        key: admin_id.to_string(),
                    })?;
            let mut admin: Admin = Self::deserialize(&old_raw)?;
            if admin.failed_login_count == 0 && admin.locked_until.is_none() {
                return Ok(());
            }
            admin.failed_login_count = 0;
            admin.locked_until = None;
            admin.updated_at = Utc::now();
            let new_raw = Self::serialize(&admin)?;
            match self.admins.compare_and_swap(
                admin_key.as_bytes(),
                Some(old_raw),
                Some(new_raw),
            )? {
                Ok(()) => return Ok(()),
                Err(_) => continue,
            }
        }
        Err(StoreError::CasRetryExhausted {
            entity: "admin".to_string(),
            key: admin_id.to_string(),
            attempts: MAX_CAS_RETRIES,
        })
    }

    /// 检查管理员账户是否处于锁定状态
    pub fn is_admin_account_locked(&self, admin_id: &str) -> Result<bool, StoreError> {
        let admin = self
            .get_admin_by_id(admin_id)?
            .ok_or_else(|| StoreError::NotFound {
                entity: "admin".to_string(),
                key: admin_id.to_string(),
            })?;
        if let Some(locked_until) = admin.locked_until {
            if locked_until > Utc::now() {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
