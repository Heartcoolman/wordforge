//! 远程探针 `probe_executions` 表的 Store 操作。
//!
//! 表为审计性质：公共 API 无 DELETE 入口，仅 probe_cleanup cron
//! 按 retention_days 清理；UPDATE 仅允许把 `pending` / `confirm_pending`
//! 推进到终态（`ok` / `error` / `timeout` / `expired` / `unsupported_ctx_version`）。

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeExecutionRow {
    pub id: String,
    pub batch_id: String,
    pub device_id: String,
    pub admin_id: String,
    pub admin_username: String,
    pub script_body: String,
    pub script_sha256: String,
    pub has_cmd_call: bool,
    pub note: Option<String>,
    pub timeout_ms: u32,
    pub status: String,
    pub result_json: Option<String>,
    pub stderr: Option<String>,
    pub duration_ms: Option<u32>,
    pub truncated: bool,
    pub dispatched_at: String,
    pub confirmed_at: Option<String>,
    pub completed_at: Option<String>,
}

/// 单次 dispatch 写入用的插入参数（不含 result / completed_at 等终态字段）。
pub struct ProbeInsert<'a> {
    pub id: &'a str,
    pub batch_id: &'a str,
    pub device_id: &'a str,
    pub admin_id: &'a str,
    pub admin_username: &'a str,
    pub script_body: &'a str,
    pub script_sha256: &'a str,
    pub has_cmd_call: bool,
    pub note: Option<&'a str>,
    pub timeout_ms: u32,
    pub status: &'a str, // 'pending' | 'offline'
    pub dispatched_at: &'a str,
}

/// 终态推进参数。`status` 必须 ∈ {'ok','error','timeout','confirm_pending',
/// 'expired','unsupported_ctx_version'}。
pub struct ProbeStatusUpdate<'a> {
    pub status: &'a str,
    pub result_json: Option<&'a str>,
    pub stderr: Option<&'a str>,
    pub duration_ms: Option<u32>,
    pub truncated: bool,
    pub completed_at: Option<&'a str>,
    pub confirmed_at: Option<&'a str>,
}

#[derive(Debug, Default, Clone)]
pub struct ProbeListFilter {
    pub batch_id: Option<String>,
    pub device_id: Option<String>,
    pub admin_id: Option<String>,
    pub status: Option<String>,
}

