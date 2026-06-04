use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::store::keys;
use crate::store::operations::wordbooks::{Wordbook, WordbookType};
use crate::store::operations::words::Word;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordbookImportHistory {
    pub id: String,
    pub user_id: String,
    pub source_type: String,
    pub source_name: Option<String>,
    pub source_url: Option<String>,
    pub status: String,
    pub wordbook_id: Option<String>,
    pub wordbook_name: Option<String>,
    pub words_imported: Option<u64>,
    pub words_skipped: Option<u64>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

fn parse_dt(s: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

fn import_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WordbookCenterImport> {
    // user_id 列已改 NOT NULL DEFAULT '';空串是 admin/system 态,在 Rust 层还原为 None。
    let user_id: String = row.get(4)?;
    Ok(WordbookCenterImport {
        remote_id: row.get(0)?,
        local_wordbook_id: row.get(1)?,
        source_url: row.get(2)?,
        version: row.get(3)?,
        user_id: (!user_id.is_empty()).then_some(user_id),
        imported_at: parse_dt(row.get(5)?)?,
        updated_at: parse_dt(row.get(6)?)?,
        word_count: row.get::<_, i64>(7)? as u64,
    })
}

const COLS: &str =
    "remote_id, local_wordbook_id, source_url, version, user_id, imported_at, updated_at, word_count";
const HISTORY_COLS: &str =
    "id, user_id, source_type, source_name, source_url, status, wordbook_id, wordbook_name, words_imported, words_skipped, error_message, created_at";

fn history_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<WordbookImportHistory> {
    Ok(WordbookImportHistory {
        id: row.get(0)?,
        user_id: row.get(1)?,
        source_type: row.get(2)?,
        source_name: row.get(3)?,
        source_url: row.get(4)?,
        status: row.get(5)?,
        wordbook_id: row.get(6)?,
        wordbook_name: row.get(7)?,
        words_imported: row.get::<_, Option<i64>>(8)?.map(|v| v as u64),
        words_skipped: row.get::<_, Option<i64>>(9)?.map(|v| v as u64),
        error_message: row.get(10)?,
        created_at: parse_dt(row.get(11)?)?,
    })
}

impl Store {
    pub fn upsert_wb_center_import(&self, import: &WordbookCenterImport) -> Result<(), StoreError> {
        let prefix = keys::source_url_hash_prefix(&import.source_url);
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO wb_center_imports
                (source_url_hash_prefix, remote_id, local_wordbook_id, source_url, version, user_id, imported_at, updated_at, word_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(source_url_hash_prefix, remote_id, user_id) DO UPDATE SET
               local_wordbook_id = excluded.local_wordbook_id,
               source_url = excluded.source_url,
               version = excluded.version,
               updated_at = excluded.updated_at,
               word_count = excluded.word_count",
            params![
                &prefix,
                &import.remote_id,
                &import.local_wordbook_id,
                &import.source_url,
                &import.version,
                import.user_id.as_deref().unwrap_or(""),
                import.imported_at.to_rfc3339(),
                import.updated_at.to_rfc3339(),
                import.word_count as i64,
            ],
        )?;
        Ok(())
    }

    /// 原子导入远程词书:单事务内建词书 + 批量 upsert 词条与挂接 + 写导入记录,任一步失败整笔
    /// 回滚,杜绝"有词书无导入记录"或半量词条孤儿(多写无事务根因)。SQL 与 upsert_wordbook /
    /// upsert_word / add_word_to_wordbook / upsert_wb_center_import 同口径,改其一须同步此处。
    /// `book.word_count` 应由调用方按实际导入数预置。
    pub fn import_remote_wordbook_atomic(
        &self,
        book: &Wordbook,
        words: &[Word],
        import: &WordbookCenterImport,
    ) -> Result<(), StoreError> {
        keys::validate_id(&book.id)?;
        let prefix = keys::source_url_hash_prefix(&import.source_url);
        let now = Utc::now().to_rfc3339();
        let book_type = match book.book_type {
            WordbookType::System => "system",
            WordbookType::User => "user",
        };
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;

        tx.execute(
            "INSERT INTO wordbooks (id, name, description, book_type, user_id, word_count, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               description = excluded.description,
               book_type = excluded.book_type,
               user_id = excluded.user_id,
               word_count = excluded.word_count",
            params![
                &book.id,
                &book.name,
                &book.description,
                book_type,
                book.user_id.as_deref(),
                book.word_count as i64,
                book.created_at.to_rfc3339(),
            ],
        )?;

        for word in words {
            keys::validate_id(&word.id)?;
            tx.execute(
                "INSERT OR REPLACE INTO words (id, text, meaning, pronunciation, part_of_speech, difficulty, examples_json, tags_json, embedding_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    word.id,
                    word.text,
                    word.meaning,
                    word.pronunciation,
                    word.part_of_speech,
                    word.difficulty,
                    Store::serialize_json(&word.examples)?,
                    Store::serialize_json(&word.tags)?,
                    word.embedding.as_ref().map(Store::serialize_json).transpose()?,
                    word.created_at.to_rfc3339(),
                ],
            )?;
            tx.execute(
                "INSERT OR IGNORE INTO wordbook_words (wordbook_id, word_id, added_at)
                 VALUES (?1, ?2, ?3)",
                params![book.id, word.id, now],
            )?;
        }

        tx.execute(
            "INSERT INTO wb_center_imports
                (source_url_hash_prefix, remote_id, local_wordbook_id, source_url, version, user_id, imported_at, updated_at, word_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(source_url_hash_prefix, remote_id, user_id) DO UPDATE SET
               local_wordbook_id = excluded.local_wordbook_id,
               source_url = excluded.source_url,
               version = excluded.version,
               updated_at = excluded.updated_at,
               word_count = excluded.word_count",
            params![
                &prefix,
                &import.remote_id,
                &import.local_wordbook_id,
                &import.source_url,
                &import.version,
                import.user_id.as_deref().unwrap_or(""),
                import.imported_at.to_rfc3339(),
                import.updated_at.to_rfc3339(),
                import.word_count as i64,
            ],
        )?;

        tx.commit()?;
        Ok(())
    }

    /// 原子同步远程词书:单事务内 upsert 变更词条 + 挂接新词 + 移除"已从远程消失的本来源词" +
    /// 回写词书计数与导入记录,任一步失败整笔回滚(对齐 import_remote_wordbook_atomic,消除 SYNC
    /// 多写无事务的孤儿/半量问题)。`remove_word_ids` 须由调用方限定为本远程来源词(按 wb-center +
    /// remote_id tag),不波及用户手动加入的词。返回同步后该词书实际词条数。SQL 与 upsert_word /
    /// add_word_to_wordbook / remove_word_from_wordbook / upsert_wb_center_import 同口径,改其一须同步此处。
    pub fn sync_remote_wordbook_atomic(
        &self,
        wb_id: &str,
        upserts: &[Word],
        add_word_ids: &[String],
        remove_word_ids: &[String],
        import: &WordbookCenterImport,
    ) -> Result<u64, StoreError> {
        keys::validate_id(wb_id)?;
        let prefix = keys::source_url_hash_prefix(&import.source_url);
        let now = Utc::now().to_rfc3339();
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;

        for word in upserts {
            keys::validate_id(&word.id)?;
            tx.execute(
                "INSERT OR REPLACE INTO words (id, text, meaning, pronunciation, part_of_speech, difficulty, examples_json, tags_json, embedding_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    word.id,
                    word.text,
                    word.meaning,
                    word.pronunciation,
                    word.part_of_speech,
                    word.difficulty,
                    Store::serialize_json(&word.examples)?,
                    Store::serialize_json(&word.tags)?,
                    word.embedding.as_ref().map(Store::serialize_json).transpose()?,
                    word.created_at.to_rfc3339(),
                ],
            )?;
        }

        for word_id in add_word_ids {
            keys::validate_id(word_id)?;
            tx.execute(
                "INSERT OR IGNORE INTO wordbook_words (wordbook_id, word_id, added_at)
                 VALUES (?1, ?2, ?3)",
                params![wb_id, word_id, now],
            )?;
        }

        for word_id in remove_word_ids {
            tx.execute(
                "DELETE FROM wordbook_words WHERE wordbook_id = ?1 AND word_id = ?2",
                params![wb_id, word_id],
            )?;
        }

        // 计数在所有挂接/移除之后,作为词书与导入记录的权威 word_count(单事务内自洽)。
        let word_count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM wordbook_words WHERE wordbook_id = ?1",
            params![wb_id],
            |r| r.get(0),
        )?;

        tx.execute(
            "UPDATE wordbooks SET word_count = ?2 WHERE id = ?1",
            params![wb_id, word_count],
        )?;

        tx.execute(
            "INSERT INTO wb_center_imports
                (source_url_hash_prefix, remote_id, local_wordbook_id, source_url, version, user_id, imported_at, updated_at, word_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(source_url_hash_prefix, remote_id, user_id) DO UPDATE SET
               local_wordbook_id = excluded.local_wordbook_id,
               source_url = excluded.source_url,
               version = excluded.version,
               updated_at = excluded.updated_at,
               word_count = excluded.word_count",
            params![
                &prefix,
                &import.remote_id,
                &import.local_wordbook_id,
                &import.source_url,
                &import.version,
                import.user_id.as_deref().unwrap_or(""),
                import.imported_at.to_rfc3339(),
                import.updated_at.to_rfc3339(),
                word_count,
            ],
        )?;

        tx.commit()?;
        Ok(word_count as u64)
    }

    /// 按 (source_url, remote_id, user_id) 三元组取导入记录。user_id=None 表示 admin/system
    /// 态(存空串)。主键已含 user_id,故各用户的同一 center 词书记录互相隔离——查询/409 预检/同步
    /// 都必须带 user_id,否则会命中他人记录(此前缺 user_id 致第二用户被永久 409 挡住)。
    pub fn get_wb_center_import(
        &self,
        source_url: &str,
        remote_id: &str,
        user_id: Option<&str>,
    ) -> Result<Option<WordbookCenterImport>, StoreError> {
        let prefix = keys::source_url_hash_prefix(source_url);
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                &format!("SELECT {COLS} FROM wb_center_imports WHERE source_url_hash_prefix = ?1 AND remote_id = ?2 AND user_id = ?3"),
                params![&prefix, remote_id, user_id.unwrap_or("")],
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
        // user_id 列已归一为 NOT NULL DEFAULT '':admin/system 态存空串,故 None 查 = '' 而非 IS NULL。
        let key = user_id.unwrap_or("");
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLS} FROM wb_center_imports WHERE user_id = ?1 ORDER BY updated_at DESC"
        ))?;
        let imports = stmt.query_map(params![key], import_from_row)?;
        Ok(imports.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn delete_wb_center_import(
        &self,
        source_url: &str,
        remote_id: &str,
        user_id: Option<&str>,
    ) -> Result<bool, StoreError> {
        let prefix = keys::source_url_hash_prefix(source_url);
        let conn = self.conn()?;
        let deleted = conn.execute(
            "DELETE FROM wb_center_imports WHERE source_url_hash_prefix = ?1 AND remote_id = ?2 AND user_id = ?3",
            params![&prefix, remote_id, user_id.unwrap_or("")],
        )?;
        Ok(deleted > 0)
    }

    pub fn insert_wordbook_import_history(
        &self,
        history: &WordbookImportHistory,
    ) -> Result<(), StoreError> {
        keys::validate_id(&history.id)?;
        keys::validate_id(&history.user_id)?;
        if let Some(wordbook_id) = history.wordbook_id.as_deref() {
            keys::validate_id(wordbook_id)?;
        }
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO wordbook_import_history
                (id, user_id, source_type, source_name, source_url, status, wordbook_id, wordbook_name, words_imported, words_skipped, error_message, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                &history.id,
                &history.user_id,
                &history.source_type,
                history.source_name.as_deref(),
                history.source_url.as_deref(),
                &history.status,
                history.wordbook_id.as_deref(),
                history.wordbook_name.as_deref(),
                history.words_imported.map(|v| v as i64),
                history.words_skipped.map(|v| v as i64),
                history.error_message.as_deref(),
                history.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn list_wordbook_import_history(
        &self,
        user_id: &str,
    ) -> Result<Vec<WordbookImportHistory>, StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {HISTORY_COLS} FROM wordbook_import_history
             WHERE user_id = ?1
             ORDER BY created_at DESC"
        ))?;
        let mut history = stmt
            .query_map(params![user_id], history_from_row)?
            .collect::<Result<Vec<_>, _>>()?;

        let known_wordbooks: std::collections::HashSet<String> = history
            .iter()
            .filter_map(|entry| entry.wordbook_id.clone())
            .collect();

        for import in self.list_wb_center_imports_by_user(Some(user_id))? {
            if known_wordbooks.contains(&import.local_wordbook_id) {
                continue;
            }
            let wordbook = self.get_wordbook(&import.local_wordbook_id)?;
            history.push(WordbookImportHistory {
                id: import.local_wordbook_id.clone(),
                user_id: user_id.to_string(),
                source_type: "center".to_string(),
                source_name: None,
                source_url: Some(import.source_url.clone()),
                status: "success".to_string(),
                wordbook_id: Some(import.local_wordbook_id.clone()),
                wordbook_name: wordbook.map(|book| book.name),
                words_imported: Some(import.word_count),
                words_skipped: None,
                error_message: None,
                created_at: import.imported_at,
            });
        }

        history.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(history)
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
            .get_wb_center_import("https://example.com", "r1", None)
            .unwrap()
            .unwrap();
        assert_eq!(got.local_wordbook_id, "local_r1");
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let store = test_store();
        assert!(store
            .get_wb_center_import("https://x.com", "nope", None)
            .unwrap()
            .is_none());
    }

    #[test]
    fn multi_user_same_remote_are_isolated() {
        // 主键含 user_id:两个 end-user 导入同一 (source_url, remote_id) 各自独立,互不覆盖、互不阻断。
        let store = test_store();
        let url = "https://shared.example.com";
        store
            .upsert_wb_center_import(&sample_import("r1", url, Some("u1")))
            .unwrap();
        store
            .upsert_wb_center_import(&sample_import("r1", url, Some("u2")))
            .unwrap();
        // 各自命名空间都能取到自己的记录
        assert!(store.get_wb_center_import(url, "r1", Some("u1")).unwrap().is_some());
        assert!(store.get_wb_center_import(url, "r1", Some("u2")).unwrap().is_some());
        // admin/system(None)命名空间没有(未导入)
        assert!(store.get_wb_center_import(url, "r1", None).unwrap().is_none());
        // 删除 u1 不影响 u2
        assert!(store.delete_wb_center_import(url, "r1", Some("u1")).unwrap());
        assert!(store.get_wb_center_import(url, "r1", Some("u1")).unwrap().is_none());
        assert!(store.get_wb_center_import(url, "r1", Some("u2")).unwrap().is_some());
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
        assert!(store.delete_wb_center_import(url, "r1", None).unwrap());
        assert!(!store.delete_wb_center_import(url, "r1", None).unwrap());
        assert!(store.get_wb_center_import(url, "r1", None).unwrap().is_none());
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
        let got = store.get_wb_center_import(url, "r1", None).unwrap().unwrap();
        assert_eq!(got.version, "2.0");
        assert_eq!(got.word_count, 20);
    }

    fn tempfile_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let store = Store::open(path.to_str().unwrap(), 5000, 4).unwrap();
        (dir, store)
    }

    fn sample_history(id: &str, user_id: &str) -> WordbookImportHistory {
        WordbookImportHistory {
            id: id.into(),
            user_id: user_id.into(),
            source_type: "upload".into(),
            source_name: Some("mybook.json".into()),
            source_url: None,
            status: "success".into(),
            wordbook_id: Some(format!("wb-{id}")),
            wordbook_name: Some("My Book".into()),
            words_imported: Some(100),
            words_skipped: Some(2),
            error_message: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn insert_and_list_history_orders_by_created_desc() {
        let (_t, store) = tempfile_store();
        let mut h1 = sample_history("h1", "u1");
        h1.created_at = Utc::now() - chrono::Duration::seconds(10);
        let h2 = sample_history("h2", "u1");
        store.insert_wordbook_import_history(&h1).unwrap();
        store.insert_wordbook_import_history(&h2).unwrap();
        // 另一个用户的应被过滤
        store
            .insert_wordbook_import_history(&sample_history("h3", "u2"))
            .unwrap();
        let list = store.list_wordbook_import_history("u1").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "h2");
        assert_eq!(list[1].id, "h1");
    }

    #[test]
    fn list_history_merges_center_imports_for_missing_entries() {
        let (_t, store) = tempfile_store();
        // 仅在 wb_center_imports 中存在
        store
            .upsert_wb_center_import(&sample_import("r1", "https://a.com/b", Some("u1")))
            .unwrap();
        let list = store.list_wordbook_import_history("u1").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].source_type, "center");
        assert_eq!(list[0].wordbook_id.as_deref(), Some("local_r1"));
        // 当 history 已有同 wordbook_id 时不重复
        let h = WordbookImportHistory {
            id: "history-r1".into(),
            user_id: "u1".into(),
            source_type: "center".into(),
            source_name: None,
            source_url: Some("https://a.com/b".into()),
            status: "success".into(),
            wordbook_id: Some("local_r1".into()),
            wordbook_name: None,
            words_imported: Some(10),
            words_skipped: None,
            error_message: None,
            created_at: Utc::now(),
        };
        store.insert_wordbook_import_history(&h).unwrap();
        let list = store.list_wordbook_import_history("u1").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "history-r1");
    }

    #[test]
    fn insert_history_validates_ids() {
        let store = test_store();
        let bad_h = WordbookImportHistory {
            id: "".into(),
            user_id: "u1".into(),
            source_type: "x".into(),
            source_name: None,
            source_url: None,
            status: "ok".into(),
            wordbook_id: None,
            wordbook_name: None,
            words_imported: None,
            words_skipped: None,
            error_message: None,
            created_at: Utc::now(),
        };
        assert!(matches!(
            store.insert_wordbook_import_history(&bad_h).unwrap_err(),
            StoreError::Validation(_)
        ));
    }
}
