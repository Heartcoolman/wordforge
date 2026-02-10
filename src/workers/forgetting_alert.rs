//! B44: Forgetting alert worker
//! Daily scan for words at high forgetting risk, generate notifications.

use crate::store::Store;

pub async fn run(store: &Store) {
    tracing::info!("Forgetting alert worker running");

    let now = chrono::Utc::now();
    let mut at_risk = 0u32;

    for item in store.word_learning_states.iter() {
        let (_, v) = match item {
            Ok(kv) => kv,
            Err(e) => {
                tracing::warn!(error = %e, "Error scanning word_learning_states");
                continue;
            }
        };

        let wls: crate::store::operations::word_states::WordLearningState =
            match serde_json::from_slice(&v) {
                Ok(s) => s,
                Err(_) => continue,
            };

        // Check if word is at high forgetting risk
        if let Some(review_date) = wls.next_review_date {
            let overdue_hours = (now - review_date).num_hours();
            if overdue_hours > 48
                && wls.state != crate::store::operations::word_states::WordState::Mastered
            {
                at_risk += 1;

                // Generate a notification for high-risk words
                let notification = serde_json::json!({
                    "id": uuid::Uuid::new_v4().to_string(),
                    "userId": wls.user_id,
                    "type": "forgetting_alert",
                    "wordId": wls.word_id,
                    "overdueHours": overdue_hours,
                    "createdAt": now.to_rfc3339(),
                    "read": false,
                });

                let key = crate::store::keys::notification_key(
                    &wls.user_id,
                    notification["id"].as_str().unwrap_or("unknown"),
                );
                if let Err(e) = store.notifications.insert(
                    key.as_bytes(),
                    match serde_json::to_vec(&notification) {
                        Ok(bytes) => bytes,
                        Err(e) => {
                            tracing::warn!(error = %e, "Failed to serialize forgetting alert notification");
                            continue;
                        }
                    },
                ) {
                    tracing::warn!(error = %e, "Failed to insert forgetting alert notification");
                }
            }
        }
    }

    tracing::info!(at_risk, "Forgetting alert: found at-risk words");
}
