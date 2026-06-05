//! B74: Confusion pair cache (weekly Sunday 5:00)

use crate::store::Store;

const USER_BATCH_SIZE: usize = 100;
const MAX_RECORDS_PER_USER: usize = 500;
const MAX_PAIRS_PER_WORD: usize = 10;
const MAX_CONFUSION_ENTRIES: usize = 10000;
/// #38/#43:每个 key(前词)累计的「不同后词」数上限。改用 freq 计数后,每键内存从
/// O(出现次数) 降到 O(distinct 后词),再对 distinct 数封顶,避免热词 key 无界膨胀。
const MAX_DISTINCT_PER_KEY: usize = 256;
/// #38/#43:全局总元素预算(Σ 各 key 的 distinct 后词数)。MAX_CONFUSION_ENTRIES 只限键数,
/// 词表小于 1 万时永不触发→循环跑满全库用户致内存随用户基数线性膨胀。此预算与键数无关,
/// 达到即停止累计,使封顶在任意词表规模下都有效。
const MAX_TOTAL_ELEMENTS: usize = 200_000;

pub async fn run(store: &Store) {
    tracing::info!("Confusion pair cache worker running");
    let store = store.clone();
    match crate::blocking::run_blocking("worker.confusion_pair_cache", move || {
        // value 由 Vec<String> 改为 distinct 后词 → 计数的 HashMap:累计阶段即去重,
        // 每键最多 MAX_DISTINCT_PER_KEY 个 distinct 后词,封住热词 key 的峰值内存。
        let mut confusion_map: std::collections::HashMap<
            String,
            std::collections::HashMap<String, u32>,
        > = std::collections::HashMap::new();
        // 全局已累计的 distinct 元素总数(Σ 各 key 的 distinct 后词数),用于总预算封顶。
        let mut total_elements = 0usize;

        let mut offset = 0usize;
        'outer: loop {
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
                if total_elements >= MAX_TOTAL_ELEMENTS {
                    break 'outer;
                }

                let records = match store.get_user_records_minimal(&user.id, MAX_RECORDS_PER_USER) {
                    Ok(r) => r,
                    Err(_) => continue,
                };

                let mut prev_incorrect: Option<String> = None;

                for (word_id, is_correct) in &records {
                    if !is_correct {
                        if let Some(ref prev_word) = prev_incorrect {
                            if prev_word != word_id && total_elements < MAX_TOTAL_ELEMENTS {
                                // 仅当新建 key 时受键数上限约束;既有 key 继续累计计数。
                                let is_new_key = !confusion_map.contains_key(prev_word);
                                if !is_new_key || confusion_map.len() < MAX_CONFUSION_ENTRIES {
                                    let counts = confusion_map.entry(prev_word.clone()).or_default();
                                    // 先取 distinct 数,避免 get_mut 的可变借用与 len() 的不可变借用冲突。
                                    let distinct = counts.len();
                                    match counts.get_mut(word_id) {
                                        Some(c) => *c += 1,
                                        None if distinct < MAX_DISTINCT_PER_KEY => {
                                            counts.insert(word_id.clone(), 1);
                                            total_elements += 1;
                                        }
                                        // 该 key 的 distinct 后词已达上限:丢弃新后词,不再增长。
                                        None => {}
                                    }
                                }
                            }
                        }
                        prev_incorrect = Some(word_id.clone());
                    } else {
                        prev_incorrect = None;
                    }
                }
            }

            offset += batch_len;

            if batch_len < USER_BATCH_SIZE || total_elements >= MAX_TOTAL_ELEMENTS {
                break;
            }
        }

        let mut cached = 0u32;
        for (word_id, counts) in &confusion_map {
            let total: u32 = counts.values().copied().sum();
            let mut freq_vec: Vec<(&str, u32)> =
                counts.iter().map(|(k, v)| (k.as_str(), *v)).collect();
            freq_vec.sort_by(|a, b| b.1.cmp(&a.1));
            freq_vec.truncate(MAX_PAIRS_PER_WORD);

            for (other_id, count) in &freq_vec {
                let score = *count as f64 / total.max(1) as f64;
                if let Err(e) = store.set_confusion_pair(word_id, other_id, score) {
                    tracing::warn!(error = %e, "Failed to store confusion pair");
                }
                cached += 1;
            }
        }

        tracing::info!(cached, "Confusion pair cache updated");
    })
    .await
    {
        Ok(()) => {}
        Err(e) => tracing::warn!(error = %e, "Confusion pair cache task failed"),
    }
}
