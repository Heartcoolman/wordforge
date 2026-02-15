//! B69: Daily aggregation (1:00 AM)
//! 利用 record_key 的时间戳特性，只扫描当天范围的记录

use crate::store::Store;

use super::parse_record_timestamp_ms;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecordCorrectOnly {
    is_correct: bool,
}

pub async fn run(store: &Store) {
    tracing::info!("Daily aggregation worker running");

    let now = chrono::Utc::now();
    let today = now.format("%Y-%m-%d").to_string();

    let today_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
    let today_start_utc =
        chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(today_start, chrono::Utc);
    let cutoff_ms = today_start_utc.timestamp_millis();

    let mut total_records = 0u64;
    let mut total_correct = 0u64;
    let mut unique_users = std::collections::HashSet::new();
    let mut unique_words = std::collections::HashSet::new();

    let user_ids = match store.list_user_ids() {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!(error = %e, "Daily aggregation: failed to list users");
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
                unique_users.insert(user_id.clone());
                continue;
            }

            let record: crate::store::operations::records::LearningRecord =
                match serde_json::from_slice(&v) {
                    Ok(r) => r,
                    Err(_) => continue,
                };

            if record.created_at < today_start_utc {
                break;
            }

            total_records += 1;
            if record.is_correct {
                total_correct += 1;
            }
            unique_users.insert(record.user_id.clone());
            unique_words.insert(record.word_id.clone());
        }
    }

    let metrics = serde_json::json!({
        "date": today,
        "totalRecords": total_records,
        "totalCorrect": total_correct,
        "uniqueUsers": unique_users.len(),
        "uniqueWords": unique_words.len(),
        "accuracy": if total_records > 0 { total_correct as f64 / total_records as f64 } else { 0.0 },
    });

    if let Err(e) = store.upsert_metrics_daily(&today, "daily_aggregation", &metrics) {
        tracing::warn!(error = %e, "Failed to store daily aggregation metrics");
    }

    tracing::info!(
        date = %today,
        records = total_records,
        users = unique_users.len(),
        "Daily aggregation complete"
    );
}
