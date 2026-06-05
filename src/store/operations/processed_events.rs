//! W1-1：records→AMAS 领域事件幂等账本。
//!
//! `processed_events` 以 `(user_id, client_record_id)` 标记"此事件的 AMAS 状态已应用"。
//! 标记由 [`Store::persist_engine_state_atomic`] 在 AMAS 状态落库的同一 tx 内原子写入
//! （见 `persist_engine_state_atomic` 的 `idempotency` 参数），保证"标记存在 ⟺ AMAS 已应用"。
//! 路由在调用 `process_event` 前用 [`Store::is_event_processed`] 预检：命中即短路跳过 AMAS，
//! 避免 outbox 重放 / 客户端重试在崩溃窗口二次累加 ELO/mastery/trust。
//! AMAS 状态回滚时须配套 [`Store::delete_processed_event`] 清标记，使重试可重新应用。

use rusqlite::params;

use crate::store::{keys, Store, StoreError};

impl Store {
    /// 此 (user_id, client_record_id) 的 AMAS 是否已应用过。
    pub fn is_event_processed(
        &self,
        user_id: &str,
        client_record_id: &str,
    ) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM processed_events WHERE user_id = ?1 AND client_record_id = ?2",
            params![user_id, client_record_id],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    }

    /// 删除幂等标记。仅用于 AMAS 状态回滚后让该事件可重新应用（与手动 rollback 配套）。
    pub fn delete_processed_event(
        &self,
        user_id: &str,
        client_record_id: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            "DELETE FROM processed_events WHERE user_id = ?1 AND client_record_id = ?2",
            params![user_id, client_record_id],
        )?;
        Ok(())
    }

    /// 直接写入幂等标记（INSERT OR IGNORE，幂等）。生产路径由 `persist_engine_state_atomic`
    /// 在 AMAS tx 内原子写入；本方法供测试与边界场景使用。
    pub fn mark_event_processed(
        &self,
        user_id: &str,
        client_record_id: &str,
    ) -> Result<(), StoreError> {
        keys::validate_id(user_id)?;
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR IGNORE INTO processed_events (user_id, client_record_id, processed_at)
             VALUES (?1, ?2, ?3)",
            params![user_id, client_record_id, now],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::store::Store;

    fn store() -> Store {
        let s = Store::open(":memory:", 5000, 1).unwrap();
        s.run_migrations().unwrap();
        s
    }

    #[test]
    fn mark_then_is_processed_roundtrip() {
        let s = store();
        assert!(!s.is_event_processed("u1", "rec-1").unwrap());
        s.mark_event_processed("u1", "rec-1").unwrap();
        assert!(s.is_event_processed("u1", "rec-1").unwrap());
        // 不同 user 同 record_id 互不影响
        assert!(!s.is_event_processed("u2", "rec-1").unwrap());
    }

    #[test]
    fn mark_is_idempotent_and_deletable() {
        let s = store();
        s.mark_event_processed("u1", "rec-1").unwrap();
        s.mark_event_processed("u1", "rec-1").unwrap(); // INSERT OR IGNORE 不报错
        assert!(s.is_event_processed("u1", "rec-1").unwrap());
        s.delete_processed_event("u1", "rec-1").unwrap();
        assert!(!s.is_event_processed("u1", "rec-1").unwrap());
    }
}
