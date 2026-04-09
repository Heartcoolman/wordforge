//! B74: Confusion pair cache (weekly Sunday 5:00)

use crate::store::Store;

const USER_BATCH_SIZE: usize = 100;
const MAX_RECORDS_PER_USER: usize = 500;
const MAX_PAIRS_PER_WORD: usize = 10;
const MAX_CONFUSION_ENTRIES: usize = 10000;

pub async fn run(store: &Store) {
    tracing::info!("Confusion pair cache worker running");

    let mut confusion_map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    let mut offset = 0usize;
    loop {
        let users = match store.list_users(USER_BATCH_SIZE, offset) {
            Ok(u) => u,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to list users for confusion analysis");
                return;
            }
        };

        if users.is_empty() {
            break;
        }

        let batch_len = users.len();

        for user in &users {
            if confusion_map.len() >= MAX_CONFUSION_ENTRIES {
                break;
            }

            let records = match store.get_user_records_minimal(&user.id, MAX_RECORDS_PER_USER) {
                Ok(r) => r,
                Err(_) => continue,
            };

            let mut prev_incorrect: Option<String> = None;

            for (word_id, is_correct) in &records {
                if !is_correct {
                    if let Some(ref prev_word) = prev_incorrect {
                        if prev_word != word_id && confusion_map.len() < MAX_CONFUSION_ENTRIES {
                            confusion_map
                                .entry(prev_word.clone())
                                .or_default()
                                .push(word_id.clone());
                        }
                    }
                    prev_incorrect = Some(word_id.clone());
                } else {
                    prev_incorrect = None;
                }
            }
        }

        offset += batch_len;

        if batch_len < USER_BATCH_SIZE || confusion_map.len() >= MAX_CONFUSION_ENTRIES {
            break;
        }
    }

    let mut cached = 0u32;
    for (word_id, confused_with) in &confusion_map {
        let mut freq: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
        for other_id in confused_with {
            *freq.entry(other_id.as_str()).or_insert(0) += 1;
        }
        let mut freq_vec: Vec<_> = freq.into_iter().collect();
        freq_vec.sort_by(|a, b| b.1.cmp(&a.1));
        freq_vec.truncate(MAX_PAIRS_PER_WORD);

        for (other_id, count) in &freq_vec {
            let score = *count as f64 / confused_with.len().max(1) as f64;
            if let Err(e) = store.set_confusion_pair(word_id, other_id, score) {
                tracing::warn!(error = %e, "Failed to store confusion pair");
            }
            cached += 1;
        }
    }

    tracing::info!(cached, "Confusion pair cache updated");
}
