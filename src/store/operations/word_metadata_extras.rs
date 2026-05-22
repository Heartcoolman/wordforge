//! v1.1-P2.5：自 `extras.rs` 拆出的单词元数据与学习记录衍生统计访问。
//! 包含 etymology / confusion_pairs / word_morphemes / list_all_words(_with_tags) /
//! aggregate_records_since / daily_aggregation_stats / get_user_records_minimal。
use rusqlite::{params, OptionalExtension};

use crate::store::keys;
use crate::store::{Store, StoreError};

impl Store {
    // -- Etymologies (Routes + Workers) --

    pub fn get_etymology(&self, word_id: &str) -> Result<Option<serde_json::Value>, StoreError> {
        keys::validate_id(word_id)?;
        let conn = self.conn()?;
        conn.query_row(
            "SELECT word_id, word, etymology, roots_json, generated, source, generated_at
             FROM etymologies WHERE word_id=?1",
            params![word_id],
            |r| {
                Ok(serde_json::json!({
                    "word_id": r.get::<_, String>(0)?,
                    "word": r.get::<_, String>(1)?,
                    "etymology": r.get::<_, String>(2)?,
                    "roots": serde_json::from_str::<serde_json::Value>(&r.get::<_, String>(3)?).unwrap_or_default(),
                    "generated": r.get::<_, i64>(4)? != 0,
                    "source": r.get::<_, Option<String>>(5)?,
                    "generated_at": r.get::<_, Option<String>>(6)?,
                }))
            },
        )
        .optional()
        .map_err(StoreError::from)
    }

