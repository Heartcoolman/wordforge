//! B45: Algorithm optimization worker
//! Daily at 00:00, run algorithm parameter optimization cycle.

use std::sync::Arc;

use crate::amas::engine::AMASEngine;
use crate::store::Store;

pub async fn run(store: &Store, engine: &Arc<AMASEngine>) {
    tracing::info!("Algorithm optimization worker running");

    let now = chrono::Utc::now();
    let yesterday = now - chrono::Duration::days(1);
    let store_for_aggregate = store.clone();

    let (total_records, total_correct) = match crate::blocking::run_blocking(
        "worker.algorithm_optimization.aggregate_records",
        move || store_for_aggregate.aggregate_records_since(yesterday),
    )
    .await
    {
        Ok(Ok(stats)) => stats,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "Algorithm optimization: failed to aggregate records");
            return;
        }
        Err(e) => {
            tracing::warn!(error = %e, "Algorithm optimization aggregate task failed");
            return;
        }
    };

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

    let store_for_metrics = store.clone();
    match crate::blocking::run_blocking("worker.algorithm_optimization.store_metrics", move || {
        store_for_metrics.upsert_metrics_daily(&date, "optimization", &metrics)
    })
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => tracing::warn!(error = %e, "Failed to store optimization metrics"),
        Err(e) => tracing::warn!(error = %e, "Algorithm optimization metrics task failed"),
    }
}
