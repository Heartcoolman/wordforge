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
                last_seen_at = ?7",
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
            "SELECT id, source, kind, severity, title, message, count, first_seen_at, last_seen_at
             FROM system_alerts
             WHERE last_seen_at >= ?1
             ORDER BY last_seen_at DESC",
        )?;
        let rows = stmt
            .query_map(params![since_rfc3339], |r| {
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
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
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
            .record_system_alert("amas.metrics_flush", "persist_failed", "error", "落库失败", "msg1")
            .unwrap();
        store
            .record_system_alert("amas.metrics_flush", "persist_failed", "error", "落库失败", "msg2")
            .unwrap();
        // 不同 kind 不合并
        store
            .record_system_alert("amas.metrics_flush", "other_failed", "warning", "其他", "msg3")
            .unwrap();

        let all = store.list_recent_system_alerts("2000-01-01T00:00:00+00:00").unwrap();
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
        let future = store.list_recent_system_alerts("2999-01-01T00:00:00+00:00").unwrap();
        assert!(future.is_empty());
    }
}
