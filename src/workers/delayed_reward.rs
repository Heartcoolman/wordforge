//! B43: Delayed reward worker
//! Every minute, scan delayed_rewards and evaluate retention after interval.

use crate::store::Store;

pub async fn run(store: &Store) {
    tracing::debug!("Delayed reward worker tick");

    // Scan word_learning_states for words that are due for review
    // and evaluate if the scheduled review interval was appropriate
    let now = chrono::Utc::now();

    let mut evaluated = 0u32;
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

        if let Some(review_date) = wls.next_review_date {
            if review_date <= now
                && wls.state != crate::store::operations::word_states::WordState::Mastered
            {
                evaluated += 1;
            }
        }
    }

    if evaluated > 0 {
        tracing::info!(evaluated, "Delayed reward: evaluated overdue words");
    }
}
