use crate::amas::metrics::MetricsRegistry;
use crate::store::Store;

pub async fn run(registry: &MetricsRegistry, store: &Store) {
    tracing::debug!("metrics_flush: start");
    let snapshot = registry.snapshot_and_reset();
    let snapshot_for_flush = snapshot.clone();
    let store_for_flush = store.clone();
    match crate::blocking::run_blocking("worker.metrics_flush", move || {
        crate::amas::metrics_persistence::flush_metrics_snapshot(snapshot_for_flush, &store_for_flush)
    })
    .await
    {
        Ok(Ok(())) => tracing::debug!("metrics_flush: done"),
        // m037 软拦截:落库失败若仅 log,run 不抛错→调度框架记 success→admin 监控盲视。
        // 故主动告警 admin(无 user 上下文,只 admin)。
        Ok(Err(e)) => {
            // 落库失败把取走的计数加回 registry，避免该区间计数随 reset 永久丢失（reset-before-confirm）。
            registry.merge_snapshot(&snapshot);
            tracing::error!(error=%e, "metrics_flush failed");
            if let Err(ae) = store.record_system_alert(
                "amas.metrics_flush",
                "persist_failed",
                "error",
                "AMAS 指标落库失败",
                &e.to_string(),
            ) {
                tracing::warn!(error=%ae, "record_system_alert failed");
            }
        }
        Err(e) => {
            // 任务失败（snapshot 未必落库）同样加回计数，避免该区间永久丢失。
            registry.merge_snapshot(&snapshot);
            tracing::error!(error=%e, "metrics_flush task failed");
            if let Err(ae) = store.record_system_alert(
                "amas.metrics_flush",
                "task_failed",
                "error",
                "AMAS 指标落库任务失败",
                &e.to_string(),
            ) {
                tracing::warn!(error=%ae, "record_system_alert failed");
            }
        }
    }

    // D3：旁路 flush HTTP 可用率小时桶（与 AMAS 指标 flush 同 5 分钟周期）。
    flush_availability_rollup(store);
}

/// D3：导出内存 hour 槽 → 升序 upsert 到 availability_rollup + 裁剪超 30d 旧桶。失败仅告警。
/// W3-2：提升为 pub 供 main.rs 在 graceful shutdown（server_future.await 之后，不再有新请求
/// 写内存桶）补调一次，补落最近 ≤5min 增量，消除重启后登录 SLO 当前小时桶缺口。
/// export_hour_rollup 是只读 snapshot 非 drain，整桶覆盖式 upsert 幂等，与 cron tick 重复无害。
pub fn flush_availability_rollup(store: &Store) {
    let exported = crate::middleware::http_metrics::export_hour_rollup();
    if exported.is_empty() {
        return;
    }
    let rows: Vec<(i64, u64, u64, u64, String)> = exported
        .into_iter()
        .map(|(k, c, e, b, buckets)| {
            (
                k,
                c,
                e,
                b,
                serde_json::to_string(&buckets).unwrap_or_else(|_| "[]".into()),
            )
        })
        .collect();
    let now_hour = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64 / 3600)
        .unwrap_or(0);
    if let Err(e) = store.flush_availability_rollup(&rows, now_hour - 30 * 24) {
        tracing::warn!(error=%e, "availability rollup flush failed");
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::amas::types::AlgorithmId;

    #[tokio::test]
    async fn metrics_flush_persists_snapshot_via_blocking_task() {
        let dir = tempdir().expect("tempdir");
        let store = Store::open(
            dir.path().join("metrics-flush.db").to_str().unwrap(),
            5000,
            2,
        )
        .expect("open store");
        store.run_migrations().expect("run migrations");

        let registry = MetricsRegistry::new();
        registry.record_call(AlgorithmId::Heuristic, 120, false);
        registry.record_call(AlgorithmId::Heuristic, 80, true);

        run(&registry, &store).await;

        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let persisted = store
            .get_metrics_daily(&today, "heuristic")
            .expect("read flushed metric")
            .expect("metric should exist");

        // P0-1: MetricsSnapshot 序列化为 camelCase，存储读出字段名同步
        assert_eq!(persisted["callCount"], 2);
        assert_eq!(persisted["errorCount"], 1);
        assert_eq!(persisted["totalLatencyUs"], 200);
    }
}
