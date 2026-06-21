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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordbookProgressStats {
    pub wordbook_id: String,
    pub word_count: u64,
    pub studied_words: u64,
    pub mastered_words: u64,
    pub reviewing_words: u64,
    pub forgotten_words: u64,
    pub last_studied_at: Option<DateTime<Utc>>,
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
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
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
        let conn = self.conn()?;
        Self::get_wordbook_conn(&conn, wordbook_id)
    }

    /// 仅更新词库的 name / description,绝不触碰 word_count。
    /// 避免 admin 改名走 get→upsert 的跨连接 read-modify-write 把陈旧 word_count 写回,
    /// 覆盖并发 add/remove_word_to_wordbook 维护的真实计数(lost update)。
    pub fn update_wordbook_meta(
        &self,
        wordbook_id: &str,
        name: &str,
        description: &str,
    ) -> Result<bool, StoreError> {
        keys::validate_id(wordbook_id)?;
        let conn = self.conn()?;
        let affected = conn.execute(
            "UPDATE wordbooks SET name = ?2, description = ?3 WHERE id = ?1",
            params![wordbook_id, name, description],
        )?;
        Ok(affected > 0)
    }

    /// 同 [`get_wordbook`],但在调用方提供的连接/事务上执行,供 read-modify-write 收进事务。
    pub fn get_wordbook_conn(
        conn: &rusqlite::Connection,
        wordbook_id: &str,
    ) -> Result<Option<Wordbook>, StoreError> {
        keys::validate_id(wordbook_id)?;
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
        let mut stmt = conn.prepare(&format!(
            "SELECT {WB_COLS} FROM wordbooks WHERE book_type = 'system' ORDER BY name"
        ))?;
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
        Self::list_wordbook_words_conn(&conn, wordbook_id, limit, offset)
    }

    /// 在调用方提供的连接/事务上执行词书内词列表查询，供 count+list 同事务一致快照复用。
    fn list_wordbook_words_conn(
        conn: &rusqlite::Connection,
        wordbook_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<String>, StoreError> {
        let mut stmt = conn.prepare(
            "SELECT word_id FROM wordbook_words WHERE wordbook_id = ?1 \
             ORDER BY added_at, word_id LIMIT ?2 OFFSET ?3",
        )?;
        let ids = stmt
            .query_map(params![wordbook_id, limit as i64, offset as i64], |r| {
                r.get(0)
            })?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(ids)
    }

    /// 在单个事务快照内同时取词书词数与当前页 word_id 列表，避免两次独立 SELECT 间
    /// 被并发增删词打断导致 total 与列表不一致。
    pub fn count_and_list_wordbook_words(
        &self,
        wordbook_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<(u64, Vec<String>), StoreError> {
        keys::validate_id(wordbook_id)?;
        self.with_transaction(|conn| {
            let total: i64 = conn.query_row(
                "SELECT COUNT(*) FROM wordbook_words WHERE wordbook_id = ?1",
                params![wordbook_id],
                |r| r.get(0),
            )?;
            let word_ids = Self::list_wordbook_words_conn(conn, wordbook_id, limit, offset)?;
            Ok((total as u64, word_ids))
        })
    }

    // ────────────────── m022:wordbook_local_tags ──────────────────
    // 本地标签覆盖层(远端 metadata 不可改),由 admin wordbook-center 页编辑。

    /// 列出 wordbook 的本地标签(按字母序)。
    pub fn list_wordbook_local_tags(&self, wordbook_id: &str) -> Result<Vec<String>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT tag FROM wordbook_local_tags WHERE wordbook_id = ?1 ORDER BY tag ASC",
        )?;
        let tags: Result<Vec<String>, _> = stmt
            .query_map(params![wordbook_id], |r| r.get::<_, String>(0))?
            .collect();
        Ok(tags?)
    }

    /// 增量加入标签;已存在的跳过(ON CONFLICT DO NOTHING)。
    pub fn add_wordbook_local_tags(
        &self,
        wordbook_id: &str,
        tags: &[String],
        created_by: Option<&str>,
    ) -> Result<(), StoreError> {
        if tags.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let now = chrono::Utc::now().to_rfc3339();
        for tag in tags {
            let trimmed = tag.trim();
            if trimmed.is_empty() || trimmed.len() > 64 {
                continue;
            }
            tx.execute(
                "INSERT INTO wordbook_local_tags (wordbook_id, tag, created_at, created_by)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(wordbook_id, tag) DO NOTHING",
                params![wordbook_id, trimmed, now, created_by],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// 删除指定标签。
    pub fn remove_wordbook_local_tags(
        &self,
        wordbook_id: &str,
        tags: &[String],
    ) -> Result<(), StoreError> {
        if tags.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        for tag in tags {
            tx.execute(
                "DELETE FROM wordbook_local_tags WHERE wordbook_id = ?1 AND tag = ?2",
                params![wordbook_id, tag.trim()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// 整体替换标签:先清空再写入(单事务)。前端"标签编辑器"提交全集时使用。
    pub fn set_wordbook_local_tags(
        &self,
        wordbook_id: &str,
        tags: &[String],
        created_by: Option<&str>,
    ) -> Result<(), StoreError> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "DELETE FROM wordbook_local_tags WHERE wordbook_id = ?1",
            params![wordbook_id],
        )?;
        let now = chrono::Utc::now().to_rfc3339();
        for tag in tags {
            let trimmed = tag.trim();
            if trimmed.is_empty() || trimmed.len() > 64 {
                continue;
            }
            tx.execute(
                "INSERT INTO wordbook_local_tags (wordbook_id, tag, created_at, created_by)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(wordbook_id, tag) DO NOTHING",
                params![wordbook_id, trimmed, now, created_by],
            )?;
        }
        tx.commit()?;
        Ok(())
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

    pub fn get_wordbook_progress_stats(
        &self,
        user_id: &str,
        wordbook_id: &str,
    ) -> Result<WordbookProgressStats, StoreError> {
        keys::validate_id(user_id)?;
        keys::validate_id(wordbook_id)?;
        let conn = self.conn()?;
        let word_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM wordbook_words WHERE wordbook_id = ?1",
            params![wordbook_id],
            |r| r.get(0),
        )?;
        let studied_words: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT ww.word_id)
             FROM wordbook_words ww
             LEFT JOIN word_learning_states wls
                ON wls.user_id = ?1 AND wls.word_id = ww.word_id
             LEFT JOIN learning_records lr
                ON lr.user_id = ?1 AND lr.word_id = ww.word_id
             WHERE ww.wordbook_id = ?2
               AND (
                    COALESCE(wls.total_attempts, 0) > 0
                    OR lr.id IS NOT NULL
               )",
            params![user_id, wordbook_id],
            |r| r.get(0),
        )?;
        let (mastered_words, reviewing_words, forgotten_words): (i64, i64, i64) = conn.query_row(
            "SELECT
                COALESCE(SUM(CASE WHEN wls.state = 'MASTERED' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN wls.state = 'REVIEWING' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN wls.state = 'FORGOTTEN' THEN 1 ELSE 0 END), 0)
             FROM wordbook_words ww
             LEFT JOIN word_learning_states wls
                ON wls.user_id = ?1 AND wls.word_id = ww.word_id
             WHERE ww.wordbook_id = ?2",
            params![user_id, wordbook_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let last_studied_raw: Option<String> = conn.query_row(
            "SELECT MAX(lr.created_at)
             FROM wordbook_words ww
             JOIN learning_records lr
                ON lr.user_id = ?1 AND lr.word_id = ww.word_id
             WHERE ww.wordbook_id = ?2",
            params![user_id, wordbook_id],
            |r| r.get(0),
        )?;
        let last_studied_at = last_studied_raw.map(parse_dt).transpose()?;

        Ok(WordbookProgressStats {
            wordbook_id: wordbook_id.to_string(),
            word_count: word_count as u64,
            studied_words: studied_words as u64,
            mastered_words: mastered_words as u64,
            reviewing_words: reviewing_words as u64,
            forgotten_words: forgotten_words as u64,
            last_studied_at,
        })
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
        store
            .upsert_wordbook(&sample_wordbook("wb2", WordbookType::System, None))
            .unwrap();
        store
            .upsert_wordbook(&sample_wordbook("wb1", WordbookType::System, None))
            .unwrap();
        store
            .upsert_wordbook(&sample_wordbook("wb3", WordbookType::User, Some("u1")))
            .unwrap();
        let books = store.list_system_wordbooks().unwrap();
        assert_eq!(books.len(), 2);
        assert_eq!(books[0].id, "wb1");
    }

    #[test]
    fn list_user_wordbooks_filters_by_user() {
        let store = test_store();
        store
            .upsert_wordbook(&sample_wordbook("wb1", WordbookType::User, Some("u1")))
            .unwrap();
        store
            .upsert_wordbook(&sample_wordbook("wb2", WordbookType::User, Some("u2")))
            .unwrap();
        let books = store.list_user_wordbooks("u1").unwrap();
        assert_eq!(books.len(), 1);
        assert_eq!(books[0].id, "wb1");
    }

    #[test]
    fn add_and_remove_word() {
        let store = test_store();
        store
            .upsert_wordbook(&sample_wordbook("wb1", WordbookType::System, None))
            .unwrap();
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
        store
            .upsert_wordbook(&sample_wordbook("wb1", WordbookType::System, None))
            .unwrap();
        store.add_word_to_wordbook("wb1", "w1").unwrap();
        store.add_word_to_wordbook("wb1", "w2").unwrap();
        store.add_word_to_wordbook("wb1", "w3").unwrap();
        assert_eq!(store.count_wordbook_words("wb1").unwrap(), 3);

        let page = store.list_wordbook_words("wb1", 2, 0).unwrap();
        assert_eq!(page.len(), 2);
        let page2 = store.list_wordbook_words("wb1", 2, 2).unwrap();
        assert_eq!(page2.len(), 1);
    }

    fn tempfile_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let store = Store::open(path.to_str().unwrap(), 5000, 4).unwrap();
        (dir, store)
    }

    #[test]
    fn book_type_helpers_roundtrip() {
        assert_eq!(book_type_to_str(&WordbookType::System), "system");
        assert_eq!(book_type_to_str(&WordbookType::User), "user");
        assert_eq!(book_type_from_str("user"), WordbookType::User);
        // 任何其他字符串都回退到 System
        assert_eq!(book_type_from_str("system"), WordbookType::System);
        assert_eq!(book_type_from_str("other"), WordbookType::System);
    }

    #[test]
    fn upsert_overwrites_existing_wordbook_fields() {
        let store = test_store();
        let mut wb = sample_wordbook("wb1", WordbookType::System, None);
        store.upsert_wordbook(&wb).unwrap();
        wb.name = "renamed".into();
        wb.description = "desc".into();
        store.upsert_wordbook(&wb).unwrap();
        let got = store.get_wordbook("wb1").unwrap().unwrap();
        assert_eq!(got.name, "renamed");
        assert_eq!(got.description, "desc");
    }

    #[test]
    fn remove_word_from_nonexistent_wordbook_returns_false() {
        let store = test_store();
        assert!(!store.remove_word_from_wordbook("nope", "w1").unwrap());
    }

    #[test]
    fn wordbook_progress_stats_returns_aggregates() {
        let (_t, store) = tempfile_store();
        store
            .upsert_wordbook(&sample_wordbook("wb1", WordbookType::System, None))
            .unwrap();
        store.add_word_to_wordbook("wb1", "w1").unwrap();
        store.add_word_to_wordbook("wb1", "w2").unwrap();
        store.add_word_to_wordbook("wb1", "w3").unwrap();

        let mut s1 = crate::store::operations::word_states::WordLearningState {
            user_id: "u1".into(),
            word_id: "w1".into(),
            state: crate::store::operations::word_states::WordState::Mastered,
            mastery_level: 0.9,
            next_review_date: None,
            half_life: 24.0,
            correct_streak: 0,
            total_attempts: 3,
            updated_at: Utc::now(),
        };
        store.set_word_learning_state(&s1).unwrap();
        s1.word_id = "w2".into();
        s1.state = crate::store::operations::word_states::WordState::Reviewing;
        store.set_word_learning_state(&s1).unwrap();
        // 一条 learning record on w1 用于 last_studied_at
        let r = crate::store::operations::records::LearningRecord {
            id: "r1".into(),
            user_id: "u1".into(),
            word_id: "w1".into(),
            is_correct: true,
            response_time_ms: 100,
            session_id: None,
            created_at: Utc::now(),
            record_type: crate::store::operations::records::RecordType::All,
            self_rating: None,
            question_mode: None,
        };
        store.create_record(&r).unwrap();

        let stats = store.get_wordbook_progress_stats("u1", "wb1").unwrap();
        assert_eq!(stats.word_count, 3);
        assert_eq!(stats.studied_words, 2);
        assert_eq!(stats.mastered_words, 1);
        assert_eq!(stats.reviewing_words, 1);
        assert_eq!(stats.forgotten_words, 0);
        assert!(stats.last_studied_at.is_some());
    }

    #[test]
    fn list_system_wordbooks_excludes_user_wordbooks() {
        let store = test_store();
        store
            .upsert_wordbook(&sample_wordbook("sys-a", WordbookType::System, None))
            .unwrap();
        store
            .upsert_wordbook(&sample_wordbook("usr-1", WordbookType::User, Some("u1")))
            .unwrap();
        let sys = store.list_system_wordbooks().unwrap();
        assert!(sys.iter().all(|w| w.book_type == WordbookType::System));
    }

    #[test]
    fn validation_rejects_empty_ids() {
        let store = test_store();
        let bad = sample_wordbook("", WordbookType::System, None);
        assert!(matches!(
            store.upsert_wordbook(&bad).unwrap_err(),
            StoreError::Validation(_)
        ));
        assert!(matches!(
            store.add_word_to_wordbook("", "w").unwrap_err(),
            StoreError::Validation(_)
        ));
        assert!(matches!(
            store.get_wordbook("").unwrap_err(),
            StoreError::Validation(_)
        ));
    }
}
