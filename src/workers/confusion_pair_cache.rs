//! B74: Confusion pair cache (weekly Sunday 5:00)
//! 分页加载用户，每批 100 个

use crate::store::Store;

const USER_BATCH_SIZE: usize = 100;
/// 每个用户读取的最大记录数
const MAX_RECORDS_PER_USER: usize = 500;
/// 每个单词最多保留的混淆对数量
const MAX_PAIRS_PER_WORD: usize = 10;

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
            let records = store.get_user_records(&user.id, MAX_RECORDS_PER_USER).unwrap_or_default();

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

        offset += batch_len;

        if batch_len < USER_BATCH_SIZE {
            break;
        }
    }

    let mut cached = 0u32;
    for (word_id, confused_with) in &confusion_map {
        // 统计每个混淆目标出现的频次
        let mut freq: std::collections::HashMap<&str, u32> = std::collections::HashMap::new();
        for other_id in confused_with {
            *freq.entry(other_id.as_str()).or_insert(0) += 1;
        }
        // 按频次降序排序，保留 top-N
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
