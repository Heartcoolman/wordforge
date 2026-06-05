//! v1.1-P2.5：原 v1.0 M1-A3 杂项 store 操作集合按职责拆三块——
//!   - 用户层个性化数据 → `user_profile_extras`
//!   - 单词元数据与衍生统计 → `word_metadata_extras`
//!   - 基础设施/系统级杂项（本文件）：monitoring、alert_dedup、
//!     password_reset_token、db_info pragmas、create_notification
use rusqlite::{params, OptionalExtension};

use crate::store::keys;
use crate::store::{Store, StoreError};

// 兼容 v1.0：外部继续以 `store::operations::extras::Badge` 引用。
pub use super::user_profile_extras::Badge;

impl Store {
    // -- Monitoring (Workers layer) --

    pub fn insert_monitoring_timeseries(
        &self,
        period_id: &str,
        data: &serde_json::Value,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        let json = Self::serialize_json(data)?;
        conn.execute(
            "INSERT INTO monitoring_timeseries (period_id, data_json) VALUES (?1, ?2)
             ON CONFLICT(period_id) DO UPDATE SET data_json=?2",
            params![period_id, json],
        )?;
        Ok(())
    }

    pub fn cleanup_old_monitoring_events(
        &self,
        before_timestamp: &str,
    ) -> Result<usize, StoreError> {
        let conn = self.conn()?;
        let deleted = conn.execute(
            "DELETE FROM engine_monitoring_events WHERE timestamp < ?1",
            params![before_timestamp],
        )?;
        Ok(deleted)
    }

    // -- Password Reset Tokens --

    pub fn cleanup_expired_reset_tokens(&self) -> Result<usize, StoreError> {
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        let deleted = conn.execute(
            "DELETE FROM password_reset_tokens WHERE expires_at < ?1",
            params![now],
        )?;
        Ok(deleted)
    }

