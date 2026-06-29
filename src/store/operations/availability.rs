//! D3：HTTP 可用率小时滚动桶持久化。
//!
//! 内存 `http_metrics::RollingStore` 的 hour 层重启清零，登录页「SLO 30d」永远到不了
//! 满 30 天。本模块把 hour 桶按 `availability_rollup` 表持久化：metrics_flush worker
//! 每 5 分钟 flush，进程启动回灌内存 hour deque，从而 30d 可用率窗口跨重启可达。

use rusqlite::params;

use crate::store::{Store, StoreError};

/// 可用率小时桶行：(hour_key, count, err5xx, bytes_in, buckets)。
type AvailabilityRow = (i64, u64, u64, u64, Vec<u64>);

impl Store {
    /// 批量 upsert 可用率小时桶 + 裁剪 `keep_from_hour` 之前的旧桶（单事务）。
    /// 桶量上界 = HOUR_CAP（30×24=720），每次 flush 全量重写当前内存态，幂等。
    pub fn flush_availability_rollup(
        &self,
        rows: &[(i64, u64, u64, u64, String)],
        keep_from_hour: i64,
    ) -> Result<(), StoreError> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO availability_rollup (hour_key, count, err5xx, bytes_in, buckets)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(hour_key) DO UPDATE SET
                     count=excluded.count, err5xx=excluded.err5xx,
                     bytes_in=excluded.bytes_in, buckets=excluded.buckets",
            )?;
            for (hour_key, count, err5xx, bytes_in, buckets) in rows {
                stmt.execute(params![
                    hour_key,
                    *count as i64,
                    *err5xx as i64,
                    *bytes_in as i64,
                    buckets,
                ])?;
            }
        }
        tx.execute(
            "DELETE FROM availability_rollup WHERE hour_key < ?1",
            params![keep_from_hour],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// admin 监控页「每小时可用率」端点数据源：返回 `from_hour` 起的小时桶（升序）。
    /// 与启动回灌共用同一查询（`load_availability_rollup`），可用率由 handler 在
    /// Rust 侧按 `1 - err5xx/count` 计算。
    pub fn list_availability_hourly(
        &self,
        from_hour: i64,
    ) -> Result<Vec<AvailabilityRow>, StoreError> {
        self.load_availability_rollup(from_hour)
    }

    /// 加载 `from_hour` 起的可用率小时桶（升序），供启动回灌恢复 ≤30d 窗口。
    /// buckets JSON 反序列化失败回落空向量（import 侧按 BUCKET_BOUNDS 补齐长度）。
    pub fn load_availability_rollup(
        &self,
        from_hour: i64,
    ) -> Result<Vec<AvailabilityRow>, StoreError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT hour_key, count, err5xx, bytes_in, buckets FROM availability_rollup
             WHERE hour_key >= ?1 ORDER BY hour_key ASC",
        )?;
        let mapped = stmt.query_map(params![from_hour], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)? as u64,
                r.get::<_, i64>(2)? as u64,
                r.get::<_, i64>(3)? as u64,
                r.get::<_, String>(4)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in mapped {
            let (hour_key, count, err5xx, bytes_in, buckets_json) = row?;
            let buckets: Vec<u64> = serde_json::from_str(&buckets_json).unwrap_or_default();
            out.push((hour_key, count, err5xx, bytes_in, buckets));
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
    fn flush_then_load_roundtrip() {
        let s = store();
        let rows = vec![
            (
                100i64,
                50u64,
                1u64,
                4096u64,
                "[10,20,30,40,50,50]".to_string(),
            ),
            (
                101i64,
                80u64,
                0u64,
                8192u64,
                "[80,80,80,80,80,80]".to_string(),
            ),
        ];
        s.flush_availability_rollup(&rows, 0).unwrap();
        let loaded = s.load_availability_rollup(0).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].0, 100);
        assert_eq!(loaded[0].1, 50);
        assert_eq!(loaded[0].2, 1);
        assert_eq!(loaded[0].4, vec![10, 20, 30, 40, 50, 50]);
        assert_eq!(loaded[1].0, 101);
    }

    #[test]
    fn flush_upserts_and_prunes() {
        let s = store();
        s.flush_availability_rollup(
            &[(100, 10, 0, 0, "[]".into()), (200, 20, 0, 0, "[]".into())],
            0,
        )
        .unwrap();
        // 重 flush hour_key=100 改计数 + prune <150（删 100，留 200）
        s.flush_availability_rollup(&[(100, 99, 5, 0, "[]".into())], 150)
            .unwrap();
        let loaded = s.load_availability_rollup(0).unwrap();
        assert_eq!(loaded.len(), 1, "hour_key=100 应被 prune");
        assert_eq!(loaded[0].0, 200);
    }

    #[test]
    fn load_filters_by_from_hour() {
        let s = store();
        s.flush_availability_rollup(
            &[(100, 1, 0, 0, "[]".into()), (300, 3, 0, 0, "[]".into())],
            0,
        )
        .unwrap();
        let loaded = s.load_availability_rollup(200).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].0, 300);
    }
}
