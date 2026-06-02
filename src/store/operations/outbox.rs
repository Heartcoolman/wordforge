//! S2-1：领域事件 outbox 持久化 + 死信。
//!
//! records→AMAS 在异步模式（`RECORDS_OUTBOX_ASYNC=true`）下，记录写入与领域事件入队
//! 解耦：handler 把事件持久化到 `outbox`，`outbox_processor` worker 异步消费（指数退避
//! 重试，超 `MAX_ATTEMPTS` 移入 `events_dead_letter`）。重启不丢事件（落 SQLite）。
//! 默认同步老路时 outbox 为空，本模块对生产零影响。

use rusqlite::{params, OptionalExtension};

use crate::store::{Store, StoreError};

/// 一条待消费的 outbox 事件。
#[derive(Debug, Clone)]
pub struct OutboxEvent {
    pub id: i64,
    pub event_type: String,
    pub payload: String,
    pub attempts: i64,
    pub created_at: String,
}

/// admin 监控用 outbox 健康快照。
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxStats {
    /// 待处理事件数。
    pub pending: i64,
    /// 处理滞后秒数 = now − 最旧待处理事件的 created_at；无待处理为 0。
    pub lag_secs: i64,
    /// 死信累计数。
    pub dead_letter: i64,
}

impl Store {
    /// 入队一条 outbox 事件（next_retry_at=now，立即可领取）。返回行 id。
    pub fn enqueue_outbox_event(&self, event_type: &str, payload: &str) -> Result<i64, StoreError> {
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO outbox (event_type, payload, attempts, next_retry_at, created_at)
             VALUES (?1, ?2, 0, ?3, ?3)",
            params![event_type, payload, now],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// 领取到期事件（next_retry_at <= now），按 id 升序，最多 limit 条。
    pub fn claim_due_outbox_events(
        &self,
        now_rfc3339: &str,
        limit: i64,
    ) -> Result<Vec<OutboxEvent>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, event_type, payload, attempts, created_at FROM outbox
             WHERE next_retry_at <= ?1 ORDER BY id ASC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![now_rfc3339, limit], |r| {
            Ok(OutboxEvent {
                id: r.get(0)?,
                event_type: r.get(1)?,
                payload: r.get(2)?,
                attempts: r.get(3)?,
                created_at: r.get(4)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// 处理成功：删除 outbox 行。
    pub fn delete_outbox_event(&self, id: i64) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM outbox WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// 处理失败但未到上限：bump attempts + 退避重排到 next_retry_at。
    pub fn reschedule_outbox_event(
        &self,
        id: i64,
        attempts: i64,
        next_retry_at_rfc3339: &str,
        error: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE outbox SET attempts = ?2, next_retry_at = ?3, last_error = ?4 WHERE id = ?1",
            params![id, attempts, next_retry_at_rfc3339, error],
        )?;
        Ok(())
    }

    /// 处理失败且到上限：原子移入死信表 + 从 outbox 删除。
    /// `final_attempts` = bump 后的最终重试次数（与 worker 日志/告警口径一致）；
    /// 不用 `ev.attempts`（那是 claim 时的 bump 前旧值，会少 1）。
    pub fn move_outbox_to_dead_letter(
        &self,
        ev: &OutboxEvent,
        final_attempts: i64,
        error: &str,
    ) -> Result<(), StoreError> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        let now = chrono::Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO events_dead_letter
                (event_type, payload, attempts, last_error, created_at, dead_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                ev.event_type,
                ev.payload,
                final_attempts,
                error,
                ev.created_at,
                now
            ],
        )?;
        tx.execute("DELETE FROM outbox WHERE id = ?1", params![ev.id])?;
        tx.commit()?;
        Ok(())
    }

    /// admin 监控：outbox 待处理数 + lag(秒) + 死信累计数。
    pub fn outbox_stats(&self) -> Result<OutboxStats, StoreError> {
        let conn = self.conn()?;
        let pending: i64 = conn.query_row("SELECT COUNT(*) FROM outbox", [], |r| r.get(0))?;
        let oldest: Option<String> = conn
            .query_row(
                "SELECT created_at FROM outbox ORDER BY id ASC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        let lag_secs = oldest
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|t| {
                (chrono::Utc::now() - t.with_timezone(&chrono::Utc))
                    .num_seconds()
                    .max(0)
            })
            .unwrap_or(0);
        let dead_letter: i64 =
            conn.query_row("SELECT COUNT(*) FROM events_dead_letter", [], |r| r.get(0))?;
        Ok(OutboxStats {
            pending,
            lag_secs,
            dead_letter,
        })
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

    fn future() -> String {
        (chrono::Utc::now() + chrono::Duration::seconds(3600)).to_rfc3339()
    }

    fn now() -> String {
        chrono::Utc::now().to_rfc3339()
    }

    #[test]
    fn enqueue_claim_delete_roundtrip() {
        let s = store();
        let id = s
            .enqueue_outbox_event("record_created", "{\"k\":1}")
            .unwrap();
        assert!(id > 0);
        let due = s.claim_due_outbox_events(&now(), 10).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].event_type, "record_created");
        assert_eq!(due[0].attempts, 0);
        s.delete_outbox_event(id).unwrap();
        assert!(s.claim_due_outbox_events(&now(), 10).unwrap().is_empty());
    }

