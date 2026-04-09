use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::store::keys;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wordbook {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub book_type: WordbookType,
    pub user_id: Option<String>,
    pub word_count: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WordbookType {
    System,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordbookWordEntry {
    pub wordbook_id: String,
    pub word_id: String,
    pub added_at: DateTime<Utc>,
}

fn book_type_to_str(t: &WordbookType) -> &'static str {
    match t {
        WordbookType::System => "system",
        WordbookType::User => "user",
    }
}

fn book_type_from_str(s: &str) -> WordbookType {
    match s {
        "user" => WordbookType::User,
        _ => WordbookType::System,
    }
}

fn parse_dt(s: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
}

fn wordbook_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Wordbook> {
    Ok(Wordbook {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        book_type: book_type_from_str(&row.get::<_, String>(3)?),
        user_id: row.get(4)?,
        word_count: row.get::<_, i64>(5)? as u64,
        created_at: parse_dt(row.get(6)?)?,
    })
}

const WB_COLS: &str = "id, name, description, book_type, user_id, word_count, created_at";

impl Store {
    pub fn upsert_wordbook(&self, wordbook: &Wordbook) -> Result<(), StoreError> {
        keys::validate_id(&wordbook.id)?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO wordbooks (id, name, description, book_type, user_id, word_count, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               description = excluded.description,
               book_type = excluded.book_type,
               user_id = excluded.user_id,
               word_count = excluded.word_count",
            params![
                &wordbook.id,
                &wordbook.name,
                &wordbook.description,
                book_type_to_str(&wordbook.book_type),
                wordbook.user_id.as_deref(),
                wordbook.word_count as i64,
                wordbook.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_wordbook(&self, wordbook_id: &str) -> Result<Option<Wordbook>, StoreError> {
        keys::validate_id(wordbook_id)?;
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                &format!("SELECT {WB_COLS} FROM wordbooks WHERE id = ?1"),
                params![wordbook_id],
                wordbook_from_row,
            )
            .optional()?)
    }

    pub fn list_system_wordbooks(&self) -> Result<Vec<Wordbook>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            &format!("SELECT {WB_COLS} FROM wordbooks WHERE book_type = 'system' ORDER BY name"),
        )?;
        let books = stmt
            .query_map([], wordbook_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(books)
    }

    pub fn list_user_wordbooks(&self, user_id: &str) -> Result<Vec<Wordbook>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            &format!("SELECT {WB_COLS} FROM wordbooks WHERE book_type = 'user' AND user_id = ?1 ORDER BY created_at DESC"),
        )?;
        let books = stmt
            .query_map(params![user_id], wordbook_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(books)
    }

    pub fn add_word_to_wordbook(
        &self,
        wordbook_id: &str,
        word_id: &str,
    ) -> Result<bool, StoreError> {
        keys::validate_id(wordbook_id)?;
        keys::validate_id(word_id)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;

        let exists: bool = tx.query_row(
            "SELECT COUNT(*) FROM wordbooks WHERE id = ?1",
            params![wordbook_id],
            |r| r.get::<_, i64>(0),
        )? > 0;
        if !exists {
            return Err(StoreError::NotFound {
                entity: "wordbook".into(),
                key: wordbook_id.into(),
            });
        }

        let inserted = tx.execute(
            "INSERT OR IGNORE INTO wordbook_words (wordbook_id, word_id, added_at)
             VALUES (?1, ?2, ?3)",
            params![wordbook_id, word_id, Utc::now().to_rfc3339()],
        )?;

        if inserted > 0 {
            tx.execute(
                "UPDATE wordbooks SET word_count = word_count + 1 WHERE id = ?1",
                params![wordbook_id],
            )?;
        }

        tx.commit()?;
        Ok(inserted > 0)
    }

    pub fn remove_word_from_wordbook(
        &self,
        wordbook_id: &str,
        word_id: &str,
    ) -> Result<bool, StoreError> {
        keys::validate_id(wordbook_id)?;
        keys::validate_id(word_id)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;

        let removed = tx.execute(
            "DELETE FROM wordbook_words WHERE wordbook_id = ?1 AND word_id = ?2",
            params![wordbook_id, word_id],
        )?;

        if removed > 0 {
            tx.execute(
                "UPDATE wordbooks SET word_count = MAX(word_count - 1, 0) WHERE id = ?1",
                params![wordbook_id],
            )?;
        }

        tx.commit()?;
        Ok(removed > 0)
    }

    pub fn list_wordbook_words(
        &self,
        wordbook_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<String>, StoreError> {
        keys::validate_id(wordbook_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT word_id FROM wordbook_words WHERE wordbook_id = ?1 LIMIT ?2 OFFSET ?3",
        )?;
        let ids = stmt
            .query_map(params![wordbook_id, limit as i64, offset as i64], |r| r.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(ids)
    }

    pub fn count_wordbook_words(&self, wordbook_id: &str) -> Result<u64, StoreError> {
        keys::validate_id(wordbook_id)?;
        let conn = self.conn()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM wordbook_words WHERE wordbook_id = ?1",
            params![wordbook_id],
            |r| r.get(0),
        )?;
        Ok(count as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> Store {
        Store::open(":memory:", 5000, 1).unwrap()
    }

    fn sample_wordbook(id: &str, book_type: WordbookType, user_id: Option<&str>) -> Wordbook {
        Wordbook {
            id: id.into(),
            name: format!("Book {id}"),
            description: String::new(),
            book_type,
            user_id: user_id.map(|s| s.into()),
            word_count: 0,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn upsert_and_get() {
        let store = test_store();
        let wb = sample_wordbook("wb1", WordbookType::System, None);
        store.upsert_wordbook(&wb).unwrap();
        let got = store.get_wordbook("wb1").unwrap().unwrap();
        assert_eq!(got.name, "Book wb1");
        assert_eq!(got.book_type, WordbookType::System);
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let store = test_store();
        assert!(store.get_wordbook("nope").unwrap().is_none());
    }

    #[test]
    fn list_system_wordbooks_sorted_by_name() {
        let store = test_store();
        store.upsert_wordbook(&sample_wordbook("wb2", WordbookType::System, None)).unwrap();
        store.upsert_wordbook(&sample_wordbook("wb1", WordbookType::System, None)).unwrap();
        store.upsert_wordbook(&sample_wordbook("wb3", WordbookType::User, Some("u1"))).unwrap();
        let books = store.list_system_wordbooks().unwrap();
        assert_eq!(books.len(), 2);
        assert_eq!(books[0].id, "wb1");
    }

    #[test]
    fn list_user_wordbooks_filters_by_user() {
        let store = test_store();
        store.upsert_wordbook(&sample_wordbook("wb1", WordbookType::User, Some("u1"))).unwrap();
        store.upsert_wordbook(&sample_wordbook("wb2", WordbookType::User, Some("u2"))).unwrap();
        let books = store.list_user_wordbooks("u1").unwrap();
        assert_eq!(books.len(), 1);
        assert_eq!(books[0].id, "wb1");
    }

    #[test]
    fn add_and_remove_word() {
        let store = test_store();
        store.upsert_wordbook(&sample_wordbook("wb1", WordbookType::System, None)).unwrap();
        assert!(store.add_word_to_wordbook("wb1", "w1").unwrap());
        assert!(!store.add_word_to_wordbook("wb1", "w1").unwrap()); // duplicate
        assert_eq!(store.get_wordbook("wb1").unwrap().unwrap().word_count, 1);

        assert!(store.remove_word_from_wordbook("wb1", "w1").unwrap());
        assert!(!store.remove_word_from_wordbook("wb1", "w1").unwrap());
        assert_eq!(store.get_wordbook("wb1").unwrap().unwrap().word_count, 0);
    }

    #[test]
    fn add_word_to_nonexistent_wordbook_fails() {
        let store = test_store();
        let err = store.add_word_to_wordbook("nope", "w1").unwrap_err();
        assert!(matches!(err, StoreError::NotFound { .. }));
    }

    #[test]
    fn list_and_count_wordbook_words() {
        let store = test_store();
        store.upsert_wordbook(&sample_wordbook("wb1", WordbookType::System, None)).unwrap();
        store.add_word_to_wordbook("wb1", "w1").unwrap();
        store.add_word_to_wordbook("wb1", "w2").unwrap();
        store.add_word_to_wordbook("wb1", "w3").unwrap();
        assert_eq!(store.count_wordbook_words("wb1").unwrap(), 3);

        let page = store.list_wordbook_words("wb1", 2, 0).unwrap();
        assert_eq!(page.len(), 2);
        let page2 = store.list_wordbook_words("wb1", 2, 2).unwrap();
        assert_eq!(page2.len(), 1);
    }
}
