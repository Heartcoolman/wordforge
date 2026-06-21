//! B69: Daily aggregation (1:00 AM)

use crate::store::Store;

pub async fn run(store: &Store) {
    tracing::info!("Daily aggregation worker running");
    let store = store.clone();
    match crate::blocking::run_blocking("worker.daily_aggregation", move || {
        let now = chrono::Utc::now();
        // 在 01:00 运行时聚合「前一整天」[昨天 00:00, 今天 00:00)，并以昨天日期入库，
        // 避免只统计到当天头一个小时（无上界会漏算前一天 23h+ 数据）。
        let today = now.format("%Y-%m-%d").to_string();
        let today_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
        let yesterday_start = today_start - chrono::Duration::days(1);
        // 聚合指标以「被聚合的那一天」（昨天）为 key。
        let agg_date = yesterday_start.format("%Y-%m-%d").to_string();

        let yesterday_start_utc =
            chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(yesterday_start, chrono::Utc);
        let today_start_utc =
            chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(today_start, chrono::Utc);

        let (total_records, total_correct, unique_users_count, unique_words_count) =
            match store.daily_aggregation_stats(yesterday_start_utc, today_start_utc) {
                Ok(stats) => stats,
                Err(e) => {
                    tracing::warn!(error = %e, "Daily aggregation: failed to query stats");
                    // m037 软拦截:统计失败会连带跳过下方 ELO/阶段快照,主动告警 admin。
                    let _ = store.record_system_alert(
                        "amas.daily_aggregation",
                        "stats_failed",
                        "error",
                        "AMAS 日聚合统计失败",
                        &e.to_string(),
                    );
                    return;
                }
            };

        let metrics = serde_json::json!({
            "date": agg_date,
            "totalRecords": total_records,
            "totalCorrect": total_correct,
            "uniqueUsers": unique_users_count,
            "uniqueWords": unique_words_count,
            "accuracy": if total_records > 0 { total_correct as f64 / total_records as f64 } else { 0.0 },
        });

        if let Err(e) = store.upsert_metrics_daily(&agg_date, "daily_aggregation", &metrics) {
            tracing::warn!(error = %e, "Failed to store daily aggregation metrics");
        }

        // AMAS 看板：ELO 日快照（散点 7d Δ 着色）+ 阶段分布日快照（趋势线）
        if let Err(e) = store.snapshot_user_elo_daily(&today) {
            tracing::warn!(error = %e, "Failed to snapshot user_elo_history");
            let _ = store.record_system_alert(
                "amas.daily_aggregation",
                "elo_snapshot_failed",
                "warning",
                "AMAS ELO 日快照失败",
                &e.to_string(),
            );
        }
        if let Err(e) = store.snapshot_amas_stage_daily(&today) {
            tracing::warn!(error = %e, "Failed to snapshot amas stage distribution");
            let _ = store.record_system_alert(
                "amas.daily_aggregation",
                "stage_snapshot_failed",
                "warning",
                "AMAS 阶段分布日快照失败",
                &e.to_string(),
            );
        }

        tracing::info!(
            date = %agg_date,
            records = total_records,
            users = unique_users_count,
            "Daily aggregation complete"
        );
    })
    .await
    {
        Ok(()) => {}
        Err(e) => tracing::warn!(error = %e, "Daily aggregation task failed"),
    }
}
