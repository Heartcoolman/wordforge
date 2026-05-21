//! M0-P3：monitoring_retention — engine_monitoring_events 保留策略 (b)
//!
//! 决策：选路径 (b) 仅做 retention 删除 + 月度 VACUUM，不做聚合（设计未冻结）。
//! 调度：每月 1 日 UTC 03:00（低峰时段）；见 workers/mod.rs JobSpec。
//!
//! 逻辑：
//! 1. 删除 timestamp < (now - RETENTION_DAYS) 的 engine_monitoring_events 行。
//! 2. 在 VACUUM 前先 checkpoint WAL，然后在线 VACUUM 回收空页。
//! 3. 所有 SQLite 操作走 blocking runtime 避免阻塞 async executor。

use crate::store::Store;

/// 保留最近 N 天的 engine_monitoring_events 数据。
const RETENTION_DAYS: i64 = 30;

pub async fn run(store: &Store) {
    tracing::info!(retention_days = RETENTION_DAYS, "monitoring_retention: start");
    let store = store.clone();

    let result = crate::blocking::run_blocking("worker.monitoring_retention", move || {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(RETENTION_DAYS)).to_rfc3339();

        let deleted = store.cleanup_old_monitoring_events(&cutoff)?;
        tracing::info!(deleted, cutoff = %cutoff, "monitoring_retention: deleted old events");

        // VACUUM 只在有删除时执行（避免空跑耗 IO）
        if deleted > 0 {
            store.vacuum_db()?;
            tracing::info!("monitoring_retention: VACUUM 完成，磁盘空间已回收");
        }

        Ok::<_, crate::store::StoreError>(deleted)
    })
    .await;

    match result {
        Ok(Ok(deleted)) => tracing::info!(deleted, "monitoring_retention: done"),
        Ok(Err(e)) => tracing::error!(error = %e, "monitoring_retention: store error"),
        Err(e) => tracing::error!(error = %e, "monitoring_retention: blocking task failed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_store() -> (tempfile::TempDir, Store) {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(
            dir.path().join("retention-test.db").to_str().unwrap(),
            5000,
            2,
        )
        .expect("open store");
        store.run_migrations().expect("migrations");
        (dir, store)
    }

    /// 插入一条超出 retention 的旧事件，验证 run() 删除并正常完成（含 VACUUM）。
    #[tokio::test]
    async fn retention_deletes_old_events_and_vacuums() {
        let (_dir, store) = make_store();

        // 插入一条 40 天前的事件（超出 RETENTION_DAYS=30）
        let old_ts = (chrono::Utc::now() - chrono::Duration::days(40)).to_rfc3339();
        {
            let conn = store.connection().expect("conn");
            conn.execute(
                "INSERT INTO engine_monitoring_events
                 (id, user_id, session_id, event_type, timestamp, latency_ms,
                  strategy_json, reward_json)
                 VALUES ('old-1', 'u1', 's1', 'process_event', ?1, 0, '{}', '{}')",
                rusqlite::params![old_ts],
            )
            .expect("insert old event");
        }

        // 插入一条近期事件（应保留）
        let recent_ts = chrono::Utc::now().to_rfc3339();
        {
            let conn = store.connection().expect("conn");
            conn.execute(
                "INSERT INTO engine_monitoring_events
                 (id, user_id, session_id, event_type, timestamp, latency_ms,
                  strategy_json, reward_json)
                 VALUES ('recent-1', 'u1', 's1', 'process_event', ?1, 0, '{}', '{}')",
                rusqlite::params![recent_ts],
            )
            .expect("insert recent event");
        }

        // 确认两条都在
        let count_before: i64 = {
            let conn = store.connection().expect("conn");
            conn.query_row(
                "SELECT COUNT(*) FROM engine_monitoring_events",
                [],
                |r| r.get(0),
            )
            .expect("count before")
        };
        assert_eq!(count_before, 2, "应有 2 条记录");

        // 运行 retention worker
        run(&store).await;

        // 旧事件被删除，近期事件保留
        let count_after: i64 = {
            let conn = store.connection().expect("conn");
            conn.query_row(
                "SELECT COUNT(*) FROM engine_monitoring_events",
                [],
                |r| r.get(0),
            )
            .expect("count after")
        };
        assert_eq!(count_after, 1, "旧事件应被删除，仅保留近期记录");

        let remaining_id: String = {
            let conn = store.connection().expect("conn");
            conn.query_row(
                "SELECT id FROM engine_monitoring_events",
                [],
                |r| r.get(0),
            )
            .expect("remaining id")
        };
        assert_eq!(remaining_id, "recent-1", "保留的应为近期事件");
    }

    /// 无旧事件时 run() 应正常完成且不触发 VACUUM。
    #[tokio::test]
    async fn retention_no_op_when_no_old_events() {
        let (_dir, store) = make_store();
        // 空表，直接运行不应 panic 或报错
        run(&store).await;
    }

    /// vacuum_db() 可独立调用且不报错。
    #[tokio::test]
    async fn vacuum_db_succeeds() {
        let (_dir, store) = make_store();
        store.vacuum_db().expect("vacuum should not fail");
    }
}
