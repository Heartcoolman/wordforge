//! B69: Daily aggregation (1:00 AM)
//! 利用 record_key 的时间戳特性，只扫描当天范围的记录
//!
//! 时区说明：当前所有时间计算均使用 UTC。如果未来需要支持用户本地时区，
//! 应从配置或用户设置中读取时区偏移，并据此计算"今天"的起止时间。

use crate::store::Store;

pub async fn run(store: &Store) {
    tracing::info!("Daily aggregation worker running");

    let now = chrono::Utc::now();
    let today = now.format("%Y-%m-%d").to_string();

    // 计算今天 00:00:00 UTC 的时间戳
    let today_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
    let today_start_utc =
        chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(today_start, chrono::Utc);

    let mut total_records = 0u64;
    let mut total_correct = 0u64;
    let mut unique_users = std::collections::HashSet::new();
    let mut unique_words = std::collections::HashSet::new();

    // 只需要 user_id，避免反序列化完整 User 对象
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
            let (_, v) = match item {
                Ok(kv) => kv,
                Err(_) => continue,
            };

            let record: crate::store::operations::records::LearningRecord =
                match serde_json::from_slice(&v) {
                    Ok(r) => r,
                    Err(_) => continue,
                };

            // record_key 按时间倒序排列，新记录在前
            // 如果遇到今天之前的记录，可以提前退出该用户的扫描
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
