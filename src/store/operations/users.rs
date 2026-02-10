use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::store::keys;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub email: String,
    pub username: String,
    pub password_hash: String,
    pub is_banned: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Store {
    pub fn create_user(&self, user: &User) -> Result<(), StoreError> {
        let email_key = keys::user_email_index_key(&user.email);

        // Atomic compare-and-swap: only insert if the email key does not exist.
        // This prevents the race condition where two concurrent registrations
        // with the same email both pass the existence check.
        let cas_result = self
            .users
            .compare_and_swap(
                email_key.as_bytes(),
                None::<&[u8]>,                   // expected: key does not exist
                Some(user.id.as_bytes().to_vec()), // new value
            )
            .map_err(StoreError::Sled)?;

        if let Err(_current_value) = cas_result {
            return Err(StoreError::Conflict {
                entity: "user_email".to_string(),
                key: user.email.clone(),
            });
        }

        let user_key = keys::user_key(&user.id);
        let user_bytes = Self::serialize(user)?;
        if let Err(e) = self.users.insert(user_key.as_bytes(), user_bytes) {
            let _ = self.users.remove(email_key.as_bytes());
            return Err(StoreError::Sled(e));
        }

        Ok(())
    }

    pub fn get_user_by_id(&self, user_id: &str) -> Result<Option<User>, StoreError> {
        let key = keys::user_key(user_id);
        match self.users.get(key.as_bytes())? {
            Some(raw) => Ok(Some(Self::deserialize(&raw)?)),
            None => Ok(None),
        }
    }

    pub fn get_user_by_email(&self, email: &str) -> Result<Option<User>, StoreError> {
        let index_key = keys::user_email_index_key(email);
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

    pub fn update_user(&self, user: &User) -> Result<(), StoreError> {
        let existing = self
            .get_user_by_id(&user.id)?
            .ok_or_else(|| StoreError::NotFound {
                entity: "user".to_string(),
                key: user.id.clone(),
            })?;

        let user_bytes = Self::serialize(user)?;
        let user_key = keys::user_key(&user.id);

        if existing.email.to_lowercase() != user.email.to_lowercase() {
            let old_email_key = keys::user_email_index_key(&existing.email);
            let new_email_key = keys::user_email_index_key(&user.email);
            let uid_bytes = user.id.as_bytes().to_vec();
            let ub = user_bytes.clone();
            let uk = user_key.clone();
            self.users
                .transaction(move |tx| {
                    // Check inside the transaction that the new email isn't already taken
                    if let Some(existing_uid) = tx.get(new_email_key.as_bytes())? {
                        // Allow if the index already points to this same user (idempotent)
                        if existing_uid.as_ref() != uid_bytes.as_slice() {
                            return sled::transaction::abort(());
                        }
                    }
                    tx.remove(old_email_key.as_bytes())?;
                    tx.insert(new_email_key.as_bytes(), uid_bytes.as_slice())?;
                    tx.insert(uk.as_bytes(), ub.as_slice())?;
                    Ok(())
                })
                .map_err(
                    |e: sled::transaction::TransactionError<()>| match e {
                        sled::transaction::TransactionError::Abort(()) => {
                            StoreError::Conflict {
                                entity: "user_email".to_string(),
                                key: user.email.clone(),
                            }
                        }
                        sled::transaction::TransactionError::Storage(se) => {
                            StoreError::Sled(se)
                        }
                    },
                )?;
        } else {
            self.users.insert(user_key.as_bytes(), user_bytes)?;
        }

        Ok(())
    }

    pub fn ban_user(&self, user_id: &str) -> Result<(), StoreError> {
        let mut user = self
            .get_user_by_id(user_id)?
            .ok_or_else(|| StoreError::NotFound {
                entity: "user".to_string(),
                key: user_id.to_string(),
            })?;
        user.is_banned = true;
        user.updated_at = Utc::now();
        self.update_user(&user)
    }

    pub fn unban_user(&self, user_id: &str) -> Result<(), StoreError> {
        let mut user = self
            .get_user_by_id(user_id)?
            .ok_or_else(|| StoreError::NotFound {
                entity: "user".to_string(),
                key: user_id.to_string(),
            })?;
        user.is_banned = false;
        user.updated_at = Utc::now();
        self.update_user(&user)
    }

    pub fn list_users(&self, limit: usize, offset: usize) -> Result<Vec<User>, StoreError> {
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
}
