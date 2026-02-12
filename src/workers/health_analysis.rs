//! B70: Health analysis (weekly Monday 5:00)
//! 分页加载用户，每批 100 个

use crate::store::Store;

const USER_BATCH_SIZE: usize = 100;
/// 每个用户读取的最大记录数
const MAX_RECORDS_PER_USER: usize = 100;
/// 正确率低于此阈值视为 at-risk
const AT_RISK_ACCURACY_THRESHOLD: f64 = 0.3;

pub async fn run(store: &Store) {
    tracing::info!("Weekly health analysis worker running");

    let week_ago = chrono::Utc::now() - chrono::Duration::days(7);

    let mut total_users = 0u32;
    let mut active_users = 0u32;
    let mut at_risk_users = 0u32;

    let mut offset = 0usize;
    loop {
        let users = match store.list_users(USER_BATCH_SIZE, offset) {
            Ok(u) => u,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to list users for health analysis");
                return;
            }
        };

        if users.is_empty() {
            break;
        }

        let batch_len = users.len();

        for user in &users {
            total_users += 1;
            let records = store.get_user_records(&user.id, MAX_RECORDS_PER_USER).unwrap_or_default();

            let recent = records.iter().filter(|r| r.created_at > week_ago).count();

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

            if recent == 0 || recent_accuracy < AT_RISK_ACCURACY_THRESHOLD {
                at_risk_users += 1;
            }
        }

        offset += batch_len;

        if batch_len < USER_BATCH_SIZE {
            break;
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
