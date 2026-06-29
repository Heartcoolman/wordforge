//! 选词拒绝原因小时滚动桶持久化。
//!
//! 内存 `rejection_metrics::REJECTION_REGISTRY` 的计数重启清零；metrics_flush worker
//! 周期 drain 增量 upsert 进 `rejections_rollup` 表（PK=hour_key+reason），admin 监控页
//! 可按小时聚合拒绝分布。仿 `operations/availability.rs::flush_availability_rollup`，
//! 区别：拒绝计数是「per-process drain 增量」，故 upsert 用 `count = count + excluded.count`
//! 累加（而非可用率桶的整桶覆盖）。

use rusqlite::params;

use crate::store::{Store, StoreError};

impl Store {
    /// 批量增量 upsert 拒绝小时桶 + 裁剪 `keep_from_hour` 之前的旧桶（单事务）。
    /// `rows`：(hour_key, reason, delta)。delta 为本次 drain 的增量，按 PK 冲突累加。
    pub fn flush_rejections_rollup(
        &self,
        rows: &[(i64, String, u64)],
        keep_from_hour: i64,
    ) -> Result<(), StoreError> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO rejections_rollup (hour_key, reason, count)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(hour_key, reason) DO UPDATE SET
                     count = count + excluded.count",
            )?;
            for (hour_key, reason, delta) in rows {
                stmt.execute(params![hour_key, reason, *delta as i64])?;
            }
        }
        tx.execute(
            "DELETE FROM rejections_rollup WHERE hour_key < ?1",
            params![keep_from_hour],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// admin 监控页数据源：返回 `from_hour` 起的拒绝小时桶（hour_key 升序、reason 升序）。
    /// 输出 camelCase JSON：hourKey / reason / count。
    pub fn list_rejections_rollup(
        &self,
        from_hour: i64,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT hour_key, reason, count FROM rejections_rollup
             WHERE hour_key >= ?1 ORDER BY hour_key ASC, reason ASC",
        )?;
        let mapped = stmt.query_map(params![from_hour], |r| {
            Ok(serde_json::json!({
                "hourKey": r.get::<_, i64>(0)?,
                "reason": r.get::<_, String>(1)?,
                "count": r.get::<_, i64>(2)?,
            }))
        })?;
        let mut out = Vec::new();
        for row in mapped {
            out.push(row?);
        }
        Ok(out)
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
    fn flush_increments_on_conflict() {
        let s = store();
        s.flush_rejections_rollup(&[(100, "not_top_k".into(), 5)], 0)
            .unwrap();
        // 同 (hour_key, reason) 再 flush → 累加而非覆盖
        s.flush_rejections_rollup(&[(100, "not_top_k".into(), 3)], 0)
            .unwrap();
        let rows = s.list_rejections_rollup(0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["hourKey"], 100);
        assert_eq!(rows[0]["reason"], "not_top_k");
        assert_eq!(rows[0]["count"], 8);
    }

    #[test]
    fn flush_prunes_old_buckets_and_orders_output() {
        let s = store();
        s.flush_rejections_rollup(
            &[
                (100, "word_not_found".into(), 1),
                (100, "confusion_dampened".into(), 2),
                (200, "not_top_k".into(), 3),
            ],
            0,
        )
        .unwrap();
        // prune <150：删 hour_key=100 两行，留 200
        s.flush_rejections_rollup(&[], 150).unwrap();
        let rows = s.list_rejections_rollup(0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["hourKey"], 200);
        assert_eq!(rows[0]["reason"], "not_top_k");
    }

    #[test]
    fn list_filters_by_from_hour_and_sorts_reason() {
        let s = store();
        s.flush_rejections_rollup(
            &[
                (300, "not_top_k".into(), 1),
                (300, "confusion_dampened".into(), 1),
                (100, "word_not_found".into(), 1),
            ],
            0,
        )
        .unwrap();
        let rows = s.list_rejections_rollup(200).unwrap();
        assert_eq!(rows.len(), 2);
        // hour_key=100 被 from_hour 过滤；剩 hour_key=300 两行按 reason 升序
        assert_eq!(rows[0]["reason"], "confusion_dampened");
        assert_eq!(rows[1]["reason"], "not_top_k");
    }
}
