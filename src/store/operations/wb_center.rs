use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::store::keys;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordbookCenterImport {
    pub remote_id: String,
    pub local_wordbook_id: String,
    pub source_url: String,
    pub version: String,
    pub user_id: Option<String>,
    pub imported_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub word_count: u64,
}

pub fn source_url_hash_prefix(url: &str) -> String {
    let hash = Sha256::digest(url.as_bytes());
    hex::encode(&hash[..8])
}

impl Store {
    pub fn upsert_wb_center_import(
        &self,
        import: &WordbookCenterImport,
    ) -> Result<(), StoreError> {
        let prefix = source_url_hash_prefix(&import.source_url);
        let key = keys::wb_center_import_key(&prefix, &import.remote_id)?;
        self.wb_center_imports
            .insert(key.as_bytes(), Self::serialize(import)?)?;
        Ok(())
    }

    pub fn get_wb_center_import(
        &self,
        source_url: &str,
        remote_id: &str,
    ) -> Result<Option<WordbookCenterImport>, StoreError> {
        let prefix = source_url_hash_prefix(source_url);
        let key = keys::wb_center_import_key(&prefix, remote_id)?;
        match self.wb_center_imports.get(key.as_bytes())? {
            Some(raw) => Ok(Some(Self::deserialize(&raw)?)),
            None => Ok(None),
        }
    }

    pub fn list_wb_center_imports_by_source(
        &self,
        source_url: &str,
    ) -> Result<Vec<WordbookCenterImport>, StoreError> {
        let prefix = source_url_hash_prefix(source_url);
        let scan_prefix = keys::wb_center_import_prefix(&prefix)?;
        let mut imports = Vec::new();
        for item in self.wb_center_imports.scan_prefix(scan_prefix.as_bytes()) {
            let (_, v) = item?;
            imports.push(Self::deserialize(&v)?);
        }
        Ok(imports)
    }

    pub fn list_wb_center_imports_by_user(
        &self,
        user_id: Option<&str>,
    ) -> Result<Vec<WordbookCenterImport>, StoreError> {
        let mut imports = Vec::new();
        for item in self.wb_center_imports.iter() {
            let (_, v) = item?;
            let import: WordbookCenterImport = Self::deserialize(&v)?;
            if import.user_id.as_deref() == user_id {
                imports.push(import);
            }
        }
        Ok(imports)
    }

    pub fn delete_wb_center_import(
        &self,
        source_url: &str,
        remote_id: &str,
    ) -> Result<bool, StoreError> {
        let prefix = source_url_hash_prefix(source_url);
        let key = keys::wb_center_import_key(&prefix, remote_id)?;
        Ok(self.wb_center_imports.remove(key.as_bytes())?.is_some())
    }
}
