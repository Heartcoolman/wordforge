//! B75: Weekly report (Monday 6:00)

use crate::store::Store;

pub async fn run(store: &Store) {
    tracing::info!("Weekly report worker running");

    let now = chrono::Utc::now();
    let week_ago = now - chrono::Duration::days(7);

    let users = store.list_users(usize::MAX, 0).unwrap_or_default();

    let mut weekly_active = 0u32;
    let mut total_records_week = 0u64;
    let mut total_correct_week = 0u64;

    for user in &users {
        let records = store.get_user_records(&user.id, 10000).unwrap_or_default();
        let weekly_records: Vec<_> = records
            .iter()
            .filter(|r| r.created_at >= week_ago)
            .collect();

        if !weekly_records.is_empty() {
            weekly_active += 1;
        }

        for r in &weekly_records {
            total_records_week += 1;
            if r.is_correct {
                total_correct_week += 1;
            }
        }
    }

    let date = now.format("%Y-%m-%d").to_string();
    let report = serde_json::json!({
        "weekEnding": date,
        "totalUsers": users.len(),
        "weeklyActiveUsers": weekly_active,
        "totalRecordsThisWeek": total_records_week,
        "weeklyAccuracy": if total_records_week > 0 {
            total_correct_week as f64 / total_records_week as f64
        } else { 0.0 },
    });

    if let Err(e) = store.upsert_metrics_daily(&date, "weekly_report", &report) {
        tracing::warn!(error = %e, "Failed to store weekly report");
    }

    tracing::info!(
        active = weekly_active,
        records = total_records_week,
        "Weekly report generated"
    );
}
