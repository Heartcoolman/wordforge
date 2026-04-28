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
