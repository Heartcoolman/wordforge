//! B44: Forgetting alert worker
//! Daily scan for words at high forgetting risk, generate notifications.
//! 在发送通知前检查 48 小时内是否已有同类通知（去重）

use crate::constants::MILLIS_PER_HOUR;
use crate::store::Store;
use std::collections::HashSet;

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

        let mut recent_alert_word_ids: Option<HashSet<String>> = None;

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

            let recent_word_ids = recent_alert_word_ids
                .get_or_insert_with(|| recent_alert_word_ids_in_window(store, user_id, cutoff));
            if recent_word_ids.contains(word_id.as_str()) {
                skipped_dedup += 1;
                continue;
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

            let key = match crate::store::keys::notification_key(
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
                .insert(key.as_bytes(), notification_bytes)
            {
                tracing::warn!(error = %e, "Failed to insert forgetting alert notification");
                continue;
            }

            recent_word_ids.insert(word_id);
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

/// 收集指定用户在去重窗口内已发送 forgetting_alert 的 word_id 集合。
fn recent_alert_word_ids_in_window(
    store: &Store,
    user_id: &str,
    cutoff: chrono::DateTime<chrono::Utc>,
) -> HashSet<String> {
    let prefix = match crate::store::keys::notification_prefix(user_id) {
        Ok(p) => p,
        Err(_) => return HashSet::new(),
    };
    let mut word_ids = HashSet::new();

    for item in store.notifications.scan_prefix(prefix.as_bytes()) {
        let (_, v) = match item {
            Ok(kv) => kv,
            Err(_) => continue,
        };

        let notif: serde_json::Value = match serde_json::from_slice(&v) {
            Ok(n) => n,
            Err(_) => continue,
        };

        if notif.get("type").and_then(|t| t.as_str()) != Some("forgetting_alert") {
            continue;
        }

        let Some(created_str) = notif.get("createdAt").and_then(|c| c.as_str()) else {
            continue;
        };
        let Ok(created) = chrono::DateTime::parse_from_rfc3339(created_str) else {
            continue;
        };

        if created.with_timezone(&chrono::Utc) < cutoff {
            continue;
        }

        if let Some(word_id) = notif.get("wordId").and_then(|w| w.as_str()) {
            word_ids.insert(word_id.to_string());
        }
    }

    word_ids
}
