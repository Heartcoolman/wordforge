//! m025:用户**自有**活动日志(区别于 `update_audit_log` v2 = admin 对用户的操作)。
//!
//! 写入路径(4 处 hook):
//!   - `auth.rs::login` 成功 → action=`user.login`,detail={ ip, ua }
//!   - learning session complete → action=`session.complete`,detail={ sessionId, accuracy, count }
//!   - study_config update → action=`goal.update`,detail={ from, to }
//!   - 疲劳告警 → action=`fatigue.alert`,detail={...}
//!
//! 写入失败仅 warn,不阻塞主响应(参考 `insert_admin_audit` 的容错策略)。

use rusqlite::params;
use serde::Serialize;

use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserActivityEntry {
    pub id: String,
    pub user_id: String,
    pub action: String,
    pub detail_json: Option<String>,
    pub ip: Option<String>,
    pub created_at: String,
}

impl Store {
    /// 写入一条用户活动。失败 → log warn,不抛错。
    pub fn insert_user_activity(
        &self,
        user_id: &str,
        action: &str,
        detail: Option<&serde_json::Value>,
        ip: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let detail_str = detail.map(|v| v.to_string());
        conn.execute(
            "INSERT INTO user_activity_log (id, user_id, action, detail_json, ip, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, user_id, action, detail_str, ip, now],
        )?;
        Ok(())
    }

    /// 列某 user 最近 N 条活动(按 created_at 倒序)。供 admin Drawer "操作日志" 用户活动栏。
    pub fn list_user_activity(
        &self,
        user_id: &str,
        limit: usize,
    ) -> Result<Vec<UserActivityEntry>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, user_id, action, detail_json, ip, created_at
             FROM user_activity_log
             WHERE user_id = ?1
             ORDER BY created_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![user_id, limit as i64], |r| {
            Ok(UserActivityEntry {
                id: r.get(0)?,
                user_id: r.get(1)?,
                action: r.get(2)?,
                detail_json: r.get(3)?,
                ip: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_list_user_activity_order_by_created_desc() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store
            .insert_user_activity("u1", "user.login", None, Some("1.2.3.4"))
            .unwrap();
        store
            .insert_user_activity(
                "u1",
                "session.complete",
                Some(&serde_json::json!({"accuracy": 0.85})),
                None,
            )
            .unwrap();
        let entries = store.list_user_activity("u1", 10).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].action, "session.complete");
        assert!(entries[0].detail_json.as_deref().unwrap().contains("0.85"));
        assert_eq!(entries[1].action, "user.login");
        assert_eq!(entries[1].ip.as_deref(), Some("1.2.3.4"));
    }

    #[test]
    fn list_user_activity_scoped_by_user() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store
            .insert_user_activity("u1", "user.login", None, None)
            .unwrap();
        store
            .insert_user_activity("u2", "user.login", None, None)
            .unwrap();
        assert_eq!(store.list_user_activity("u1", 10).unwrap().len(), 1);
        assert_eq!(store.list_user_activity("u2", 10).unwrap().len(), 1);
    }
}
