//! B74: Confusion pair cache (weekly Sunday 5:00)

use crate::store::Store;

pub async fn run(store: &Store) {
    tracing::info!("Confusion pair cache worker running");

    // Scan records to find commonly confused word pairs
    let mut confusion_map: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    // Look at sequential incorrect records for the same user
    let users = match store.list_users(usize::MAX, 0) {
        Ok(u) => u,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to list users for confusion analysis");
            return;
        }
    };

    for user in &users {
        let records = store.get_user_records(&user.id, 500).unwrap_or_default();

        // Look for consecutive incorrect answers as confusion indicators
        let mut prev_incorrect: Option<String> = None;
        for record in &records {
            if !record.is_correct {
                if let Some(ref prev_word) = prev_incorrect {
                    if prev_word != &record.word_id {
                        confusion_map
                            .entry(prev_word.clone())
                            .or_default()
                            .push(record.word_id.clone());
                    }
                }
                prev_incorrect = Some(record.word_id.clone());
            } else {
                prev_incorrect = None;
            }
        }
    }

    // Store confusion pairs
    let mut cached = 0u32;
    for (word_id, confused_with) in &confusion_map {
        for other_id in confused_with {
            let key = crate::store::keys::confusion_pair_key(word_id, other_id);
            let pair = serde_json::json!({
                "wordA": word_id,
                "wordB": other_id,
                "score": 0.5,
                "updatedAt": chrono::Utc::now().to_rfc3339(),
            });
            if let Err(e) = store.confusion_pairs.insert(
                key.as_bytes(),
                match serde_json::to_vec(&pair) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to serialize confusion pair");
                        continue;
                    }
                },
            ) {
                tracing::warn!(error = %e, "Failed to store confusion pair");
            }
            cached += 1;
        }
    }

    tracing::info!(cached, "Confusion pair cache updated");
}
