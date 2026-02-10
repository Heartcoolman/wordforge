use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::store::keys;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Admin {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub created_at: DateTime<Utc>,
}

impl Store {
    pub fn create_admin(&self, admin: &Admin) -> Result<(), StoreError> {
        let email_key = keys::admin_email_index_key(&admin.email);
        let cas_result = self
            .admins
            .compare_and_swap(
                email_key.as_bytes(),
                None::<&[u8]>,
                Some(admin.id.as_bytes().to_vec()),
            )
            .map_err(StoreError::Sled)?;

        if let Err(_current_value) = cas_result {
            return Err(StoreError::Conflict {
                entity: "admin_email".to_string(),
                key: admin.email.clone(),
            });
        }

        let key = keys::admin_key(&admin.id);
        let admin_bytes = Self::serialize(admin)?;
        if let Err(e) = self.admins.insert(key.as_bytes(), admin_bytes) {
            let _ = self.admins.remove(email_key.as_bytes());
            return Err(StoreError::Sled(e));
        }
        Ok(())
    }

    pub fn get_admin_by_id(&self, admin_id: &str) -> Result<Option<Admin>, StoreError> {
        let key = keys::admin_key(admin_id);
        match self.admins.get(key.as_bytes())? {
            Some(raw) => Ok(Some(Self::deserialize(&raw)?)),
            None => Ok(None),
        }
    }

    pub fn get_admin_by_email(&self, email: &str) -> Result<Option<Admin>, StoreError> {
        let index_key = keys::admin_email_index_key(email);
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
        for item in self.admins.iter() {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);
            if !key_str.starts_with("email:") {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
