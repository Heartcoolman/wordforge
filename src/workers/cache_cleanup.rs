//! B68: AMAS cache cleanup (every 10 minutes)
//! 边扫描边删除，限制单次最多删除 10000 条

use crate::store::Store;

/// 单次清理最多删除的条目数
const MAX_REMOVALS_PER_RUN: u32 = 10_000;

pub async fn run(store: &Store) {
    tracing::debug!("AMAS cache cleanup worker tick");

    // Clean up expired monitoring events (older than 7 days)
    let cutoff = chrono::Utc::now() - chrono::Duration::days(7);
    let cutoff_ms = cutoff.timestamp_millis();
    let mut removed = 0u32;

    // 边扫描边删除，避免先收集所有 key 再批量删除
    for item in store.engine_monitoring_events.iter() {
        if removed >= MAX_REMOVALS_PER_RUN {
            tracing::info!(
                removed,
                "Cache cleanup: reached single-run limit, remaining items deferred to next run"
            );
            break;
        }

        let (k, v) = match item {
            Ok(kv) => kv,
            Err(_) => continue,
        };

        if let Ok(event) = serde_json::from_slice::<serde_json::Value>(&v) {
            let Some(event_ts) = event
                .get("timestamp")
                .and_then(|v| v.as_str())
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.timestamp_millis())
            else {
                tracing::debug!("Cache cleanup: skip event with invalid timestamp");
                continue;
            };

            if event_ts < cutoff_ms {
                if let Ok(()) = store
                    .engine_monitoring_events
                    .remove(k.as_ref())
                    .map(|_| ())
                {
                    removed += 1;
                }
            }
        }
    }

    if removed > 0 {
        tracing::info!(removed, "Cache cleanup: removed old monitoring events");
    }
}
