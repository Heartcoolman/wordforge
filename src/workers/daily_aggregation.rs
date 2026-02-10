//! B69: Daily aggregation (1:00 AM)

use crate::store::Store;

pub async fn run(store: &Store) {
    tracing::info!("Daily aggregation worker running");

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let mut total_records = 0u64;
    let mut total_correct = 0u64;
    let mut unique_users = std::collections::HashSet::new();
    let mut unique_words = std::collections::HashSet::new();

    for item in store.records.iter() {
        let (_, v) = match item {
            Ok(kv) => kv,
            Err(_) => continue,
        };

        let record: crate::store::operations::records::LearningRecord =
            match serde_json::from_slice(&v) {
                Ok(r) => r,
                Err(_) => continue,
            };

        if record.created_at.format("%Y-%m-%d").to_string() == today {
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