    pub fn set_etymology(&self, word_id: &str, data: &serde_json::Value) -> Result<(), StoreError> {
        keys::validate_id(word_id)?;
        let conn = self.conn()?;
        let word = data.get("word").and_then(|v| v.as_str()).unwrap_or("");
        let etymology = data.get("etymology").and_then(|v| v.as_str()).unwrap_or("");
        let roots = Self::serialize_json(&data.get("roots").unwrap_or(&serde_json::json!([])))?;
        let generated = data
            .get("generated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false) as i64;
        let source = data.get("source").and_then(|v| v.as_str());
        let generated_at = data.get("generated_at").and_then(|v| v.as_str());
        conn.execute(
            "INSERT INTO etymologies (word_id, word, etymology, roots_json, generated, source, generated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(word_id) DO UPDATE SET word=?2, etymology=?3, roots_json=?4, generated=?5, source=?6, generated_at=?7",
            params![word_id, word, etymology, roots, generated, source, generated_at],
        )?;
        Ok(())
    }

    pub fn delete_etymology(&self, word_id: &str) -> Result<(), StoreError> {
        keys::validate_id(word_id)?;
        let conn = self.conn()?;
        conn.execute("DELETE FROM etymologies WHERE word_id=?1", params![word_id])?;
        Ok(())
    }

    pub fn list_words_without_etymology(
        &self,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT w.id, w.text FROM words w
             LEFT JOIN etymologies e ON w.id = e.word_id
             WHERE e.word_id IS NULL LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, String>(0)?,
                "text": r.get::<_, String>(1)?,
            }))
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn list_all_words(&self) -> Result<Vec<serde_json::Value>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT id, text, difficulty FROM words")?;
        let rows = stmt.query_map([], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, String>(0)?,
                "text": r.get::<_, String>(1)?,
                "difficulty": r.get::<_, f64>(2)?,
            }))
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    // -- Confusion Pairs --

    pub fn get_confusion_pairs_for_word(
        &self,
        word_id: &str,
        limit: usize,
    ) -> Result<Vec<(String, f64)>, StoreError> {
        keys::validate_id(word_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT word_id_a, word_id_b, score FROM confusion_pairs
             WHERE word_id_a = ?1 OR word_id_b = ?1
             ORDER BY score DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![word_id, limit as i64], |r| {
            let a: String = r.get(0)?;
            let b: String = r.get(1)?;
            let score: f64 = r.get(2)?;
            let other = if a == word_id { b } else { a };
            Ok((other, score))
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    pub fn set_confusion_pair(
        &self,
        word_id_a: &str,
        word_id_b: &str,
        score: f64,
    ) -> Result<(), StoreError> {
        keys::validate_id(word_id_a)?;
        keys::validate_id(word_id_b)?;
        let (a, b) = keys::canonical_pair(word_id_a, word_id_b);
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO confusion_pairs (word_id_a, word_id_b, score, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(word_id_a, word_id_b) DO UPDATE SET score=?3, updated_at=?4",
            params![a, b, score, now],
        )?;
        Ok(())
    }

    // -- Word Morphemes --

    pub fn set_word_morphemes(
        &self,
        word_id: &str,
        morphemes: &[serde_json::Value],
    ) -> Result<(), StoreError> {
        keys::validate_id(word_id)?;
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM word_morphemes WHERE word_id=?1",
            params![word_id],
        )?;
        for (i, m) in morphemes.iter().enumerate() {
            let text = m.get("text").and_then(|v| v.as_str()).unwrap_or("");
            let mtype = m
                .get("type")
                .or(m.get("morpheme_type"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let meaning = m.get("meaning").and_then(|v| v.as_str()).unwrap_or("");
            tx.execute(
                "INSERT INTO word_morphemes (word_id, position, text, morpheme_type, meaning)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![word_id, i as i64, text, mtype, meaning],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_word_morphemes(
        &self,
        word_id: &str,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        keys::validate_id(word_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT text, morpheme_type, meaning FROM word_morphemes
             WHERE word_id=?1 ORDER BY position",
        )?;
        let rows: Vec<serde_json::Value> = stmt
            .query_map(params![word_id], |r| {
                Ok(serde_json::json!({
                    "text": r.get::<_, String>(0)?,
                    "type": r.get::<_, String>(1)?,
                    "meaning": r.get::<_, String>(2)?,
                }))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if rows.is_empty() {
            Ok(None)
        } else {
            Ok(Some(serde_json::Value::Array(rows)))
        }
    }

    // -- Records Aggregation --

    /// Aggregate (total_records, total_correct) since a given timestamp.
    pub fn aggregate_records_since(
        &self,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<(u64, u64), StoreError> {
        let conn = self.conn()?;
        let (total, correct): (i64, i64) = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(CASE WHEN is_correct=1 THEN 1 ELSE 0 END), 0)
             FROM learning_records WHERE created_at >= ?1",
            params![since.to_rfc3339()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok((total as u64, correct as u64))
    }

    /// Aggregate daily stats: (total, correct, unique_users, unique_words) for records on or after `since`.
    pub fn daily_aggregation_stats(
        &self,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<(u64, u64, u64, u64), StoreError> {
        let conn = self.conn()?;
        let since_str = since.to_rfc3339();
        let (total, correct): (i64, i64) = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(CASE WHEN is_correct=1 THEN 1 ELSE 0 END), 0)
             FROM learning_records WHERE created_at >= ?1",
            params![&since_str],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let unique_users: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT user_id) FROM learning_records WHERE created_at >= ?1",
            params![&since_str],
            |r| r.get(0),
        )?;
        let unique_words: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT word_id) FROM learning_records WHERE created_at >= ?1",
            params![&since_str],
            |r| r.get(0),
        )?;
        Ok((
            total as u64,
            correct as u64,
            unique_users as u64,
            unique_words as u64,
        ))
    }

    /// List all words with tags (for clustering).
    pub fn list_all_words_with_tags(&self) -> Result<Vec<(String, f64, Vec<String>)>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT id, difficulty, tags_json FROM words")?;
        let rows = stmt.query_map([], |r| {
            let id: String = r.get(0)?;
            let difficulty: f64 = r.get(1)?;
            let tags_json: String = r.get(2)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            Ok((id, difficulty, tags))
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// Get user records with minimal fields for confusion analysis.
    pub fn get_user_records_minimal(
        &self,
        user_id: &str,
        limit: usize,
    ) -> Result<Vec<(String, bool)>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT word_id, is_correct FROM learning_records
             WHERE user_id=?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![user_id, limit as i64], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? != 0))
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use serde_json::json;

    fn test_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let store = Store::open(path.to_str().unwrap(), 5000, 4).unwrap();
        (dir, store)
    }

    #[test]
    fn etymology_set_get_delete() {
        let (_t, store) = test_store();
        assert!(store.get_etymology("w1").unwrap().is_none());
        store
            .set_etymology(
                "w1",
                &json!({
                    "word":"foo","etymology":"old text",
                    "roots":[{"text":"f"}],
                    "generated":true,"source":"llm","generated_at":"2026-05-01T00:00:00Z"
                }),
            )
            .unwrap();
        let got = store.get_etymology("w1").unwrap().unwrap();
        assert_eq!(got["etymology"], json!("old text"));
        assert_eq!(got["generated"], json!(true));
        store.delete_etymology("w1").unwrap();
        assert!(store.get_etymology("w1").unwrap().is_none());
    }

    #[test]
    fn list_words_without_etymology_filters_existing_rows() {
        let (_t, store) = test_store();
        let conn = store.connection().unwrap();
        conn.execute(
            "INSERT INTO words (id, text, meaning, difficulty, created_at) VALUES ('w1','foo','m',0.5,?1)",
            params![Utc::now().to_rfc3339()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO words (id, text, meaning, difficulty, created_at) VALUES ('w2','bar','m',0.7,?1)",
            params![Utc::now().to_rfc3339()],
        )
        .unwrap();
        store
            .set_etymology("w1", &json!({"word":"foo","etymology":"e","roots":[]}))
            .unwrap();
        let missing = store.list_words_without_etymology(10).unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0]["id"], json!("w2"));

        let all = store.list_all_words().unwrap();
        assert_eq!(all.len(), 2);
        let with_tags = store.list_all_words_with_tags().unwrap();
        assert_eq!(with_tags.len(), 2);
    }

    #[test]
    fn confusion_pairs_canonical_ordering_and_lookup() {
        let (_t, store) = test_store();
        store.set_confusion_pair("w-b", "w-a", 0.5).unwrap();
        // upsert update
        store.set_confusion_pair("w-a", "w-b", 0.9).unwrap();
        let pairs = store.get_confusion_pairs_for_word("w-a", 10).unwrap();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, "w-b");
        assert!((pairs[0].1 - 0.9).abs() < 1e-9);
        // query other side
        let from_b = store.get_confusion_pairs_for_word("w-b", 10).unwrap();
        assert_eq!(from_b[0].0, "w-a");
    }

    #[test]
    fn set_word_morphemes_replaces_existing_rows() {
        let (_t, store) = test_store();
        store
            .set_word_morphemes(
                "w1",
                &[json!({"text":"pre","type":"prefix","meaning":"before"})],
            )
            .unwrap();
        store
            .set_word_morphemes(
                "w1",
                &[
                    json!({"text":"root","morpheme_type":"root","meaning":"core"}),
                    json!({"text":"suf","type":"suffix","meaning":"after"}),
                ],
            )
            .unwrap();
        let got = store.get_word_morphemes("w1").unwrap().unwrap();
        let arr = got.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["text"], json!("root"));
        assert_eq!(arr[1]["type"], json!("suffix"));
        assert!(store.get_word_morphemes("missing").unwrap().is_none());
    }

    #[test]
    fn aggregate_and_daily_stats_count_correctly() {
        let (_t, store) = test_store();
        let now = Utc::now();
        let conn = store.connection().unwrap();
        for (i, (uid, wid, ok)) in [("u1", "w1", true), ("u1", "w2", false), ("u2", "w1", true)]
            .iter()
            .enumerate()
        {
            conn.execute(
                "INSERT INTO learning_records (user_id, id, word_id, is_correct, response_time_ms, created_at)
                 VALUES (?1, ?2, ?3, ?4, 100, ?5)",
                params![uid, format!("r{i}"), wid, *ok as i64, now.to_rfc3339()],
            )
            .unwrap();
        }
        drop(conn);
        let (total, correct) = store
            .aggregate_records_since(now - Duration::seconds(1))
            .unwrap();
        assert_eq!((total, correct), (3, 2));

        let (t, c, uu, uw) = store
            .daily_aggregation_stats(now - Duration::seconds(1))
            .unwrap();
        assert_eq!((t, c, uu, uw), (3, 2, 2, 2));
    }

    #[test]
    fn get_user_records_minimal_returns_word_and_correct_flag() {
        let (_t, store) = test_store();
        let now = Utc::now();
        let conn = store.connection().unwrap();
        for i in 0..3 {
            conn.execute(
                "INSERT INTO learning_records (user_id, id, word_id, is_correct, response_time_ms, created_at)
                 VALUES ('u1', ?1, ?2, ?3, 100, ?4)",
                params![format!("r{i}"), format!("w{i}"), (i % 2) as i64, (now + Duration::seconds(i as i64)).to_rfc3339()],
            )
            .unwrap();
        }
        drop(conn);
        let mini = store.get_user_records_minimal("u1", 10).unwrap();
        assert_eq!(mini.len(), 3);
        // 最新的在前
        assert_eq!(mini[0].0, "w2");
    }
}
