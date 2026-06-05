//! W2-4：离站备份各 target 的运行时状态。
//!
//! 与 `backup-policy` 配置 section 解耦——状态在 backup_offsite worker 每次运行时写入，
//! admin settings BackupRenderer 读出每 target 上次成功/失败，使灾备可验证。

use rusqlite::{params, OptionalExtension};

use crate::store::{Store, StoreError};

/// 一个离站 target 的运行时状态行。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupTargetStatus {
    pub name: String,
    pub uri: String,
    /// 上次成功上传时间（RFC3339）；从未成功为 None。
    pub last_ok_at: Option<String>,
    /// 上次成功上传的字节数。
    pub last_bytes: Option<i64>,
    /// 上次失败原因；上次成功则为 None。
    pub last_error: Option<String>,
    /// 上次尝试时间（成功或失败均更新）。
    pub last_attempt_at: String,
}

impl Store {
    /// 标记某 target 上次上传成功（清 last_error，置 last_ok_at + last_bytes）。
    pub fn mark_backup_target_ok(
        &self,
        name: &str,
        uri: &str,
        bytes: i64,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO backup_target_status
                (name, uri, last_ok_at, last_bytes, last_error, last_attempt_at)
             VALUES (?1, ?2, ?3, ?4, NULL, ?3)
             ON CONFLICT(name) DO UPDATE SET
                uri = ?2, last_ok_at = ?3, last_bytes = ?4, last_error = NULL, last_attempt_at = ?3",
            params![name, uri, now, bytes],
        )?;
        Ok(())
    }

    /// 标记某 target 上次上传失败（置 last_error，保留既有 last_ok_at/last_bytes）。
    pub fn mark_backup_target_failed(
        &self,
        name: &str,
        uri: &str,
        error: &str,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO backup_target_status
                (name, uri, last_ok_at, last_bytes, last_error, last_attempt_at)
             VALUES (?1, ?2, NULL, NULL, ?3, ?4)
             ON CONFLICT(name) DO UPDATE SET
                uri = ?2, last_error = ?3, last_attempt_at = ?4",
            params![name, uri, error, now],
        )?;
        Ok(())
    }

    /// 列出全部 target 状态（按 name 升序）。
    pub fn list_backup_target_status(&self) -> Result<Vec<BackupTargetStatus>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT name, uri, last_ok_at, last_bytes, last_error, last_attempt_at
             FROM backup_target_status ORDER BY name ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(BackupTargetStatus {
                    name: r.get(0)?,
                    uri: r.get(1)?,
                    last_ok_at: r.get(2)?,
                    last_bytes: r.get(3)?,
                    last_error: r.get(4)?,
                    last_attempt_at: r.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 取单个 target 状态（测试用）。
    pub fn get_backup_target_status(
        &self,
        name: &str,
    ) -> Result<Option<BackupTargetStatus>, StoreError> {
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                "SELECT name, uri, last_ok_at, last_bytes, last_error, last_attempt_at
                 FROM backup_target_status WHERE name = ?1",
                params![name],
                |r| {
                    Ok(BackupTargetStatus {
                        name: r.get(0)?,
                        uri: r.get(1)?,
                        last_ok_at: r.get(2)?,
                        last_bytes: r.get(3)?,
                        last_error: r.get(4)?,
                        last_attempt_at: r.get(5)?,
                    })
                },
            )
            .optional()?)
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
    fn ok_then_failed_preserves_last_ok() {
        let s = store();
        s.mark_backup_target_ok("s3-main", "s3://b/p", 4096).unwrap();
        let st = s.get_backup_target_status("s3-main").unwrap().unwrap();
        assert_eq!(st.last_bytes, Some(4096));
        assert!(st.last_ok_at.is_some());
        assert!(st.last_error.is_none());

        // 失败：保留 last_ok_at/last_bytes，置 last_error。
        s.mark_backup_target_failed("s3-main", "s3://b/p", "timeout")
            .unwrap();
        let st2 = s.get_backup_target_status("s3-main").unwrap().unwrap();
        assert_eq!(st2.last_error.as_deref(), Some("timeout"));
        assert!(st2.last_ok_at.is_some(), "失败不应清除上次成功时间");
        assert_eq!(st2.last_bytes, Some(4096));

        // 再次成功：清 last_error。
        s.mark_backup_target_ok("s3-main", "s3://b/p", 8192).unwrap();
        let st3 = s.get_backup_target_status("s3-main").unwrap().unwrap();
        assert!(st3.last_error.is_none());
        assert_eq!(st3.last_bytes, Some(8192));
    }

    #[test]
    fn list_orders_by_name() {
        let s = store();
        s.mark_backup_target_ok("z", "file:///z", 1).unwrap();
        s.mark_backup_target_failed("a", "rsync:h:/a", "boom").unwrap();
        let list = s.list_backup_target_status().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "a");
        assert_eq!(list[1].name, "z");
    }
}
