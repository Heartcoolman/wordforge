//! wal_checkpoint — 周期主动 `PRAGMA wal_checkpoint(TRUNCATE)` 回收 WAL。
//!
//! 背景：Store::open 仅设 wal_autocheckpoint（PASSIVE 内联，不缩 WAL 文件、且高写时与写锁
//! 争用），高并发答题下 WAL 会膨胀到数百 MB，臃肿 WAL 放大每次 commit 的页查找成本。本 worker
//! 在低峰周期把 WAL 帧落库并截断文件，把回收工作从写线程移走。
//!
//! 调度：每 10 分钟（见 workers/mod.rs JobSpec）。SQLite 操作走 blocking runtime。
//! TRUNCATE 在有并发写时可能返回 busy，仅 warn、不致命，下个周期再试。

use crate::store::Store;

pub async fn run(store: &Store) {
    let store = store.clone();
    let result = crate::blocking::run_blocking("worker.wal_checkpoint", move || {
        store.checkpoint_truncate()
    })
    .await;

    match result {
        Ok(Ok(())) => tracing::debug!("wal_checkpoint: WAL 已截断回收"),
        Ok(Err(e)) => tracing::warn!(error = %e, "wal_checkpoint: checkpoint 失败（多为并发写竞争，下周期再试）"),
        Err(e) => tracing::error!(error = %e, "wal_checkpoint: blocking task failed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_store() -> (tempfile::TempDir, Store) {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(
            dir.path().join("wal-checkpoint-test.db").to_str().unwrap(),
            5000,
            2,
        )
        .expect("open store");
        store.run_migrations().expect("migrations");
        (dir, store)
    }

    /// 写入若干行后 run() 正常完成且不报错。
    #[tokio::test]
    async fn checkpoint_runs_clean() {
        let (_dir, store) = make_store();
        {
            let conn = store.connection().expect("conn");
            for i in 0..50 {
                conn.execute(
                    "INSERT INTO engine_user_states (user_id, state_json, created_at)
                     VALUES (?1, '{}', '2026-01-01T00:00:00Z')",
                    rusqlite::params![format!("u{i}")],
                )
                .expect("insert");
            }
        }
        run(&store).await;
        // checkpoint_truncate 可独立再调一次也不报错
        store.checkpoint_truncate().expect("checkpoint should not fail");
    }
}
