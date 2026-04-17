use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::store::keys;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Word {
    pub id: String,
    pub text: String,
    pub meaning: String,
    pub pronunciation: Option<String>,
    pub part_of_speech: Option<String>,
    pub difficulty: f64,
    pub examples: Vec<String>,
    pub tags: Vec<String>,
    pub embedding: Option<Vec<f64>>,
    pub created_at: DateTime<Utc>,
}

fn word_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Word> {
    let created_at_str: String = row.get(9)?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(9, rusqlite::types::Type::Text, Box::new(e))
        })?;

    let examples_json: String = row.get(6)?;
    let tags_json: String = row.get(7)?;
    let embedding_json: Option<String> = row.get(8)?;

    Ok(Word {
        id: row.get(0)?,
        text: row.get(1)?,
        meaning: row.get(2)?,
        pronunciation: row.get(3)?,
        part_of_speech: row.get(4)?,
        difficulty: row.get(5)?,
        examples: serde_json::from_str(&examples_json).unwrap_or_default(),
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        embedding: embedding_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .unwrap_or_default(),
        created_at,
    })
}

const WORD_COLS: &str =
    "id, text, meaning, pronunciation, part_of_speech, difficulty, examples_json, tags_json, embedding_json, created_at";

impl Store {
    pub fn upsert_word(&self, word: &Word) -> Result<(), StoreError> {
        keys::validate_id(&word.id)?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO words (id, text, meaning, pronunciation, part_of_speech, difficulty, examples_json, tags_json, embedding_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                word.id,
                word.text,
                word.meaning,
                word.pronunciation,
                word.part_of_speech,
                word.difficulty,
                Self::serialize_json(&word.examples)?,
                Self::serialize_json(&word.tags)?,
                word.embedding.as_ref().map(|e| Self::serialize_json(e)).transpose()?,
                word.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_word(&self, word_id: &str) -> Result<Option<Word>, StoreError> {
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                &format!("SELECT {WORD_COLS} FROM words WHERE id = ?1"),
                params![word_id],
                word_from_row,
            )
            .optional()?)
    }

    pub fn get_words_by_ids(
        &self,
        word_ids: &[String],
    ) -> Result<HashMap<String, Word>, StoreError> {
        if word_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let conn = self.conn()?;
        let mut words = HashMap::with_capacity(word_ids.len());
        let placeholders: Vec<&str> = word_ids.iter().map(|_| "?").collect();
        let sql = format!(
            "SELECT {WORD_COLS} FROM words WHERE id IN ({})",
            placeholders.join(",")
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(word_ids.iter()), word_from_row)?;
        for row in rows {
            let word = row?;
            words.entry(word.id.clone()).or_insert(word);
        }
        Ok(words)
    }