impl Store {
    /// 批量插入 dispatch 时的初始行（同一 batch 多 device）。
    pub fn insert_probe_executions(
        &self,
        rows: &[ProbeInsert<'_>],
    ) -> Result<(), StoreError> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        for r in rows {
            tx.execute(
                "INSERT INTO probe_executions (
                    id, batch_id, device_id, admin_id, admin_username,
                    script_body, script_sha256, has_cmd_call, note, timeout_ms,
                    status, dispatched_at
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5,
                    ?6, ?7, ?8, ?9, ?10,
                    ?11, ?12
                 )",
                params![
                    r.id,
                    r.batch_id,
                    r.device_id,
                    r.admin_id,
                    r.admin_username,
                    r.script_body,
                    r.script_sha256,
                    if r.has_cmd_call { 1_i64 } else { 0 },
                    r.note,
                    r.timeout_ms as i64,
                    r.status,
                    r.dispatched_at,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_probe_execution(
        &self,
        request_id: &str,
    ) -> Result<Option<ProbeExecutionRow>, StoreError> {
        let conn = self.conn()?;
        let row = conn
            .query_row(
                "SELECT id, batch_id, device_id, admin_id, admin_username,
                        script_body, script_sha256, has_cmd_call, note, timeout_ms,
                        status, result_json, stderr, duration_ms, truncated,
                        dispatched_at, confirmed_at, completed_at
                 FROM probe_executions WHERE id = ?1",
                params![request_id],
                row_to_probe,
            )
            .optional()?;
        Ok(row)
    }

    pub fn update_probe_status(
        &self,
        request_id: &str,
        upd: &ProbeStatusUpdate<'_>,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE probe_executions SET
                status = ?2,
                result_json = COALESCE(?3, result_json),
                stderr = COALESCE(?4, stderr),
                duration_ms = COALESCE(?5, duration_ms),
                truncated = ?6,
                completed_at = COALESCE(?7, completed_at),
                confirmed_at = COALESCE(?8, confirmed_at)
             WHERE id = ?1",
            params![
                request_id,
                upd.status,
                upd.result_json,
                upd.stderr,
                upd.duration_ms.map(|v| v as i64),
                if upd.truncated { 1_i64 } else { 0 },
                upd.completed_at,
                upd.confirmed_at,
            ],
        )?;
        Ok(())
    }

    pub fn list_probe_executions(
        &self,
        filter: &ProbeListFilter,
        limit: u32,
        offset: u32,
    ) -> Result<(Vec<ProbeExecutionRow>, u64), StoreError> {
        let conn = self.conn()?;

        // 构造 WHERE 子句
        let mut where_clauses: Vec<&str> = Vec::new();
        let mut params_vec: Vec<&dyn rusqlite::ToSql> = Vec::new();
        if let Some(ref v) = filter.batch_id {
            where_clauses.push("batch_id = ?");
            params_vec.push(v);
        }
        if let Some(ref v) = filter.device_id {
            where_clauses.push("device_id = ?");
            params_vec.push(v);
        }
        if let Some(ref v) = filter.admin_id {
            where_clauses.push("admin_id = ?");
            params_vec.push(v);
        }
        if let Some(ref v) = filter.status {
            where_clauses.push("status = ?");
            params_vec.push(v);
        }
        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };

        let total: u64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM probe_executions {where_sql}"),
            params_vec.as_slice(),
            |r| r.get::<_, i64>(0).map(|v| v as u64),
        )?;

        let limit_i = limit as i64;
        let offset_i = offset as i64;
        params_vec.push(&limit_i);
        params_vec.push(&offset_i);

        let sql = format!(
            "SELECT id, batch_id, device_id, admin_id, admin_username,
                    script_body, script_sha256, has_cmd_call, note, timeout_ms,
                    status, result_json, stderr, duration_ms, truncated,
                    dispatched_at, confirmed_at, completed_at
             FROM probe_executions
             {where_sql}
             ORDER BY dispatched_at DESC
             LIMIT ? OFFSET ?"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows: Result<Vec<_>, _> = stmt
            .query_map(params_vec.as_slice(), row_to_probe)?
            .collect();
        Ok((rows?, total))
    }

    /// 数 batch 中 pending / confirm_pending 行数（admin REPL SSE 流用来判断是否
    /// 全部到齐）。
    pub fn count_probe_in_batch(&self, batch_id: &str) -> Result<u64, StoreError> {
        let conn = self.conn()?;
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM probe_executions WHERE batch_id = ?1",
            params![batch_id],
            |r| r.get(0),
        )?;
        Ok(n as u64)
    }

    /// 删除 dispatched_at < cutoff 的行（probe_cleanup cron 用）。
    pub fn delete_probe_older_than(&self, cutoff: &str) -> Result<u64, StoreError> {
        let conn = self.conn()?;
        let n = conn.execute(
            "DELETE FROM probe_executions WHERE dispatched_at < ?1",
            params![cutoff],
        )?;
        Ok(n as u64)
    }
}

use rusqlite::OptionalExtension;

