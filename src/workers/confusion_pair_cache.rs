//! B74: Confusion pair cache (weekly Sunday 5:00)
//! 分页加载用户，每批 100 个

use crate::store::Store;

const USER_BATCH_SIZE: usize = 100;
const MAX_RECORDS_PER_USER: usize = 500;
const MAX_PAIRS_PER_WORD: usize = 10;
const MAX_CONFUSION_ENTRIES: usize = 10000;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecordMinimal {
    word_id: String,
    is_correct: bool,
}

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

            let prefix = match crate::store::keys::record_prefix(&user.id) {
                Ok(p) => p,
                Err(_) => continue,
            };

            let mut records_read = 0usize;
            let mut prev_incorrect: Option<String> = None;

            for item in store.records.scan_prefix(prefix.as_bytes()) {
                if records_read >= MAX_RECORDS_PER_USER {
                    break;
                }
                let (_, v) = match item {
                    Ok(kv) => kv,
                    Err(_) => continue,
                };
                let record: RecordMinimal = match serde_json::from_slice(&v) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                records_read += 1;

                if !record.is_correct {
                    if let Some(ref prev_word) = prev_incorrect {
                        if prev_word != &record.word_id && confusion_map.len() < MAX_CONFUSION_ENTRIES {
                            confusion_map
                                .entry(prev_word.clone())
                                .or_default()
                                .push(record.word_id.clone());
                        }
                    }
                    prev_incorrect = Some(record.word_id);
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

        for (other_id, score) in &freq_vec {
            let key = match crate::store::keys::confusion_pair_key(word_id, other_id) {
                Ok(k) => k,
                Err(_) => continue,
            };
            let pair = serde_json::json!({
                "wordA": word_id,
                "wordB": other_id,
                "score": *score as f64 / confused_with.len().max(1) as f64,
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