    pub fn list_words(&self, limit: usize, offset: usize) -> Result<Vec<Word>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {WORD_COLS} FROM words ORDER BY created_at DESC LIMIT ?1 OFFSET ?2"
        ))?;
        let rows = stmt.query_map(params![limit as i64, offset as i64], word_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn delete_word(&self, word_id: &str) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute_batch("BEGIN")?;
        let result = (|| -> Result<(), StoreError> {
            // Decrement word_count before deleting wordbook_words rows
            conn.execute(
                "UPDATE wordbooks SET word_count = MAX(word_count - 1, 0)
                 WHERE id IN (SELECT wordbook_id FROM wordbook_words WHERE word_id = ?1)",
                params![word_id],
            )?;
            for table in &[
                "wordbook_words",
                "learning_records",
                "word_learning_states",
                "mastery_states",
                "word_elo",
                "etymologies",
                "word_morphemes",
                "alert_dedup",
            ] {
                conn.execute(
                    &format!("DELETE FROM {table} WHERE word_id = ?1"),
                    params![word_id],
                )?;
            }
            conn.execute(
                "DELETE FROM confusion_pairs WHERE word_id_a = ?1 OR word_id_b = ?1",
                params![word_id],
            )?;
            conn.execute("DELETE FROM words WHERE id = ?1", params![word_id])?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT")?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    pub fn count_words(&self) -> Result<u64, StoreError> {
        let conn = self.conn()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM words", [], |r| r.get(0))?;
        Ok(count as u64)
    }

    pub fn search_words(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<Word>, u64), StoreError> {
        let conn = self.conn()?;
        let pattern = format!("%{query}%");

        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM words WHERE text LIKE ?1 OR meaning LIKE ?1",
            params![pattern],
            |r| r.get(0),
        )?;

        let mut stmt = conn.prepare(&format!(
            "SELECT {WORD_COLS} FROM words WHERE text LIKE ?1 OR meaning LIKE ?1 ORDER BY text LIMIT ?2 OFFSET ?3"
        ))?;
        let rows = stmt.query_map(params![pattern, limit as i64, offset as i64], word_from_row)?;
        let words: Vec<Word> = rows.collect::<Result<Vec<_>, _>>()?;
        Ok((words, total as u64))
    }

    pub fn get_words_without_embedding(&self, limit: usize) -> Result<Vec<Word>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {WORD_COLS} FROM words WHERE embedding_json IS NULL ORDER BY created_at DESC LIMIT ?1"
        ))?;
        let rows = stmt.query_map(params![limit as i64], word_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> Store {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store
    }

    fn sample_word(id: &str, text: &str) -> Word {
        Word {
            id: id.to_string(),
            text: text.to_string(),
            meaning: "meaning".to_string(),
            pronunciation: None,
            part_of_speech: None,
            difficulty: 0.5,
            examples: vec!["ex".to_string()],
            tags: vec!["tag".to_string()],
            embedding: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn create_and_list_words() {
        let store = test_store();
        store.upsert_word(&sample_word("w1", "apple")).unwrap();
        store.upsert_word(&sample_word("w2", "banana")).unwrap();

        let list = store.list_words(10, 0).unwrap();
        assert_eq!(list.len(), 2);
        let texts: Vec<&str> = list.iter().map(|w| w.text.as_str()).collect();
        assert!(texts.contains(&"apple"));
        assert!(texts.contains(&"banana"));
    }

    #[test]
    fn get_words_by_ids_returns_existing_words_only() {
        let store = test_store();
        store.upsert_word(&sample_word("w1", "apple")).unwrap();
        store.upsert_word(&sample_word("w2", "banana")).unwrap();

        let words = store
            .get_words_by_ids(&[
                "w2".to_string(),
                "missing".to_string(),
                "w1".to_string(),
                "w1".to_string(),
            ])
            .unwrap();

        assert_eq!(words.len(), 2);
        assert!(words.contains_key("w1"));
        assert!(words.contains_key("w2"));
    }

    #[test]
    fn search_words_matches_text_and_meaning() {
        let store = test_store();
        let mut w = sample_word("w1", "apple");
        w.meaning = "a fruit".to_string();
        store.upsert_word(&w).unwrap();
        store.upsert_word(&sample_word("w2", "banana")).unwrap();

        let (results, total) = store.search_words("app", 10, 0).unwrap();
        assert_eq!(total, 1);
        assert_eq!(results[0].id, "w1");

        let (results, total) = store.search_words("fruit", 10, 0).unwrap();
        assert_eq!(total, 1);
        assert_eq!(results[0].id, "w1");
    }

    #[test]
    fn get_words_without_embedding_filters_correctly() {
        let store = test_store();
        store.upsert_word(&sample_word("w1", "apple")).unwrap();
        let mut w2 = sample_word("w2", "banana");
        w2.embedding = Some(vec![0.1, 0.2]);
        store.upsert_word(&w2).unwrap();

        let words = store.get_words_without_embedding(10).unwrap();
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].id, "w1");
    }

    #[test]
    fn upsert_word_overwrites_existing() {
        let store = test_store();
        store.upsert_word(&sample_word("w1", "apple")).unwrap();
        let mut updated = sample_word("w1", "apricot");
        updated.meaning = "updated".to_string();
        store.upsert_word(&updated).unwrap();

        let word = store.get_word("w1").unwrap().unwrap();
        assert_eq!(word.text, "apricot");
        assert_eq!(word.meaning, "updated");
        assert_eq!(store.count_words().unwrap(), 1);
    }

    #[test]
    fn delete_word_removes_word() {
        let store = test_store();
        store.upsert_word(&sample_word("w1", "apple")).unwrap();
        store.delete_word("w1").unwrap();
        assert!(store.get_word("w1").unwrap().is_none());
        assert_eq!(store.count_words().unwrap(), 0);
    }

    #[test]
    fn count_words_returns_correct_count() {
        let store = test_store();
        assert_eq!(store.count_words().unwrap(), 0);
        store.upsert_word(&sample_word("w1", "apple")).unwrap();
        store.upsert_word(&sample_word("w2", "banana")).unwrap();
        assert_eq!(store.count_words().unwrap(), 2);
    }

    #[test]
    fn list_words_with_offset() {
        let store = test_store();
        store.upsert_word(&sample_word("w1", "apple")).unwrap();
        store.upsert_word(&sample_word("w2", "banana")).unwrap();
        store.upsert_word(&sample_word("w3", "cherry")).unwrap();

        let list = store.list_words(2, 1).unwrap();
        assert_eq!(list.len(), 2);

        let list = store.list_words(10, 3).unwrap();
        assert!(list.is_empty());
    }
}
