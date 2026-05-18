use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::store::keys;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordFavorite {
    pub user_id: String,
    pub word_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordNote {
    pub user_id: String,
    pub id: String,
    pub word_id: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const FAVORITE_COLS: &str = "user_id, word_id, created_at";
const NOTE_COLS: &str = "user_id, id, word_id, content, created_at, updated_at";

fn parse_dt(s: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

fn favorite_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WordFavorite> {
    Ok(WordFavorite {
        user_id: row.get(0)?,
        word_id: row.get(1)?,
        created_at: parse_dt(row.get(2)?)?,
    })
}

fn note_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WordNote> {
    Ok(WordNote {
        user_id: row.get(0)?,
        id: row.get(1)?,
        word_id: row.get(2)?,
        content: row.get(3)?,
        created_at: parse_dt(row.get(4)?)?,
        updated_at: parse_dt(row.get(5)?)?,
    })
}

impl Store {
    pub fn upsert_word_favorite(
        &self,
        user_id: &str,
        word_id: &str,
    ) -> Result<WordFavorite, StoreError> {
        keys::validate_id(user_id)?;
        keys::validate_id(word_id)?;
        let now = Utc::now();
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR IGNORE INTO word_favorites (user_id, word_id, created_at)
             VALUES (?1, ?2, ?3)",
            params![user_id, word_id, now.to_rfc3339()],
        )?;
        conn.query_row(
            &format!("SELECT {FAVORITE_COLS} FROM word_favorites WHERE user_id=?1 AND word_id=?2"),
            params![user_id, word_id],
            favorite_from_row,
        )
        .map_err(StoreError::from)
    }

    pub fn delete_word_favorite(&self, user_id: &str, word_id: &str) -> Result<bool, StoreError> {
        keys::validate_id(user_id)?;
        keys::validate_id(word_id)?;
        let conn = self.conn()?;
        let deleted = conn.execute(
            "DELETE FROM word_favorites WHERE user_id=?1 AND word_id=?2",
            params![user_id, word_id],
        )?;
        Ok(deleted > 0)
    }

    pub fn list_word_favorites(
        &self,
        user_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<WordFavorite>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {FAVORITE_COLS} FROM word_favorites
             WHERE user_id=?1
             ORDER BY created_at DESC
             LIMIT ?2 OFFSET ?3"
        ))?;
        let rows = stmt.query_map(
            params![user_id, limit as i64, offset as i64],
            favorite_from_row,
        )?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn count_word_favorites(&self, user_id: &str) -> Result<u64, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM word_favorites WHERE user_id=?1",
            params![user_id],
            |r| r.get(0),
        )?;
        Ok(count as u64)
    }

    pub fn get_word_favorite_statuses(
        &self,
        user_id: &str,
        word_ids: &[String],
    ) -> Result<HashMap<String, WordFavorite>, StoreError> {
        keys::validate_id(user_id)?;
        if word_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let conn = self.conn()?;
        let placeholders: Vec<&str> = word_ids.iter().map(|_| "?").collect();
        let sql = format!(
            "SELECT {FAVORITE_COLS} FROM word_favorites
             WHERE user_id=? AND word_id IN ({})",
            placeholders.join(",")
        );
        let mut values: Vec<&dyn rusqlite::types::ToSql> = Vec::with_capacity(word_ids.len() + 1);
        values.push(&user_id);
        for word_id in word_ids {
            values.push(word_id);
        }
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(values.as_slice(), favorite_from_row)?;
        let mut out = HashMap::new();
        for row in rows {
            let favorite = row?;
            out.insert(favorite.word_id.clone(), favorite);
        }
        Ok(out)
    }

    pub fn create_word_note(
        &self,
        user_id: &str,
        word_id: &str,
        content: &str,
    ) -> Result<WordNote, StoreError> {
        keys::validate_id(user_id)?;
        keys::validate_id(word_id)?;
        let now = Utc::now();
        let note = WordNote {
            user_id: user_id.to_string(),
            id: uuid::Uuid::new_v4().to_string(),
            word_id: word_id.to_string(),
            content: content.to_string(),
            created_at: now,
            updated_at: now,
        };
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO word_notes (user_id, id, word_id, content, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &note.user_id,
                &note.id,
                &note.word_id,
                &note.content,
                note.created_at.to_rfc3339(),
                note.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(note)
    }

    pub fn list_word_notes(
        &self,
        user_id: &str,
        word_id: Option<&str>,
    ) -> Result<Vec<WordNote>, StoreError> {
        keys::validate_id(user_id)?;
        if let Some(word_id) = word_id {
            keys::validate_id(word_id)?;
        }
        let conn = self.conn()?;
        let notes = match word_id {
            Some(word_id) => {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {NOTE_COLS} FROM word_notes
                     WHERE user_id=?1 AND word_id=?2
                     ORDER BY updated_at DESC"
                ))?;
                let rows = stmt.query_map(params![user_id, word_id], note_from_row)?;
                rows.collect::<Result<Vec<_>, _>>()?
            }
            None => {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {NOTE_COLS} FROM word_notes
                     WHERE user_id=?1
                     ORDER BY updated_at DESC"
                ))?;
                let rows = stmt.query_map(params![user_id], note_from_row)?;
                rows.collect::<Result<Vec<_>, _>>()?
            }
        };
        Ok(notes)
    }

    pub fn get_word_note(
        &self,
        user_id: &str,
        note_id: &str,
    ) -> Result<Option<WordNote>, StoreError> {
        keys::validate_id(user_id)?;
        keys::validate_id(note_id)?;
        let conn = self.conn()?;
        conn.query_row(
            &format!("SELECT {NOTE_COLS} FROM word_notes WHERE user_id=?1 AND id=?2"),
            params![user_id, note_id],
            note_from_row,
        )
        .optional()
        .map_err(StoreError::from)
    }

    pub fn update_word_note(
        &self,
        user_id: &str,
        note_id: &str,
        content: &str,
    ) -> Result<Option<WordNote>, StoreError> {
        keys::validate_id(user_id)?;
        keys::validate_id(note_id)?;
        let now = Utc::now();
        let conn = self.conn()?;
        let updated = conn.execute(
            "UPDATE word_notes SET content=?1, updated_at=?2 WHERE user_id=?3 AND id=?4",
            params![content, now.to_rfc3339(), user_id, note_id],
        )?;
        if updated == 0 {
            return Ok(None);
        }
        self.get_word_note(user_id, note_id)
    }

    pub fn delete_word_note(&self, user_id: &str, note_id: &str) -> Result<bool, StoreError> {
        keys::validate_id(user_id)?;
        keys::validate_id(note_id)?;
        let conn = self.conn()?;
        let deleted = conn.execute(
            "DELETE FROM word_notes WHERE user_id=?1 AND id=?2",
            params![user_id, note_id],
        )?;
        Ok(deleted > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let store = Store::open(path.to_str().unwrap(), 5000, 4).unwrap();
        (dir, store)
    }

    fn user() -> String {
        format!("user-{}", uuid::Uuid::new_v4())
    }

    fn word() -> String {
        format!("word-{}", uuid::Uuid::new_v4())
    }

    #[test]
    fn favorite_upsert_idempotent_and_returns_current_row() {
        let store = test_store();
        let (u, w) = (user(), word());
        let f1 = store.upsert_word_favorite(&u, &w).unwrap();
        let f2 = store.upsert_word_favorite(&u, &w).unwrap();
        assert_eq!(f1.user_id, f2.user_id);
        assert_eq!(f1.word_id, f2.word_id);
        // INSERT OR IGNORE => created_at 维持原值
        assert_eq!(f1.created_at, f2.created_at);
    }

    #[test]
    fn favorite_delete_returns_true_then_false() {
        let store = test_store();
        let (u, w) = (user(), word());
        store.upsert_word_favorite(&u, &w).unwrap();
        assert!(store.delete_word_favorite(&u, &w).unwrap());
        assert!(!store.delete_word_favorite(&u, &w).unwrap());
    }

    #[test]
    fn favorite_list_pagination_and_count() {
        let store = test_store();
        let u = user();
        for _ in 0..3 {
            store.upsert_word_favorite(&u, &word()).unwrap();
        }
        let all = store.list_word_favorites(&u, 10, 0).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(store.count_word_favorites(&u).unwrap(), 3);
        let page = store.list_word_favorites(&u, 2, 0).unwrap();
        assert_eq!(page.len(), 2);
        let page2 = store.list_word_favorites(&u, 2, 2).unwrap();
        assert_eq!(page2.len(), 1);
    }

    #[test]
    fn favorite_statuses_empty_input_short_circuits() {
        let store = test_store();
        let u = user();
        let m = store.get_word_favorite_statuses(&u, &[]).unwrap();
        assert!(m.is_empty());
    }

    #[test]
    fn favorite_statuses_only_returns_existing() {
        let store = test_store();
        let u = user();
        let w1 = word();
        let w2 = word();
        store.upsert_word_favorite(&u, &w1).unwrap();
        let missing = word();
        let m = store.get_word_favorite_statuses(&u, &[w1.clone(), w2.clone(), missing.clone()]).unwrap();
        assert!(m.contains_key(&w1));
        assert!(!m.contains_key(&w2));
        assert!(!m.contains_key(&missing));
    }

    #[test]
    fn note_create_get_update_delete_roundtrip() {
        let store = test_store();
        let (u, w) = (user(), word());
        let note = store.create_word_note(&u, &w, "initial").unwrap();
        assert_eq!(note.content, "initial");

        let got = store.get_word_note(&u, &note.id).unwrap().unwrap();
        assert_eq!(got.content, "initial");

        let updated = store.update_word_note(&u, &note.id, "edited").unwrap().unwrap();
        assert_eq!(updated.content, "edited");

        assert!(store.delete_word_note(&u, &note.id).unwrap());
        assert!(store.get_word_note(&u, &note.id).unwrap().is_none());
        assert!(!store.delete_word_note(&u, &note.id).unwrap());
    }

    #[test]
    fn note_list_filters_by_word_when_provided() {
        let store = test_store();
        let u = user();
        let w1 = word();
        let w2 = word();
        store.create_word_note(&u, &w1, "a").unwrap();
        store.create_word_note(&u, &w1, "b").unwrap();
        store.create_word_note(&u, &w2, "c").unwrap();
        let all = store.list_word_notes(&u, None).unwrap();
        assert_eq!(all.len(), 3);
        let only_w1 = store.list_word_notes(&u, Some(&w1)).unwrap();
        assert_eq!(only_w1.len(), 2);
    }

    #[test]
    fn note_update_missing_returns_none() {
        let store = test_store();
        let u = user();
        let missing = format!("note-{}", uuid::Uuid::new_v4());
        assert!(store.update_word_note(&u, &missing, "x").unwrap().is_none());
    }

    #[test]
    fn favorite_validation_rejects_bad_id() {
        let store = test_store();
        let err = store.upsert_word_favorite("", "wid").unwrap_err();
        assert!(matches!(err, StoreError::Validation(_)));
    }

    #[test]
    fn note_validation_rejects_bad_id() {
        let store = test_store();
        let err = store.create_word_note("", "wid", "c").unwrap_err();
        assert!(matches!(err, StoreError::Validation(_)));
    }
}
