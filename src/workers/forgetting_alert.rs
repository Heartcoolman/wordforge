//! B44: Forgetting alert worker
//! Daily scan for words at high forgetting risk, generate notifications.

use crate::constants::MILLIS_PER_HOUR;
use crate::store::Store;

const DEDUP_WINDOW_HOURS: i64 = 48;
const MAX_DUE_WORDS_PER_USER: usize = 500;

pub async fn run(store: &Store) {
    tracing::info!("Forgetting alert worker running");
    let store = store.clone();
    match crate::blocking::run_blocking("worker.forgetting_alert", move || {
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
            let due_words = match store.get_due_words(user_id, MAX_DUE_WORDS_PER_USER) {
                Ok(w) => w,
                Err(e) => {
                    tracing::warn!(error = %e, user_id, "Forgetting alert: failed to get due words");
                    continue;
                }
            };

            for state in &due_words {
                let Some(review_date) = state.next_review_date else {
                    continue;
                };

                let review_ts_ms = review_date.timestamp_millis().max(0);
                if review_ts_ms > cutoff_ms {
                    continue;
                }

                if state.state == crate::store::operations::word_states::WordState::Mastered {
                    continue;
                }

                match store.get_alert_dedup(user_id, &state.word_id) {
                    Ok(Some(prev_ms)) if prev_ms >= cutoff_ms => {
                        skipped_dedup += 1;
                        continue;
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Forgetting alert: failed to check dedup");
                        continue;
                    }
                    _ => {}
                }

                let overdue_hours = now_ms.saturating_sub(review_ts_ms) / MILLIS_PER_HOUR;
                let notification = serde_json::json!({
                    "id": uuid::Uuid::new_v4().to_string(),
                    "userId": user_id,
                    "type": "reminder",
                    "title": "Forgetting Alert",
                    "message": format!("Word '{}' is overdue by {} hours", state.word_id, overdue_hours),
                    "wordId": state.word_id,
                    "overdueHours": overdue_hours,
                    "createdAt": now.to_rfc3339(),
                    "read": false,
                });

                if let Err(e) = store.create_notification(&notification) {
                    tracing::warn!(error = %e, "Failed to insert forgetting alert notification");
                    continue;
                }

                let _ = store.set_alert_dedup(user_id, &state.word_id, now_ms);
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
    })
    .await
    {
        Ok(()) => {}
        Err(e) => tracing::warn!(error = %e, "Forgetting alert task failed"),
    }
}