    pub fn create_password_reset_token(
        &self,
        token_hash: &str,
        user_id: &str,
        expires_at: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO password_reset_tokens (token_hash, user_id, expires_at) VALUES (?1, ?2, ?3)",
            params![token_hash, user_id, expires_at],
        )?;
        Ok(())
    }

    pub fn get_password_reset_token(
        &self,
        token_hash: &str,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        let conn = self.conn()?;
        conn.query_row(
            "SELECT token_hash, user_id, expires_at FROM password_reset_tokens WHERE token_hash=?1",
            params![token_hash],
            |r| {
                Ok(serde_json::json!({
                    "token_hash": r.get::<_, String>(0)?,
                    "user_id": r.get::<_, String>(1)?,
                    "expires_at": r.get::<_, String>(2)?,
                }))
            },
        )
        .optional()
        .map_err(StoreError::from)
    }

    pub fn take_password_reset_token(
        &self,
        token_hash: &str,
    ) -> Result<Option<serde_json::Value>, StoreError> {
        let conn = self.conn()?;
        let entry = conn
            .query_row(
                "DELETE FROM password_reset_tokens WHERE token_hash=?1
                 RETURNING token_hash, user_id, expires_at",
                params![token_hash],
                |r| {
                    Ok(serde_json::json!({
                        "token_hash": r.get::<_, String>(0)?,
                        "user_id": r.get::<_, String>(1)?,
                        "expires_at": r.get::<_, String>(2)?,
                    }))
                },
            )
            .optional()?;
        Ok(entry)
    }

    // -- Alert Dedup --

    pub fn get_alert_dedup(&self, user_id: &str, word_id: &str) -> Result<Option<i64>, StoreError> {
        keys::validate_id(user_id)?;
        keys::validate_id(word_id)?;
        let conn = self.conn()?;
        conn.query_row(
            "SELECT last_alerted_at_ms FROM alert_dedup WHERE user_id=?1 AND word_id=?2",
            params![user_id, word_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(StoreError::from)
    }

    pub fn set_alert_dedup(
        &self,
        user_id: &str,
        word_id: &str,
        ts_ms: i64,
    ) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        keys::validate_id(word_id)?;
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO alert_dedup (user_id, word_id, last_alerted_at_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(user_id, word_id) DO UPDATE SET last_alerted_at_ms=?3",
            params![user_id, word_id, ts_ms],
        )?;
        Ok(())
    }

    // -- DB Info Pragmas --

    pub fn db_size_bytes(&self) -> Result<i64, StoreError> {
        let conn = self.conn()?;
        let page_count: i64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let page_size: i64 = conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        Ok(page_count * page_size)
    }

    pub fn db_ping(&self) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.query_row("SELECT 1", [], |r| r.get::<_, i64>(0))?;
        Ok(())
    }

    pub fn db_page_size(&self) -> Result<i64, StoreError> {
        let conn = self.conn()?;
        Ok(conn.query_row("PRAGMA page_size", [], |r| r.get(0))?)
    }

    pub fn db_page_count(&self) -> Result<i64, StoreError> {
        let conn = self.conn()?;
        Ok(conn.query_row("PRAGMA page_count", [], |r| r.get(0))?)
    }

    pub fn db_wal_enabled(&self) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        let mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
        Ok(mode.eq_ignore_ascii_case("wal"))
    }

    /// 当前 SQLite WAL 文件大小（字节）。取主库路径后 stat `<path>-wal`；
    /// 内存库 / 无 WAL 文件 / stat 失败一律返回 0（资源条降级，不阻断 health）。
    pub fn db_wal_size_bytes(&self) -> Result<i64, StoreError> {
        let conn = self.conn()?;
        // PRAGMA database_list 列：seq | name | file（main 库 file 为绝对路径，内存库为空）
        let path: Option<String> = conn
            .query_row("PRAGMA database_list", [], |r| r.get::<_, String>(2))
            .optional()?;
        match path {
            Some(p) if !p.is_empty() => {
                let wal = format!("{p}-wal");
                Ok(std::fs::metadata(&wal).map(|m| m.len() as i64).unwrap_or(0))
            }
            _ => Ok(0),
        }
    }

    pub fn db_table_list(&self) -> Result<Vec<String>, StoreError> {
        let conn = self.conn()?;
        let mut stmt =
            conn.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    // -- Notifications --

    /// Create a single notification.
    pub fn create_notification(&self, notification: &serde_json::Value) -> Result<(), StoreError> {
        let conn = self.conn()?;
        let user_id = notification["userId"].as_str().unwrap_or_default();
        let id = notification["id"].as_str().unwrap_or_default();
        let ntype = notification["type"].as_str().unwrap_or("system");
        let title = notification["title"].as_str().unwrap_or_default();
        let message = notification["message"].as_str().unwrap_or_default();
        let word_id = notification["wordId"].as_str();
        let overdue_hours = notification["overdueHours"].as_i64();
        let read = notification["read"].as_bool().unwrap_or(false);
        let created_at = notification["createdAt"].as_str().unwrap_or_default();
        conn.execute(
            "INSERT OR IGNORE INTO notifications (user_id, id, notification_type, title, message, word_id, overdue_hours, read, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![user_id, id, ntype, title, message, word_id, overdue_hours, read as i64, created_at],
        )?;
        Ok(())
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
    fn monitoring_timeseries_upsert_and_cleanup_events() {
        let (_t, store) = test_store();
        store
            .insert_monitoring_timeseries("p1", &json!({"x":1}))
            .unwrap();
        store
            .insert_monitoring_timeseries("p1", &json!({"x":2}))
            .unwrap();
        let conn = store.connection().unwrap();
        let cnt: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM monitoring_timeseries WHERE period_id='p1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cnt, 1);

        // cleanup: 插一条 timestamp=旧 的 engine_monitoring_events
        conn.execute(
            "INSERT INTO engine_monitoring_events (id, user_id, session_id, event_type, timestamp, latency_ms)
             VALUES ('e1','u1','s1','process_event','2020-01-01T00:00:00Z',0)",
            [],
        )
        .unwrap();
        let removed = store
            .cleanup_old_monitoring_events("2024-01-01T00:00:00Z")
            .unwrap();
        assert_eq!(removed, 1);
    }

    #[test]
    fn password_reset_token_lifecycle() {
        let (_t, store) = test_store();
        let future = (Utc::now() + Duration::hours(1)).to_rfc3339();
        store
            .create_password_reset_token("hash1", "u1", &future)
            .unwrap();
        let got = store.get_password_reset_token("hash1").unwrap().unwrap();
        assert_eq!(got["user_id"], json!("u1"));
        let taken = store.take_password_reset_token("hash1").unwrap().unwrap();
        assert_eq!(taken["token_hash"], json!("hash1"));
        assert!(store.get_password_reset_token("hash1").unwrap().is_none());
        // cleanup 删除过期
        let past = (Utc::now() - Duration::hours(1)).to_rfc3339();
        store
            .create_password_reset_token("hash2", "u1", &past)
            .unwrap();
        let removed = store.cleanup_expired_reset_tokens().unwrap();
        assert!(removed >= 1);
    }

    #[test]
    fn alert_dedup_set_get() {
        let (_t, store) = test_store();
        assert!(store.get_alert_dedup("u1", "w1").unwrap().is_none());
        store.set_alert_dedup("u1", "w1", 100).unwrap();
        assert_eq!(store.get_alert_dedup("u1", "w1").unwrap(), Some(100));
        store.set_alert_dedup("u1", "w1", 200).unwrap();
        assert_eq!(store.get_alert_dedup("u1", "w1").unwrap(), Some(200));
    }

    #[test]
    fn db_info_helpers_report_consistent_values() {
        let (_t, store) = test_store();
        assert!(store.db_size_bytes().unwrap() > 0);
        store.db_ping().unwrap();
        assert!(store.db_page_size().unwrap() > 0);
        assert!(store.db_page_count().unwrap() > 0);
        // WAL 默认开启
        assert!(store.db_wal_enabled().unwrap());
        let tables = store.db_table_list().unwrap();
        assert!(tables.contains(&"words".into()));
    }

    #[test]
    fn create_notification_inserts_row() {
        let (_t, store) = test_store();
        let now = Utc::now().to_rfc3339();
        store
            .create_notification(&json!({
                "userId":"u1","id":"n1","type":"review",
                "title":"t","message":"m","wordId":"w1","overdueHours":3,
                "read":false,"createdAt":now
            }))
            .unwrap();
        // INSERT OR IGNORE 相同 (user_id, id) 不重复插入
        store
            .create_notification(&json!({
                "userId":"u1","id":"n1","type":"review","title":"t2","message":"m2","createdAt":now
            }))
            .unwrap();
        let conn = store.connection().unwrap();
        let title: String = conn
            .query_row(
                "SELECT title FROM notifications WHERE user_id='u1' AND id='n1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(title, "t");
    }
}