fn row_to_probe(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProbeExecutionRow> {
    Ok(ProbeExecutionRow {
        id: row.get(0)?,
        batch_id: row.get(1)?,
        device_id: row.get(2)?,
        admin_id: row.get(3)?,
        admin_username: row.get(4)?,
        script_body: row.get(5)?,
        script_sha256: row.get(6)?,
        has_cmd_call: row.get::<_, i64>(7)? != 0,
        note: row.get(8)?,
        timeout_ms: row.get::<_, i64>(9)? as u32,
        status: row.get(10)?,
        result_json: row.get(11)?,
        stderr: row.get(12)?,
        duration_ms: row
            .get::<_, Option<i64>>(13)?
            .map(|v| v as u32),
        truncated: row.get::<_, i64>(14)? != 0,
        dispatched_at: row.get(15)?,
        confirmed_at: row.get(16)?,
        completed_at: row.get(17)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ins(id: &str, batch_id: &str, device_id: &str, status: &str) -> ProbeInsert<'static> {
        // leak strings for test simplicity（仅在 #[cfg(test)] 中）
        let id = Box::leak(id.to_string().into_boxed_str());
        let batch_id = Box::leak(batch_id.to_string().into_boxed_str());
        let device_id = Box::leak(device_id.to_string().into_boxed_str());
        let status = Box::leak(status.to_string().into_boxed_str());
        ProbeInsert {
            id,
            batch_id,
            device_id,
            admin_id: "adm-1",
            admin_username: "admin@example.com",
            script_body: "return 1;",
            script_sha256: "abc",
            has_cmd_call: false,
            note: None,
            timeout_ms: 3000,
            status,
            dispatched_at: "2026-05-19T00:00:00Z",
        }
    }

    #[test]
    fn insert_and_get_round_trip() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store
            .insert_probe_executions(&[ins("req-1", "b-1", "dev-A", "pending")])
            .unwrap();
        let row = store.get_probe_execution("req-1").unwrap().unwrap();
        assert_eq!(row.id, "req-1");
        assert_eq!(row.batch_id, "b-1");
        assert_eq!(row.status, "pending");
        assert!(row.result_json.is_none());
    }

    #[test]
    fn update_status_advances_to_ok() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store
            .insert_probe_executions(&[ins("req-2", "b-2", "dev-B", "pending")])
            .unwrap();
        store
            .update_probe_status(
                "req-2",
                &ProbeStatusUpdate {
                    status: "ok",
                    result_json: Some(r#"{"ua":"test"}"#),
                    stderr: None,
                    duration_ms: Some(42),
                    truncated: false,
                    completed_at: Some("2026-05-19T00:00:05Z"),
                    confirmed_at: None,
                },
            )
            .unwrap();
        let row = store.get_probe_execution("req-2").unwrap().unwrap();
        assert_eq!(row.status, "ok");
        assert_eq!(row.duration_ms, Some(42));
        assert_eq!(row.result_json.as_deref(), Some(r#"{"ua":"test"}"#));
    }

    #[test]
    fn list_by_batch() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store
            .insert_probe_executions(&[
                ins("req-a", "b-x", "d1", "pending"),
                ins("req-b", "b-x", "d2", "pending"),
                ins("req-c", "b-y", "d3", "pending"),
            ])
            .unwrap();
        let (rows, total) = store
            .list_probe_executions(
                &ProbeListFilter {
                    batch_id: Some("b-x".into()),
                    ..Default::default()
                },
                50,
                0,
            )
            .unwrap();
        assert_eq!(total, 2);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.batch_id == "b-x"));
    }

    #[test]
    fn delete_older_than_cutoff() {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        // 直接插入两条不同 dispatched_at（绕过 ins 的固定时间戳）。
        // 用块作用域确保 conn 在调用 delete_probe_older_than 之前已释放回 pool。
        {
            let conn = store.conn().unwrap();
            conn.execute(
                "INSERT INTO probe_executions (id, batch_id, device_id, admin_id, admin_username, script_body, script_sha256, has_cmd_call, timeout_ms, status, dispatched_at) VALUES ('old', 'b-old', 'd', 'a', 'a', 's', 'h', 0, 3000, 'ok', '2026-01-01T00:00:00Z')",
                [],
            ).unwrap();
            conn.execute(
                "INSERT INTO probe_executions (id, batch_id, device_id, admin_id, admin_username, script_body, script_sha256, has_cmd_call, timeout_ms, status, dispatched_at) VALUES ('new', 'b-new', 'd', 'a', 'a', 's', 'h', 0, 3000, 'ok', '2026-05-19T00:00:00Z')",
                [],
            ).unwrap();
        }

        let deleted = store
            .delete_probe_older_than("2026-03-01T00:00:00Z")
            .unwrap();
        assert_eq!(deleted, 1);
        assert!(store.get_probe_execution("old").unwrap().is_none());
        assert!(store.get_probe_execution("new").unwrap().is_some());
    }
}
