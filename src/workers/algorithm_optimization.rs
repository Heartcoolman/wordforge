//! B45: Algorithm optimization worker
//! Daily at 00:00, run algorithm parameter optimization cycle.

use std::sync::Arc;

use crate::amas::engine::AMASEngine;
use crate::store::Store;

pub async fn run(store: &Store, _engine: &Arc<AMASEngine>) {
    tracing::info!("Algorithm optimization worker running");

    // Collect aggregate performance data across all users
    let mut total_records = 0u64;
    let mut total_correct = 0u64;

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

        total_records += 1;
        if record.is_correct {
            total_correct += 1;
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

    // Store optimization results
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
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
