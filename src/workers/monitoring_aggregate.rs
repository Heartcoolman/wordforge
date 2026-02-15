//! Monitoring aggregation worker.
//! 定期聚合系统指标（请求延迟、错误率、活跃用户数），写入时序数据。

use crate::store::Store;

pub async fn run(store: &Store) {
    tracing::debug!("monitoring_aggregate: start");

    let now = chrono::Utc::now();
    let period_start = now - chrono::Duration::minutes(5);

    let events = match store.get_recent_monitoring_events(1000) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(error = %e, "monitoring_aggregate: failed to read events");
            return;
        }
    };

    let mut latencies: Vec<i64> = Vec::new();
    let mut error_count = 0u64;
    let mut total_count = 0u64;
    let mut users = std::collections::HashSet::new();

    for event in &events {
        let ts = event
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok());
        let Some(ts) = ts else { continue };
        if ts.with_timezone(&chrono::Utc) < period_start {
            continue;
        }

        total_count += 1;

        if let Some(lat) = event.get("latencyMs").and_then(|v| v.as_i64()) {
            latencies.push(lat);
        }

        if event.get("isAnomaly").and_then(|v| v.as_bool()) == Some(true) {
            error_count += 1;
        }

        if let Some(uid) = event.get("userId").and_then(|v| v.as_str()) {
            users.insert(uid.to_string());
        }
    }

    latencies.sort_unstable();

    let (p50, p95, p99) = if latencies.is_empty() {
        (0.0, 0.0, 0.0)
    } else {
        let p = |pct: f64| -> f64 {
            let idx = ((pct / 100.0) * (latencies.len() as f64 - 1.0)).round() as usize;
            latencies[idx.min(latencies.len() - 1)] as f64
        };
        (p(50.0), p(95.0), p(99.0))
    };

    let error_rate = if total_count > 0 {
        error_count as f64 / total_count as f64
    } else {
        0.0
    };

    let active_users = match store.count_active_users_since(period_start) {
        Ok(n) => n,
        Err(_) => users.len(),
    };

    let period_id = now.format("%Y%m%d%H%M").to_string();
    let aggregate = serde_json::json!({
        "periodStart": period_start.to_rfc3339(),
        "periodEnd": now.to_rfc3339(),
        "totalEvents": total_count,
        "errorCount": error_count,
        "errorRate": error_rate,
        "latencyP50": p50,
        "latencyP95": p95,
        "latencyP99": p99,
        "activeUsers": active_users,
    });

    let ts_key = match crate::store::keys::monitoring_ts_key(now.timestamp_millis(), &period_id) {
        Ok(k) => k,
        Err(e) => {
            tracing::warn!(error = %e, "monitoring_aggregate: failed to build key");
            return;
        }
    };

    if let Err(e) = store.monitoring_timeseries.insert(
        ts_key.as_bytes(),
        match serde_json::to_vec(&aggregate) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(error = %e, "monitoring_aggregate: failed to serialize");
                return;
            }
        },
    ) {
        tracing::warn!(error = %e, "monitoring_aggregate: failed to store");
    }

    tracing::debug!(
        total_count,
        error_count,
        active_users,
        "monitoring_aggregate: done"
    );
}
