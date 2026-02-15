//! B45: Algorithm optimization worker
//! Daily at 00:00, run algorithm parameter optimization cycle.
//! 只扫描近 24 小时的记录而非全表

use std::sync::Arc;

use crate::amas::engine::AMASEngine;
use crate::store::Store;

use super::parse_record_timestamp_ms;

#[derive(serde::Deserialize)]
struct RecordCorrectOnly {
    is_correct: bool,
}

#[derive(serde::Deserialize)]
struct RecordWithCreatedAt {
    is_correct: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn run(store: &Store, engine: &Arc<AMASEngine>) {
    tracing::info!("Algorithm optimization worker running");

    let now = chrono::Utc::now();
    let yesterday = now - chrono::Duration::days(1);
    let cutoff_ms = yesterday.timestamp_millis();

    let mut total_records = 0u64;
    let mut total_correct = 0u64;

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

    // E4: Simple parameter adjustment based on overall accuracy
    if total_records >= 50 {
        let mut config = engine.get_config().await;
        let mut adjusted = false;

        if overall_accuracy < 0.4 {
            let old = config.constraints.max_difficulty_when_fatigued;
            config.constraints.max_difficulty_when_fatigued = (old - 0.05).max(0.2);
            if (config.constraints.max_difficulty_when_fatigued - old).abs() > f64::EPSILON {
                tracing::info!(
                    old = format!("{:.3}", old),
                    new = format!("{:.3}", config.constraints.max_difficulty_when_fatigued),
                    "Algorithm optimization: lowered max_difficulty_when_fatigued due to low accuracy"
                );
                adjusted = true;
            }
        }

        if overall_accuracy > 0.85 {
            let old = config.constraints.max_difficulty_when_fatigued;
            config.constraints.max_difficulty_when_fatigued = (old + 0.03).min(0.9);
            if (config.constraints.max_difficulty_when_fatigued - old).abs() > f64::EPSILON {
                tracing::info!(
                    old = format!("{:.3}", old),
                    new = format!("{:.3}", config.constraints.max_difficulty_when_fatigued),
                    "Algorithm optimization: raised max_difficulty_when_fatigued due to high accuracy"
                );
                adjusted = true;
            }
        }

        if adjusted {
            if let Err(e) = engine.reload_config(config).await {
                tracing::warn!(error = %e, "Algorithm optimization: failed to update config");
            }
        }
    }

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
