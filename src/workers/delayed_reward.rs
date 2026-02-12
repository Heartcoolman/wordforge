//! B43: Delayed reward worker
//! Every 5 minutes, scan per-user word_learning_states for overdue reviews.

use crate::store::Store;

pub async fn run(store: &Store) {
    tracing::debug!("Delayed reward worker tick");

    let now_ms = chrono::Utc::now().timestamp_millis().max(0);
    let evaluated = count_overdue_words(store, now_ms);

    if evaluated > 0 {
        tracing::info!(evaluated, "Delayed reward: evaluated overdue words");
    }
}

pub fn count_overdue_words(store: &Store, now_ms: i64) -> u32 {
    let mut evaluated = 0u32;

    let user_ids = match store.list_user_ids() {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!(error = %e, "Delayed reward: failed to list users");
            return 0;
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

            if due_ts_ms > now_ms {
                break;
            }

            let state = match store.get_word_learning_state(user_id, &word_id) {
                Ok(Some(s)) => s,
                Ok(None) => continue,
                Err(e) => {
                    tracing::warn!(error = %e, "Delayed reward: failed to read word state");
                    continue;
                }
            };

            if let Some(review_date) = state.next_review_date {
                let review_ts_ms = review_date.timestamp_millis().max(0);
                if review_ts_ms == due_ts_ms
                    && review_ts_ms <= now_ms
                    && state.state != crate::store::operations::word_states::WordState::Mastered
                {
                    evaluated += 1;
                }
            }
        }
    }

    evaluated
}

#[cfg(test)]
mod tests {
    use super::count_overdue_words;
    use crate::store::operations::users::User;
    use crate::store::operations::word_states::{WordLearningState, WordState};
    use crate::store::Store;
    use chrono::{Duration, Utc};
    use tempfile::tempdir;

    fn sample_user(id: &str, email: &str) -> User {
        User {
            id: id.to_string(),
            email: email.to_string(),
            username: format!("user-{id}"),
            password_hash: "hash".to_string(),
            is_banned: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            failed_login_count: 0,
            locked_until: None,
        }
    }

    fn sample_state(
        user_id: &str,
        word_id: &str,
        state: WordState,
        next_review_date: Option<chrono::DateTime<Utc>>,
    ) -> WordLearningState {
        WordLearningState {
            user_id: user_id.to_string(),
            word_id: word_id.to_string(),
            state,
            mastery_level: 0.5,
            next_review_date,
            half_life: 2.0,
            correct_streak: 1,
            total_attempts: 3,
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn count_overdue_words_ignores_future_and_mastered() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path().join("db-delayed-reward").to_str().unwrap()).unwrap();

        let user = sample_user("u1", "u1@example.com");
        store.create_user(&user).unwrap();

        let now = Utc::now();

        store
            .set_word_learning_state(&sample_state(
                &user.id,
                "w_due",
                WordState::Learning,
                Some(now - Duration::hours(2)),
            ))
            .unwrap();

        store
            .set_word_learning_state(&sample_state(
                &user.id,
                "w_future",
                WordState::Learning,
                Some(now + Duration::hours(1)),
            ))
            .unwrap();

        store
            .set_word_learning_state(&sample_state(
                &user.id,
                "w_mastered",
                WordState::Mastered,
                Some(now - Duration::hours(3)),
            ))
            .unwrap();

        let count = count_overdue_words(&store, now.timestamp_millis().max(0));
        assert_eq!(count, 1);
    }
}
