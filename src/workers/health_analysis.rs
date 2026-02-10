//! B70: Health analysis (weekly Monday 5:00)

use crate::store::Store;

pub async fn run(store: &Store) {
    tracing::info!("Weekly health analysis worker running");

    // Analyze user health metrics
    let mut total_users = 0u32;
    let mut active_users = 0u32;
    let mut at_risk_users = 0u32;

    // Check all users
    let users = match store.list_users(usize::MAX, 0) {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to list users for health analysis");
            return;
        }
    };

    let week_ago = chrono::Utc::now() - chrono::Duration::days(7);

    for user in &users {
        total_users += 1;
        let records = store.get_user_records(&user.id, 100).unwrap_or_default();

        let recent = records
            .iter()
            .filter(|r| r.created_at > week_ago)
            .count();

        if recent > 0 {
            active_users += 1;
        }

        let recent_accuracy = if recent > 0 {
            let correct = records
                .iter()
                .filter(|r| r.created_at > week_ago && r.is_correct)
                .count();
            correct as f64 / recent as f64
        } else {
            0.0
        };

        if recent == 0 || recent_accuracy < 0.3 {
            at_risk_users += 1;
        }
    }

    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let report = serde_json::json!({
        "date": date,
        "totalUsers": total_users,
        "activeUsers": active_users,
        "atRiskUsers": at_risk_users,
        "weeklyRetention": if total_users > 0 { active_users as f64 / total_users as f64 } else { 0.0 },
    });

    if let Err(e) = store.upsert_metrics_daily(&date, "health_analysis", &report) {
        tracing::warn!(error = %e, "Failed to store health analysis report");
    }

    tracing::info!(
        total = total_users,
        active = active_users,
        at_risk = at_risk_users,
        "Weekly health analysis complete"
    );
}
