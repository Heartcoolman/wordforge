//! B75: Weekly report (Monday 6:30)

use crate::store::Store;

/// 每批获取用户数
const USER_BATCH_SIZE: usize = 500;
/// 每个用户读取的最大记录数
const MAX_RECORDS_PER_USER: usize = 10_000;

pub async fn run(store: &Store) {
    tracing::info!("Weekly report worker running");

    let now = chrono::Utc::now();
    let week_ago = now - chrono::Duration::days(7);

    let mut weekly_active = 0u32;
    let mut total_records_week = 0u64;
    let mut total_correct_week = 0u64;
    let mut total_users = 0u64;

    // 分批获取用户，流式计算统计值
    let mut offset = 0usize;
    loop {
        let users = store
            .list_users(USER_BATCH_SIZE, offset)
            .unwrap_or_default();

        if users.is_empty() {
            break;
        }

        let batch_len = users.len();
        total_users += batch_len as u64;

        for user in &users {
            // 利用 record_key 的时间倒序特性，只读取近期记录
            // 一旦遇到 week_ago 之前的记录立即停止扫描
            let records = store.get_user_records(&user.id, MAX_RECORDS_PER_USER).unwrap_or_default();
            let mut has_weekly = false;

            for r in &records {
                if r.created_at < week_ago {
                    break; // record_key 按时间倒序，后续记录更早，无需继续
                }
                has_weekly = true;
                total_records_week += 1;
                if r.is_correct {
                    total_correct_week += 1;
                }
            }

            if has_weekly {
                weekly_active += 1;
            }
        }

        offset += batch_len;

        if batch_len < USER_BATCH_SIZE {
            break;
        }
    }

    let date = now.format("%Y-%m-%d").to_string();
    let report = serde_json::json!({
        "weekEnding": date,
        "totalUsers": total_users,
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
