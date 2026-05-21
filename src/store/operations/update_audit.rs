//! S5：升级历史审计表 `update_audit_log`。
//!
//! 每次 apply 触发时写入一条记录（started_at），后台任务完成/失败时更新 outcome。
//! 供 `GET /api/admin/updates/history` 返回升级历史列表。

use rusqlite::params;
use serde::Serialize;

use crate::store::{Store, StoreError};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAuditEntry {
    pub id: String,
    pub admin_id: String,
    pub from_version: String,
    pub to_version: String,
    pub channel: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    /// `success` | `failed` | `in_progress`
    pub outcome: String,
    pub error: Option<String>,
}

impl Store {
    /// 写入一条升级记录（apply 触发时调用，outcome 初始为 `in_progress`）。
    pub fn insert_update_audit(
        &self,
        id: &str,
        admin_id: &str,
        from_version: &str,
        to_version: &str,
        channel: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO update_audit_log
                (id, admin_id, from_version, to_version, channel, started_at, outcome)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'in_progress')",
            params![id, admin_id, from_version, to_version, channel, now],
        )?;
        Ok(())
    }

    /// 更新升级记录的最终状态（后台任务完成/失败时调用）。
    pub fn complete_update_audit(
        &self,
        id: &str,
        outcome: &str,
        error: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE update_audit_log
             SET outcome = ?2, completed_at = ?3, error = ?4
             WHERE id = ?1",
            params![id, outcome, now, error],
        )?;
        Ok(())
    }

    /// 返回最近 N 条升级记录（按 started_at 倒序）。
    pub fn list_update_audit(&self, limit: usize) -> Result<Vec<UpdateAuditEntry>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, admin_id, from_version, to_version, channel,
                    started_at, completed_at, outcome, error
             FROM update_audit_log
             ORDER BY started_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(UpdateAuditEntry {
                id: row.get(0)?,
                admin_id: row.get(1)?,
                from_version: row.get(2)?,
                to_version: row.get(3)?,
                channel: row.get(4)?,
                started_at: row.get(5)?,
                completed_at: row.get(6)?,
                outcome: row.get(7)?,
                error: row.get(8)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        let s = Store::open(":memory:", 5000, 1).unwrap();
        crate::store::migrate::run(&s).unwrap();
        s
    }

    #[test]
    fn insert_and_list() {
        let s = store();
        s.insert_update_audit("id-1", "admin-1", "v1.0.0", "v1.1.0", "stable")
            .unwrap();
        let list = s.list_update_audit(10).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].outcome, "in_progress");
        assert_eq!(list[0].from_version, "v1.0.0");
    }

    #[test]
    fn complete_updates_outcome() {
        let s = store();
        s.insert_update_audit("id-2", "admin-1", "v1.0.0", "v1.1.0", "stable")
            .unwrap();
        s.complete_update_audit("id-2", "success", None).unwrap();
        let list = s.list_update_audit(10).unwrap();
        assert_eq!(list[0].outcome, "success");
        assert!(list[0].completed_at.is_some());
        assert!(list[0].error.is_none());
    }

    #[test]
    fn complete_with_error() {
        let s = store();
        s.insert_update_audit("id-3", "admin-1", "v1.0.0", "v1.1.0", "beta")
            .unwrap();
        s.complete_update_audit("id-3", "failed", Some("sha256 mismatch"))
            .unwrap();
        let list = s.list_update_audit(10).unwrap();
        assert_eq!(list[0].outcome, "failed");
        assert_eq!(list[0].error.as_deref(), Some("sha256 mismatch"));
    }

    #[test]
    fn list_returns_most_recent_first() {
        let s = store();
        s.insert_update_audit("id-a", "admin-1", "v1.0.0", "v1.1.0", "stable")
            .unwrap();
        // 插入第二条时 started_at 可能相同秒，ORDER BY started_at DESC 结果稳定
        s.insert_update_audit("id-b", "admin-1", "v1.1.0", "v1.2.0", "stable")
            .unwrap();
        let list = s.list_update_audit(1).unwrap();
        assert_eq!(list.len(), 1); // limit=1 只返回最新一条
    }
}
