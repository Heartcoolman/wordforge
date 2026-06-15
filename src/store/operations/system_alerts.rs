//! m037 系统告警表(AMAS 数据软拦截告警的可写载体)。
//! admin 无应用内通知箱,失败告警落此表,由 /api/admin/monitoring/events 时间线透出。
//! dedup key=(source,kind):同源同类失败合并计数,防 worker 周期失败把表打爆。
//! 时间统一用 RFC3339,与 monitoring 端点 cutoff 同格式以便字符串比较。

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemAlert {
    pub id: String,
    pub source: String,
    pub kind: String,
    pub severity: String,
    pub title: String,
    pub message: String,
    pub count: i64,
    pub first_seen_at: String,
    pub last_seen_at: String,
    /// m041:admin 收件箱已读时间(NULL=未读)。
    pub read_at: Option<String>,
    /// m041:确认该告警的 admin id。
    pub acked_by: Option<String>,
}

impl Store {
    /// 记录一条系统告警(dedup upsert)。按 (source,kind) 去重:已存在则 count+1 并更新
    /// severity/title/message/last_seen_at;不存在则插入。severity 取值对齐 monitoring
    /// 词表(error/warning/info)。供 worker(仅 &Store)与 handler 共用。
    pub fn record_system_alert(
        &self,
        source: &str,
        kind: &str,
        severity: &str,
        title: &str,
        message: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO system_alerts
                (id, source, kind, severity, title, message, count, first_seen_at, last_seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?7)
             ON CONFLICT(source, kind) DO UPDATE SET
                count = count + 1,
                severity = ?4,
                title = ?5,
                message = ?6,
                last_seen_at = ?7,
                -- m041:同源同类告警再次发生 → 重置已读态,重新进入未读收件箱
                read_at = NULL,
                acked_by = NULL",
            params![id, source, kind, severity, title, message, now],
        )?;
        Ok(())
    }

    /// 列出 last_seen_at >= since 的告警(最新在前),供 admin 监控时间线合并。
    pub fn list_recent_system_alerts(
        &self,
        since_rfc3339: &str,
    ) -> Result<Vec<SystemAlert>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, source, kind, severity, title, message, count,
                    first_seen_at, last_seen_at, read_at, acked_by
             FROM system_alerts
             WHERE last_seen_at >= ?1
             ORDER BY last_seen_at DESC",
        )?;
        let rows = stmt
            .query_map(params![since_rfc3339], row_to_alert)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// m041:admin 收件箱列表(最新在前)。`unread_only` 时仅返回 read_at IS NULL。
    /// `limit` 限制条数。等价于 `list_admin_alerts_paged(unread_only, limit, 0)`,
    /// 保留原签名供既有调用方使用。
    pub fn list_admin_alerts(
        &self,
        unread_only: bool,
        limit: i64,
    ) -> Result<Vec<SystemAlert>, StoreError> {
        self.list_admin_alerts_paged(unread_only, limit, 0)
    }

    /// P2 修复:admin 收件箱列表分页版(最新在前)。`unread_only` 时仅返回 read_at IS NULL。
    /// `limit`/`offset` 提供分页,使调用方可翻页覆盖**全部**未读告警 —— 修复未读超
    /// INBOX_LIMIT 时被截断项无法单条标记已读、导致 unread 计数与列表割裂的缺陷。
    /// `mark_all_system_alerts_read` 仍作兜底。`offset` 取负数按 0 处理。
    pub fn list_admin_alerts_paged(
        &self,
        unread_only: bool,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<SystemAlert>, StoreError> {
        let offset = offset.max(0);
        let conn = self.conn()?;
        // LIMIT ?1 OFFSET ?2:列序与 row_to_alert 一致。
        let sql = if unread_only {
            "SELECT id, source, kind, severity, title, message, count,
                    first_seen_at, last_seen_at, read_at, acked_by
             FROM system_alerts
             WHERE read_at IS NULL
             ORDER BY last_seen_at DESC
             LIMIT ?1 OFFSET ?2"
        } else {
            "SELECT id, source, kind, severity, title, message, count,
                    first_seen_at, last_seen_at, read_at, acked_by
             FROM system_alerts
             ORDER BY last_seen_at DESC
             LIMIT ?1 OFFSET ?2"
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt
            .query_map(params![limit, offset], row_to_alert)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// m041:admin 收件箱未读计数(read_at IS NULL)。
    pub fn count_unread_system_alerts(&self) -> Result<i64, StoreError> {
        let conn = self.conn()?;
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM system_alerts WHERE read_at IS NULL",
            [],
            |r| r.get(0),
        )?;
        Ok(n)
    }

    /// m041:标记单条告警已读(幂等,首次置 read_at + acked_by)。返回是否命中存量行。
    pub fn mark_system_alert_read(&self, id: &str, admin_id: &str) -> Result<bool, StoreError> {
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        let n = conn.execute(
            "UPDATE system_alerts
             SET read_at = COALESCE(read_at, ?2), acked_by = COALESCE(acked_by, ?3)
             WHERE id = ?1",
            params![id, now, admin_id],
        )?;
        Ok(n > 0)
    }

    /// W2-3:一键全部已读。仅置原本未读(read_at IS NULL)的行,用 COALESCE 不覆盖已有 read_at,
    /// 保持幂等且不改既读行的首次已读时间。返回本次标记的条数。
    /// 注意:record_system_alert 在同源同类再次发生时会重置 read_at=NULL(设计如此),故本操作
    /// 非"永久清空"——新告警仍会回到未读。
    pub fn mark_all_system_alerts_read(&self, admin_id: &str) -> Result<u64, StoreError> {
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        let n = conn.execute(
            "UPDATE system_alerts
             SET read_at = COALESCE(read_at, ?1), acked_by = COALESCE(acked_by, ?2)
             WHERE read_at IS NULL",
            params![now, admin_id],
        )?;
        Ok(n as u64)
    }
}

/// 行 → SystemAlert 映射(供 list_recent / list_admin 复用,列序须一致)。
fn row_to_alert(r: &rusqlite::Row) -> rusqlite::Result<SystemAlert> {
    Ok(SystemAlert {
        id: r.get(0)?,
        source: r.get(1)?,
        kind: r.get(2)?,
        severity: r.get(3)?,
        title: r.get(4)?,
        message: r.get(5)?,
        count: r.get(6)?,
        first_seen_at: r.get(7)?,
        last_seen_at: r.get(8)?,
        read_at: r.get(9)?,
        acked_by: r.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use crate::store::Store;

    fn test_store() -> Store {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store.run_migrations().unwrap();
        store
    }

    #[test]
    fn record_dedups_by_source_kind_and_counts() {
        let store = test_store();
        store
            .record_system_alert(
                "amas.metrics_flush",
                "persist_failed",
                "error",
                "落库失败",
                "msg1",
            )
            .unwrap();
        store
            .record_system_alert(
                "amas.metrics_flush",
                "persist_failed",
                "error",
                "落库失败",
                "msg2",
            )
            .unwrap();
        // 不同 kind 不合并
        store
            .record_system_alert(
                "amas.metrics_flush",
                "other_failed",
                "warning",
                "其他",
                "msg3",
            )
            .unwrap();

        let all = store
            .list_recent_system_alerts("2000-01-01T00:00:00+00:00")
            .unwrap();
        assert_eq!(all.len(), 2);
        let persist = all
            .iter()
            .find(|a| a.kind == "persist_failed")
            .expect("persist row");
        assert_eq!(persist.count, 2);
        assert_eq!(persist.message, "msg2"); // 最新 message
    }

    #[test]
    fn list_recent_respects_since() {
        let store = test_store();
        store
            .record_system_alert("s", "k", "error", "t", "m")
            .unwrap();
        // since 在未来 → 无结果
        let future = store
            .list_recent_system_alerts("2999-01-01T00:00:00+00:00")
            .unwrap();
        assert!(future.is_empty());
    }

    #[test]
    fn admin_inbox_read_and_unread_count() {
        let store = test_store();
        store
            .record_system_alert("s1", "k1", "error", "t1", "m1")
            .unwrap();
        store
            .record_system_alert("s2", "k2", "warning", "t2", "m2")
            .unwrap();

        // 初始两条全未读
        assert_eq!(store.count_unread_system_alerts().unwrap(), 2);
        let unread = store.list_admin_alerts(true, 50).unwrap();
        assert_eq!(unread.len(), 2);

        // 标记其一已读
        let id = unread[0].id.clone();
        assert!(store.mark_system_alert_read(&id, "admin-x").unwrap());
        assert_eq!(store.count_unread_system_alerts().unwrap(), 1);
        // unread 过滤后只剩一条
        assert_eq!(store.list_admin_alerts(true, 50).unwrap().len(), 1);
        // 全量仍两条,且已读条带 read_at/acked_by
        let all = store.list_admin_alerts(false, 50).unwrap();
        assert_eq!(all.len(), 2);
        let read_row = all.iter().find(|a| a.id == id).unwrap();
        assert!(read_row.read_at.is_some());
        assert_eq!(read_row.acked_by.as_deref(), Some("admin-x"));
    }

    /// W2-3:一键全部已读——仅标记未读行,幂等不覆盖已读时间。
    #[test]
    fn mark_all_alerts_read_clears_unread() {
        let store = test_store();
        store.record_system_alert("s1", "k1", "error", "t1", "m1").unwrap();
        store.record_system_alert("s2", "k2", "warning", "t2", "m2").unwrap();
        store.record_system_alert("s3", "k3", "info", "t3", "m3").unwrap();
        // 先单条已读一条,记录其 read_at 不应被全部已读覆盖。
        let one = store.list_admin_alerts(true, 50).unwrap()[0].id.clone();
        store.mark_system_alert_read(&one, "admin-a").unwrap();
        let read_at_before = store
            .list_admin_alerts(false, 50)
            .unwrap()
            .into_iter()
            .find(|a| a.id == one)
            .unwrap()
            .read_at;

        // 全部已读:标记剩余 2 条,返回标记条数。
        let marked = store.mark_all_system_alerts_read("admin-b").unwrap();
        assert_eq!(marked, 2, "应只标记原本未读的 2 条");
        assert_eq!(store.count_unread_system_alerts().unwrap(), 0);

        // 既读行的 read_at/acked_by 未被覆盖(COALESCE 保首次)。
        let after = store
            .list_admin_alerts(false, 50)
            .unwrap()
            .into_iter()
            .find(|a| a.id == one)
            .unwrap();
        assert_eq!(after.read_at, read_at_before);
        assert_eq!(after.acked_by.as_deref(), Some("admin-a"));

        // 幂等:再次全部已读标记 0 条。
        assert_eq!(store.mark_all_system_alerts_read("admin-c").unwrap(), 0);
    }

    #[test]
    fn recurring_alert_resets_read_state() {
        let store = test_store();
        store
            .record_system_alert("s", "k", "error", "t", "m1")
            .unwrap();
        let id = store.list_admin_alerts(false, 1).unwrap()[0].id.clone();
        store.mark_system_alert_read(&id, "admin-x").unwrap();
        assert_eq!(store.count_unread_system_alerts().unwrap(), 0);
        // 同源同类再次发生 → 重置为未读
        store
            .record_system_alert("s", "k", "error", "t", "m2")
            .unwrap();
        assert_eq!(store.count_unread_system_alerts().unwrap(), 1);
    }

    #[test]
    fn mark_read_missing_id_returns_false() {
        let store = test_store();
        assert!(!store.mark_system_alert_read("nope", "admin-x").unwrap());
    }

    /// P2 修复:分页可覆盖被 limit 截断的未读告警,使其能被逐条标记已读。
    #[test]
    fn paged_unread_covers_truncated_alerts() {
        let store = test_store();
        // 造 5 条全未读告警(不同 source/kind,避免 dedup 合并)。
        for i in 0..5 {
            store
                .record_system_alert(
                    &format!("s{i}"),
                    &format!("k{i}"),
                    "error",
                    "t",
                    "m",
                )
                .unwrap();
        }
        assert_eq!(store.count_unread_system_alerts().unwrap(), 5);

        // 第一页:limit=2 模拟 INBOX_LIMIT 截断。
        let page0 = store.list_admin_alerts_paged(true, 2, 0).unwrap();
        assert_eq!(page0.len(), 2);
        // 翻页拿到第 3、4 条 —— 这些在旧实现里会被截断、无法单条标记。
        let page1 = store.list_admin_alerts_paged(true, 2, 2).unwrap();
        assert_eq!(page1.len(), 2);
        let page2 = store.list_admin_alerts_paged(true, 2, 4).unwrap();
        assert_eq!(page2.len(), 1);

        // 三页 id 不重叠且并集=全部 5 条。
        let mut ids: Vec<String> = page0
            .iter()
            .chain(page1.iter())
            .chain(page2.iter())
            .map(|a| a.id.clone())
            .collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 5, "分页应无重叠地覆盖全部未读");

        // 逐条标记已读后未读归零(证明截断项可达)。
        for id in &ids {
            assert!(store.mark_system_alert_read(id, "admin-x").unwrap());
        }
        assert_eq!(store.count_unread_system_alerts().unwrap(), 0);

        // 负 offset 当作 0;list_admin_alerts 等价于 offset=0。
        assert_eq!(
            store.list_admin_alerts_paged(false, 2, -1).unwrap().len(),
            store.list_admin_alerts(false, 2).unwrap().len()
        );
    }
}
