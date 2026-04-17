//! B43: Delayed reward worker
//! Every 5 minutes, scan per-user word_learning_states for overdue reviews.

use crate::store::Store;

const MAX_DUE_WORDS_PER_USER: usize = 1000;

pub async fn run(store: &Store) {
    tracing::debug!("Delayed reward worker tick");

    let now_ms = chrono::Utc::now().timestamp_millis().max(0);
    let evaluated = count_overdue_words(store, now_ms);

    if evaluated > 0 {
        tracing::info!(evaluated, "Delayed reward: evaluated overdue words");
    }
}

pub fn count_overdue_words(store: &Store, _now_ms: i64) -> u32 {
    let mut evaluated = 0u32;

    let user_ids = match store.list_user_ids() {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!(error = %e, "Delayed reward: failed to list users");
            return 0;
        }
    };

    for user_id in &user_ids {
        let due_words = match store.get_due_words(user_id, MAX_DUE_WORDS_PER_USER) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!(error = %e, user_id, "Delayed reward: failed to get due words");
                continue;
            }
        };

        for state in &due_words {
            if state.state != crate::store::operations::word_states::WordState::Mastered {
                evaluated += 1;
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
        let store = Store::open(
            dir.path().join("db-delayed-reward").to_str().unwrap(),
            5000,
            1,
        )
        .unwrap();

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
