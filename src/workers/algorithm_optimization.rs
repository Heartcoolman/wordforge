//! B45: Algorithm optimization worker
//! Daily at 00:00, run algorithm parameter optimization cycle.
//! 只扫描近 24 小时的记录而非全表

use std::sync::Arc;

use crate::amas::engine::AMASEngine;
use crate::store::Store;

#[derive(serde::Deserialize)]
struct RecordCorrectOnly {
    is_correct: bool,
}

#[derive(serde::Deserialize)]
struct RecordWithCreatedAt {
    is_correct: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

fn parse_record_timestamp_ms(record_key: &[u8]) -> Option<i64> {
    let first_sep = record_key.iter().position(|b| *b == b':')?;
    let tail = &record_key[first_sep + 1..];
    let second_sep = tail.iter().position(|b| *b == b':')?;
    let reverse_ts_bytes = &tail[..second_sep];

    let reverse_ts_str = std::str::from_utf8(reverse_ts_bytes).ok()?;
    let reverse_ts = reverse_ts_str.parse::<u64>().ok()?;
    let ts_u64 = u64::MAX.checked_sub(reverse_ts)?;

    i64::try_from(ts_u64).ok()
}

pub async fn run(store: &Store, _engine: &Arc<AMASEngine>) {
    tracing::info!("Algorithm optimization worker running");

    let now = chrono::Utc::now();
    let yesterday = now - chrono::Duration::days(1);
    let cutoff_ms = yesterday.timestamp_millis();

    let mut total_records = 0u64;
    let mut total_correct = 0u64;

    // 按用户前缀扫描，利用 record_key 的时间倒序特性只看近期记录。
    // 这里仅拉取 user_id，避免 list_users 的全量反序列化和排序开销。
    let user_ids = match store.list_user_ids() {
        Ok(ids) => ids,
        Err(e) => {
            tracing::warn!(error = %e, "Algorithm optimization: failed to list user IDs");
            return;
        }
    };

    for user_id in &user_ids {
        let prefix = match crate::store::keys::record_prefix(user_id) {
            Ok(p) => p,
            Err(_) => continue,
        };
        for item in store.records.scan_prefix(prefix.as_bytes()) {
            let (k, v) = match item {
                Ok(kv) => kv,
                Err(_) => continue,
            };

            // record_key 按时间倒序，新记录在前；遇到超过 24 小时前的记录即停止。
            // 优先基于 key 中的时间戳短路，避免不必要的 JSON 反序列化。
            if let Some(ts_ms) = parse_record_timestamp_ms(&k) {
                if ts_ms < cutoff_ms {
                    break;
                }

                let record: RecordCorrectOnly = match serde_json::from_slice(&v) {
                    Ok(r) => r,
                    Err(_) => continue,
                };

                total_records += 1;
                if record.is_correct {
                    total_correct += 1;
                }
                continue;
            }

            let record: RecordWithCreatedAt = match serde_json::from_slice(&v) {
                Ok(r) => r,
                Err(_) => continue,
            };

            if record.created_at < yesterday {
                break;
            }

            total_records += 1;
            if record.is_correct {
                total_correct += 1;
            }
        }
    }

    let overall_accuracy = if total_records > 0 {
        total_correct as f64 / total_records as f64
    } else {
        0.0
    };

    tracing::info!(
        total_records,
        total_correct,
        overall_accuracy = format!("{:.3}", overall_accuracy),
        "Algorithm optimization: collected aggregate statistics"
    );

    let date = now.format("%Y-%m-%d").to_string();
    let metrics = serde_json::json!({
        "date": date,
        "totalRecords": total_records,
        "totalCorrect": total_correct,
        "overallAccuracy": overall_accuracy,
        "optimizationRun": true,
    });

    if let Err(e) = store.upsert_metrics_daily(&date, "optimization", &metrics) {
        tracing::warn!(error = %e, "Failed to store optimization metrics");
    }
}
