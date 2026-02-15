//! B68: AMAS cache cleanup (every 10 minutes)
//! 边扫描边删除，限制单次最多删除 10000 条

use crate::store::Store;

use super::parse_monitoring_event_timestamp_ms;

/// 单次清理最多删除的条目数
const MAX_REMOVALS_PER_RUN: u32 = 10_000;

pub async fn run(store: &Store) {
    tracing::debug!("AMAS cache cleanup worker tick");

    let cutoff_ms = (chrono::Utc::now() - chrono::Duration::days(7)).timestamp_millis();
    let mut removed = 0u32;

    for item in store.engine_monitoring_events.iter() {
        if removed >= MAX_REMOVALS_PER_RUN {
            tracing::info!(
                removed,
                "Cache cleanup: reached single-run limit, remaining items deferred to next run"
            );
            break;
        }

        let (k, _) = match item {
            Ok(kv) => kv,
            Err(_) => continue,
        };

        let Some(event_ts) = parse_monitoring_event_timestamp_ms(&k) else {
            continue;
        };

        if event_ts < cutoff_ms {
            if store.engine_monitoring_events.remove(k.as_ref()).is_ok() {
                removed += 1;
            }
        }
    }

    if removed > 0 {
        tracing::info!(removed, "Cache cleanup: removed old monitoring events");
    }
}
