//! B44: Forgetting alert worker
//! Daily scan for words at high forgetting risk, generate notifications.
//! 使用 alert_dedup tree 进行 O(1) 去重检查

use crate::constants::MILLIS_PER_HOUR;
use crate::store::Store;

/// 通知去重窗口
const DEDUP_WINDOW_HOURS: i64 = 48;

pub async fn run(store: &Store) {
    tracing::info!("Forgetting alert worker running");

    let now = chrono::Utc::now();
    let dedup_window = chrono::Duration::hours(DEDUP_WINDOW_HOURS);
    let cutoff = now - dedup_window;
    let cutoff_ms = cutoff.timestamp_millis().max(0);
    let now_ms = now.timestamp_millis().max(0);
    let mut at_risk = 0u32;
    let mut skipped_dedup = 0u32;

    let user_ids = match store.list_user_ids() {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!(error = %e, "Forgetting alert: failed to list users");
            return;
        }
    };

    for user_id in &user_ids {
        let prefix = match crate::store::keys::word_due_index_prefix(user_id) {
            Ok(p) => p,
            Err(_) => continue,
        };

        for item in store.word_due_index.scan_prefix(prefix.as_bytes()) {
            let (key, _) = match item {
                Ok(kv) => kv,
                Err(e) => {
                    tracing::warn!(error = %e, "Error scanning word_due_index");
                    continue;
                }
            };

            let Some((due_ts_ms, word_id)) = crate::store::keys::parse_due_index_item_key(&key)
            else {
                continue;
            };

            if due_ts_ms > cutoff_ms {
                break;
            }

            let state = match store.get_word_learning_state(user_id, &word_id) {
                Ok(Some(s)) => s,
                Ok(None) => continue,
                Err(e) => {
                    tracing::warn!(error = %e, "Forgetting alert: failed to read word state");
                    continue;
                }
            };

            let Some(review_date) = state.next_review_date else {
                continue;
            };

            let review_ts_ms = review_date.timestamp_millis().max(0);
            if review_ts_ms != due_ts_ms
                || state.state == crate::store::operations::word_states::WordState::Mastered
            {
                continue;
            }

            let dedup_key = match crate::store::keys::alert_dedup_key(user_id, &word_id) {
                Ok(k) => k,
                Err(_) => continue,
            };
            if let Ok(Some(ts_bytes)) = store.alert_dedup.get(dedup_key.as_bytes()) {
                if let Ok(ts_str) = std::str::from_utf8(&ts_bytes) {
                    if let Ok(prev_ms) = ts_str.parse::<i64>() {
                        if prev_ms >= cutoff_ms {
                            skipped_dedup += 1;
                            continue;
                        }
                    }
                }
            }

            let overdue_hours = now_ms.saturating_sub(due_ts_ms) / MILLIS_PER_HOUR;
            let notification = serde_json::json!({
                "id": uuid::Uuid::new_v4().to_string(),
                "userId": user_id,
                "type": "forgetting_alert",
                "wordId": word_id,
                "overdueHours": overdue_hours,
                "createdAt": now.to_rfc3339(),
                "read": false,
            });

            let notif_key = match crate::store::keys::notification_key(
                user_id,
                notification["id"].as_str().unwrap_or("unknown"),
            ) {
                Ok(k) => k,
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to build notification key");
                    continue;
                }
            };

            let notification_bytes = match serde_json::to_vec(&notification) {
                Ok(bytes) => bytes,
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to serialize forgetting alert notification");
                    continue;
                }
            };

            if let Err(e) = store
                .notifications
                .insert(notif_key.as_bytes(), notification_bytes)
            {
                tracing::warn!(error = %e, "Failed to insert forgetting alert notification");
                continue;
            }

            let _ = store
                .alert_dedup
                .insert(dedup_key.as_bytes(), now_ms.to_string().as_bytes());
            at_risk += 1;
        }
    }

    if skipped_dedup > 0 {
        tracing::info!(
            skipped_dedup,
            "Forgetting alert: skipped duplicate notifications"
        );
    }
    tracing::info!(at_risk, "Forgetting alert: found at-risk words");
}