    #[test]
    fn rescheduled_event_not_claimed_until_due() {
        let s = store();
        let id = s.enqueue_outbox_event("record_created", "{}").unwrap();
        s.reschedule_outbox_event(id, 1, &future(), "boom").unwrap();
        // next_retry_at 在未来 → 当前 now 不应领取
        assert!(s.claim_due_outbox_events(&now(), 10).unwrap().is_empty());
        // 用未来时间点领取则可见，且 attempts 已 bump
        let due = s
            .claim_due_outbox_events(
                &(chrono::Utc::now() + chrono::Duration::seconds(7200)).to_rfc3339(),
                10,
            )
            .unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].attempts, 1);
    }

    /// 死信路径：重试到上限后移入 events_dead_letter，outbox 清空。
    #[test]
    fn dead_letter_after_max_attempts() {
        let s = store();
        s.enqueue_outbox_event("record_created", "{\"bad\":true}")
            .unwrap();
        let ev = s.claim_due_outbox_events(&now(), 1).unwrap().remove(0);
        s.move_outbox_to_dead_letter(&ev, 5, "exhausted after 5 attempts")
            .unwrap();
        let stats = s.outbox_stats().unwrap();
        assert_eq!(stats.pending, 0, "死信后 outbox 应空");
        assert_eq!(stats.dead_letter, 1, "死信表应有 1 条");
        // 死信表记录的是 final_attempts(5)，而非 ev.attempts(0)
        let recorded: i64 = s
            .conn()
            .unwrap()
            .query_row("SELECT attempts FROM events_dead_letter", [], |r| r.get(0))
            .unwrap();
        assert_eq!(recorded, 5, "死信 attempts 应为最终重试次数");
    }

    /// 重启不丢事件：落 SQLite 文件，重开 Store 后仍可领取。
    #[test]
    fn events_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("outbox-restart.db");
        let p = path.to_str().unwrap();
        {
            let s = Store::open(p, 5000, 1).unwrap();
            s.run_migrations().unwrap();
            s.enqueue_outbox_event("record_created", "{\"persisted\":1}")
                .unwrap();
        } // drop Store → 关池，模拟重启
        let s2 = Store::open(p, 5000, 1).unwrap();
        s2.run_migrations().unwrap();
        let due = s2.claim_due_outbox_events(&now(), 10).unwrap();
        assert_eq!(due.len(), 1, "重启后事件应仍在 outbox");
        assert_eq!(due[0].payload, "{\"persisted\":1}");
    }

    #[test]
    fn outbox_stats_reports_pending_and_lag() {
        let s = store();
        assert_eq!(s.outbox_stats().unwrap().pending, 0);
        s.enqueue_outbox_event("record_created", "{}").unwrap();
        let stats = s.outbox_stats().unwrap();
        assert_eq!(stats.pending, 1);
        assert!(stats.lag_secs >= 0);
    }
}
