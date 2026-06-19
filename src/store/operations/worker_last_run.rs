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

    /// m060：追加一条 worker 运行历史记录（worker_runs，append-only）。
    /// `ran_at` 为 RFC3339 字符串；`outcome` ∈ success|failure|skipped。
    pub fn insert_worker_run(
        &self,
        worker_name: &str,
        ran_at: &str,
        duration_ms: i64,
        outcome: &str,
        error: Option<&str>,
    ) -> Result<(), StoreError> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO worker_runs (worker_name, ran_at, duration_ms, outcome, error)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![worker_name, ran_at, duration_ms, outcome, error],
        )?;
        Ok(())
    }

    /// m060：按 ran_at 倒序读 worker 运行历史；`worker` 为 None 时返回全部 worker。
    pub fn list_worker_runs(
        &self,
        worker: Option<&str>,
        limit: u32,
    ) -> Result<Vec<WorkerRun>, StoreError> {
        let limit = limit.clamp(1, 1000) as i64;
        let conn = self.conn()?;
        let map_row = |row: &rusqlite::Row<'_>| {
            Ok(WorkerRun {
                worker_name: row.get(0)?,
                ran_at: row.get(1)?,
                duration_ms: row.get(2)?,
                outcome: row.get(3)?,
                error: row.get(4)?,
            })
        };
        let rows = match worker {
            Some(name) => {
                let mut stmt = conn.prepare(
                    "SELECT worker_name, ran_at, duration_ms, outcome, error
                     FROM worker_runs
                     WHERE worker_name = ?1
                     ORDER BY ran_at DESC
                     LIMIT ?2",
                )?;
                let out = stmt
                    .query_map(params![name, limit], map_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                out
            }
            None => {
                let mut stmt = conn.prepare(
                    "SELECT worker_name, ran_at, duration_ms, outcome, error
                     FROM worker_runs
                     ORDER BY ran_at DESC
                     LIMIT ?1",
                )?;
                let out = stmt
                    .query_map(params![limit], map_row)?
                    .collect::<Result<Vec<_>, _>>()?;
                out
            }
        };
        Ok(rows)
    }
}

impl Store {
    /// m060：删除 ran_at < cutoff 的 worker_runs 行（保留窗清理，由 monitoring_retention 调用）。
    pub fn cleanup_old_worker_runs(&self, cutoff: &str) -> Result<usize, StoreError> {
        let conn = self.conn()?;
        let n = conn.execute("DELETE FROM worker_runs WHERE ran_at < ?1", params![cutoff])?;
        Ok(n)
    }
}

/// m060：worker_runs 行（append-only 运行历史）。
#[derive(Debug, Clone)]
pub struct WorkerRun {
    pub worker_name: String,
    pub ran_at: String,
    /// NULL 时为 None（skipped 分支可能无耗时）。
    pub duration_ms: Option<i64>,
    pub outcome: String,
    pub error: Option<String>,
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
