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
    pub created_at: DateTime<Utc>,
}

fn word_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Word> {
    let created_at_str: String = row.get(8)?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(e))
        })?;

    let examples_json: String = row.get(6)?;
    let tags_json: String = row.get(7)?;

    Ok(Word {
        id: row.get(0)?,
        text: row.get(1)?,
        meaning: row.get(2)?,
        pronunciation: row.get(3)?,
        part_of_speech: row.get(4)?,
        difficulty: row.get(5)?,
        examples: serde_json::from_str(&examples_json).unwrap_or_default(),
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        created_at,
    })
}

const WORD_COLS: &str =
    "id, text, meaning, pronunciation, part_of_speech, difficulty, examples_json, tags_json, created_at";

impl Store {
    pub fn upsert_word(&self, word: &Word) -> Result<(), StoreError> {
        let conn = self.conn()?;
        Self::upsert_word_conn(&conn, word)
    }

    /// 在调用方提供的连接（可处于事务中）上执行 upsert，供 read-modify-write 单事务复用。
    pub fn upsert_word_conn(conn: &rusqlite::Connection, word: &Word) -> Result<(), StoreError> {
        keys::validate_id(&word.id)?;
        conn.execute(
            "INSERT OR REPLACE INTO words (id, text, meaning, pronunciation, part_of_speech, difficulty, examples_json, tags_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                word.id,
                word.text,
                word.meaning,
                word.pronunciation,
                word.part_of_speech,
                word.difficulty,
                Self::serialize_json(&word.examples)?,
                Self::serialize_json(&word.tags)?,
                word.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_word(&self, word_id: &str) -> Result<Option<Word>, StoreError> {
        let conn = self.conn()?;
        Self::get_word_conn(&conn, word_id)
    }

    /// 在调用方提供的连接（可处于事务中）上读取，供 read-modify-write 单事务复用。
    pub fn get_word_conn(
        conn: &rusqlite::Connection,
        word_id: &str,
    ) -> Result<Option<Word>, StoreError> {
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
        Self::list_words_conn(&conn, limit, offset)
    }

    /// 在调用方提供的连接/事务上执行列表查询，供 count+list 同事务一致快照复用。
    pub(crate) fn list_words_conn(
        conn: &rusqlite::Connection,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Word>, StoreError> {
        let mut stmt = conn.prepare(&format!(
            "SELECT {WORD_COLS} FROM words ORDER BY created_at DESC LIMIT ?1 OFFSET ?2"
        ))?;
        let rows = stmt.query_map(params![limit as i64, offset as i64], word_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// 在调用方提供的连接/事务上执行总数查询，供 count+list 同事务一致快照复用。
    pub(crate) fn count_words_conn(conn: &rusqlite::Connection) -> Result<u64, StoreError> {
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM words", [], |r| r.get(0))?;
        Ok(count as u64)
    }

    /// 在单个事务快照内同时取总数与当前页，避免两次独立 SELECT 间被并发写入打断
    /// 导致 total 与 items 不一致（分页末页错算）。
    pub fn count_and_list_words(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<(u64, Vec<Word>), StoreError> {
        self.with_transaction(|conn| {
            let total = Self::count_words_conn(conn)?;
            let items = Self::list_words_conn(conn, limit, offset)?;
            Ok((total, items))
        })
    }

    pub fn delete_word(&self, word_id: &str) -> Result<(), StoreError> {
        // 用 with_transaction（RAII Transaction）替代手写 BEGIN/COMMIT/ROLLBACK：
        // commit 前任何早退（闭包 Err / COMMIT 失败 / panic）都由 Transaction 的 Drop
        // 自动 ROLLBACK，连接归还池前不会残留未提交事务，避免污染后续使用者。
        self.with_transaction(|conn| {
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
                // m050 反投毒账本（per (user,word)）、会话已展示词、掌握度事件三表无 FK CASCADE，
                // 必须显式按 word_id 清理，否则删词后孤儿行残留：word_elo_user_contrib 会冻结
                // 同 id 复现后的合法 ELO 更新，另两表污染分析/会话再加入路径。
                "word_elo_user_contrib",
                "session_shown_words",
                "word_mastery_events",
                "etymologies",
                "word_morphemes",
                "alert_dedup",
                "word_favorites",
                "word_notes",
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
        })
    }

    pub fn count_words(&self) -> Result<u64, StoreError> {
        let conn = self.conn()?;
        Self::count_words_conn(&conn)
    }

    pub fn search_words(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<Word>, u64), StoreError> {
        let conn = self.conn()?;
        // 转义用户输入中的 LIKE 元字符(\ % _)并配合 ESCAPE '\\':否则用户搜索词里的 % / _
        // 会被当作通配符——搜 "100%" 会匹配所有以 100 开头的词、搜 "_" 会匹配全表,
        // 导致 COUNT 总数与结果页都错误。参数仍走绑定,不存在 SQL 注入,这里只修正语义。
        let escaped = query
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let pattern = format!("%{escaped}%");

        let total: i64 = conn.query_row(
            "SELECT COUNT(*) FROM words WHERE text LIKE ?1 ESCAPE '\\' OR meaning LIKE ?1 ESCAPE '\\'",
            params![pattern],
            |r| r.get(0),
        )?;

        let mut stmt = conn.prepare(&format!(
            "SELECT {WORD_COLS} FROM words WHERE text LIKE ?1 ESCAPE '\\' OR meaning LIKE ?1 ESCAPE '\\' ORDER BY text LIMIT ?2 OFFSET ?3"
        ))?;
        let rows = stmt.query_map(params![pattern, limit as i64, offset as i64], word_from_row)?;
        let words: Vec<Word> = rows.collect::<Result<Vec<_>, _>>()?;
        Ok((words, total as u64))
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> Store {
        Store::open(":memory:", 5000, 1).unwrap()
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
