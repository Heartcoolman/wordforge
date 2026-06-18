//! M1-A5：worker_last_run 表的读写操作。

use rusqlite::{params, OptionalExtension};

use crate::store::{Store, StoreError};

#[derive(Debug, Clone)]
pub struct WorkerLastRun {
    pub worker_name: String,
    pub last_run_at: i64,
    pub last_duration_ms: i64,
    pub last_error: Option<String>,
    pub last_outcome: String,
    /// 该 worker 单次任务执行中触发并被 catch_unwind 捕获的累计 panic 次数（m059）。
    pub panic_count: i64,
}

impl Store {
    /// 写入（upsert）一次 worker 执行结果。
    pub fn upsert_worker_last_run(
        &self,
        worker_name: &str,
        last_run_at: i64,
        duration_ms: i64,
        outcome: &str,
        error: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO worker_last_run
                 (worker_name, last_run_at, last_duration_ms, last_outcome, last_error)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(worker_name) DO UPDATE SET
                 last_run_at      = excluded.last_run_at,
                 last_duration_ms = excluded.last_duration_ms,
                 last_outcome     = excluded.last_outcome,
                 last_error       = excluded.last_error",
            params![worker_name, last_run_at, duration_ms, outcome, error],
        )?;
        Ok(())
    }

    /// 读取所有 worker 的最后执行记录，按 worker_name 升序。
    pub fn list_worker_last_run(&self) -> Result<Vec<WorkerLastRun>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT worker_name, last_run_at, last_duration_ms, last_error, last_outcome, panic_count
             FROM worker_last_run
             ORDER BY worker_name ASC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(WorkerLastRun {
                    worker_name: row.get(0)?,
                    last_run_at: row.get(1)?,
                    last_duration_ms: row.get(2)?,
                    last_error: row.get(3)?,
                    last_outcome: row.get(4)?,
                    panic_count: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// 读取单个 worker 的最后执行记录；不存在时返回 None。
    pub fn get_worker_last_run(
        &self,
        worker_name: &str,
    ) -> Result<Option<WorkerLastRun>, StoreError> {
        let conn = self.conn()?;
        let row = conn
            .query_row(
                "SELECT worker_name, last_run_at, last_duration_ms, last_error, last_outcome, panic_count
                 FROM worker_last_run
                 WHERE worker_name = ?1",
                params![worker_name],
                |row| {
                    Ok(WorkerLastRun {
                        worker_name: row.get(0)?,
                        last_run_at: row.get(1)?,
                        last_duration_ms: row.get(2)?,
                        last_error: row.get(3)?,
                        last_outcome: row.get(4)?,
                        panic_count: row.get(5)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    /// 自增某 worker 的累计 panic 次数（m059）。
    ///
    /// 与 [`upsert_worker_last_run`] 解耦：panic 发生时该 worker 可能尚无任何
    /// last_run 记录，故走 UPSERT；冲突时仅累加 panic_count，不触碰执行时间/结果列。
    /// 首次插入用占位时间戳（last_run_at=0、last_outcome='panic'），后续正常执行会
    /// 被 [`upsert_worker_last_run`] 覆盖回真实结果，而 panic_count 因不在该 upsert
    /// 的更新列内得以保留。
    pub fn increment_worker_panic(&self, worker_name: &str) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO worker_last_run
                 (worker_name, last_run_at, last_duration_ms, last_outcome, last_error, panic_count)
             VALUES (?1, 0, 0, 'panic', NULL, 1)
             ON CONFLICT(worker_name) DO UPDATE SET
                 panic_count = worker_last_run.panic_count + 1",
            params![worker_name],
        )?;
        Ok(())
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
    fn upsert_and_list() {
        let s = store();
        s.upsert_worker_last_run("metrics_flush", 1000, 50, "success", None)
            .unwrap();
        let rows = s.list_worker_last_run().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].worker_name, "metrics_flush");
        assert_eq!(rows[0].last_run_at, 1000);
        assert_eq!(rows[0].last_duration_ms, 50);
        assert_eq!(rows[0].last_outcome, "success");
        assert!(rows[0].last_error.is_none());
    }

    #[test]
    fn upsert_overwrites_previous() {
        let s = store();
        s.upsert_worker_last_run("session_cleanup", 1000, 10, "success", None)
            .unwrap();
        s.upsert_worker_last_run("session_cleanup", 2000, 20, "failure", Some("db error"))
            .unwrap();
        let rows = s.list_worker_last_run().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].last_run_at, 2000);
        assert_eq!(rows[0].last_outcome, "failure");
        assert_eq!(rows[0].last_error.as_deref(), Some("db error"));
    }

    #[test]
    fn get_missing_returns_none() {
        let s = store();
        assert!(s.get_worker_last_run("nonexistent").unwrap().is_none());
    }
}
