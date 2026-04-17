use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

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

fn parse_dt(s: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

fn import_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WordbookCenterImport> {
    Ok(WordbookCenterImport {
        remote_id: row.get(0)?,
        local_wordbook_id: row.get(1)?,
        source_url: row.get(2)?,
        version: row.get(3)?,
        user_id: row.get(4)?,
        imported_at: parse_dt(row.get(5)?)?,
        updated_at: parse_dt(row.get(6)?)?,
        word_count: row.get::<_, i64>(7)? as u64,
    })
}

const COLS: &str =
    "remote_id, local_wordbook_id, source_url, version, user_id, imported_at, updated_at, word_count";

impl Store {
    pub fn upsert_wb_center_import(&self, import: &WordbookCenterImport) -> Result<(), StoreError> {
        let prefix = keys::source_url_hash_prefix(&import.source_url);
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO wb_center_imports
                (source_url_hash_prefix, remote_id, local_wordbook_id, source_url, version, user_id, imported_at, updated_at, word_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(source_url_hash_prefix, remote_id) DO UPDATE SET
               local_wordbook_id = excluded.local_wordbook_id,
               source_url = excluded.source_url,
               version = excluded.version,
               user_id = excluded.user_id,
               updated_at = excluded.updated_at,
               word_count = excluded.word_count",
            params![
                &prefix,
                &import.remote_id,
                &import.local_wordbook_id,
                &import.source_url,
                &import.version,
                import.user_id.as_deref(),
                import.imported_at.to_rfc3339(),
                import.updated_at.to_rfc3339(),
                import.word_count as i64,
            ],
        )?;
        Ok(())
    }

    pub fn get_wb_center_import(
        &self,
        source_url: &str,
        remote_id: &str,
    ) -> Result<Option<WordbookCenterImport>, StoreError> {
        let prefix = keys::source_url_hash_prefix(source_url);
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                &format!("SELECT {COLS} FROM wb_center_imports WHERE source_url_hash_prefix = ?1 AND remote_id = ?2"),
                params![&prefix, remote_id],
                import_from_row,
            )
            .optional()?)
    }

    pub fn list_wb_center_imports_by_source(
        &self,
        source_url: &str,
    ) -> Result<Vec<WordbookCenterImport>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLS} FROM wb_center_imports WHERE source_url = ?1"
        ))?;
        let imports = stmt
            .query_map(params![source_url], import_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(imports)
    }

    pub fn list_wb_center_imports_by_user(
        &self,
        user_id: Option<&str>,
    ) -> Result<Vec<WordbookCenterImport>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = match user_id {
            Some(_) => conn.prepare(
                &format!("SELECT {COLS} FROM wb_center_imports WHERE user_id = ?1 ORDER BY updated_at DESC"),
            )?,
            None => conn.prepare(
                &format!("SELECT {COLS} FROM wb_center_imports WHERE user_id IS NULL ORDER BY updated_at DESC"),
            )?,
        };
        let imports = match user_id {
            Some(uid) => stmt.query_map(params![uid], import_from_row)?,
            None => stmt.query_map([], import_from_row)?,
        };
        Ok(imports.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn delete_wb_center_import(
        &self,
        source_url: &str,
        remote_id: &str,
    ) -> Result<bool, StoreError> {
        let prefix = keys::source_url_hash_prefix(source_url);
        let conn = self.conn()?;
        let deleted = conn.execute(
            "DELETE FROM wb_center_imports WHERE source_url_hash_prefix = ?1 AND remote_id = ?2",
            params![&prefix, remote_id],
        )?;
        Ok(deleted > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> Store {
        Store::open(":memory:", 5000, 1).unwrap()
    }

    fn sample_import(
        remote_id: &str,
        source_url: &str,
        user_id: Option<&str>,
    ) -> WordbookCenterImport {
        WordbookCenterImport {
            remote_id: remote_id.into(),
            local_wordbook_id: format!("local_{remote_id}"),
            source_url: source_url.into(),
            version: "1.0".into(),
            user_id: user_id.map(|s| s.into()),
            imported_at: Utc::now(),
            updated_at: Utc::now(),
            word_count: 10,
        }
    }

    #[test]
    fn upsert_and_get() {
        let store = test_store();
        let imp = sample_import("r1", "https://example.com", None);
        store.upsert_wb_center_import(&imp).unwrap();
        let got = store
            .get_wb_center_import("https://example.com", "r1")
            .unwrap()
            .unwrap();
        assert_eq!(got.local_wordbook_id, "local_r1");
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let store = test_store();
        assert!(store
            .get_wb_center_import("https://x.com", "nope")
            .unwrap()
            .is_none());
    }

    #[test]
    fn list_by_source() {
        let store = test_store();
        let url = "https://example.com";
        store
            .upsert_wb_center_import(&sample_import("r1", url, None))
            .unwrap();
        store
            .upsert_wb_center_import(&sample_import("r2", url, None))
            .unwrap();
        store
            .upsert_wb_center_import(&sample_import("r3", "https://other.com", None))
            .unwrap();
        let imports = store.list_wb_center_imports_by_source(url).unwrap();
        assert_eq!(imports.len(), 2);
    }

    #[test]
    fn list_by_user_none() {
        let store = test_store();
        store
            .upsert_wb_center_import(&sample_import("r1", "https://a.com", None))
            .unwrap();
        store
            .upsert_wb_center_import(&sample_import("r2", "https://b.com", Some("u1")))
            .unwrap();
        let imports = store.list_wb_center_imports_by_user(None).unwrap();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].remote_id, "r1");
    }

    #[test]
    fn list_by_user_some() {
        let store = test_store();
        store
            .upsert_wb_center_import(&sample_import("r1", "https://a.com", Some("u1")))
            .unwrap();
        store
            .upsert_wb_center_import(&sample_import("r2", "https://b.com", Some("u2")))
            .unwrap();
        let imports = store.list_wb_center_imports_by_user(Some("u1")).unwrap();
        assert_eq!(imports.len(), 1);
    }

    #[test]
    fn delete() {
        let store = test_store();
        let url = "https://example.com";
        store
            .upsert_wb_center_import(&sample_import("r1", url, None))
            .unwrap();
        assert!(store.delete_wb_center_import(url, "r1").unwrap());
        assert!(!store.delete_wb_center_import(url, "r1").unwrap());
        assert!(store.get_wb_center_import(url, "r1").unwrap().is_none());
    }

    #[test]
    fn upsert_updates_existing() {
        let store = test_store();
        let url = "https://example.com";
        let mut imp = sample_import("r1", url, None);
        store.upsert_wb_center_import(&imp).unwrap();
        imp.version = "2.0".into();
        imp.word_count = 20;
        store.upsert_wb_center_import(&imp).unwrap();
        let got = store.get_wb_center_import(url, "r1").unwrap().unwrap();
        assert_eq!(got.version, "2.0");
        assert_eq!(got.word_count, 20);
    }
}
