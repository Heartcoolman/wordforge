//! 选词评分模块：综合 AMAS 算法输出，对候选词进行评分排序

use std::cmp::Ordering;
use std::collections::HashMap;

use serde::Serialize;

use crate::amas::config::{EloConfig, MemoryModelConfig, WordSelectorConfig};
use crate::amas::elo::zpd_priority;
use crate::amas::memory::mdm::MdmState;
use crate::amas::types::StrategyParams;
use crate::response::AppError;
use crate::store::operations::words::Word;
use crate::store::Store;

fn score_desc(a: &ScoredWord, b: &ScoredWord) -> Ordering {
    b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal)
}

fn retain_top_k_by_score(words: &mut Vec<ScoredWord>, k: usize) {
    if k == 0 {
        words.clear();
        return;
    }

    if words.len() <= k {
        words.sort_by(score_desc);
        return;
    }

    words.select_nth_unstable_by(k - 1, score_desc);
    words.truncate(k);
    words.sort_by(score_desc);
}

fn review_ucb_bonus(review_population: usize, total_attempts: u32, ws: &WordSelectorConfig) -> f64 {
    if review_population <= 1 {
        return 0.0;
    }

    let numerator = (review_population as f64 + 1.0).ln();
    let denominator = total_attempts as f64 + 1.0;
    let bonus = ws.review_ucb_weight * (numerator / denominator).sqrt();

    bonus.min(ws.review_ucb_max_bonus)
}

fn score_new_word_prefetched(
    word: &Word,
    word_elo_rating: f64,
    user_elo_rating: f64,
    strategy: &StrategyParams,
    ws: &WordSelectorConfig,
    elo_config: &EloConfig,
) -> f64 {
    let diff_gap = (word.difficulty - strategy.difficulty).abs();
    let sigma = ws.new_word_gaussian_sigma;
    let difficulty_penalty = (-diff_gap.powi(2) / (2.0 * sigma.powi(2))).exp();
    zpd_priority(user_elo_rating, word_elo_rating, elo_config) * difficulty_penalty
}

fn score_review_word_prefetched(
    mdm_state: &MdmState,
    now_ms: i64,
    mm: &MemoryModelConfig,
    ws: &WordSelectorConfig,
) -> (f64, f64) {
    let recall = crate::amas::memory::mdm::recall_probability(mdm_state, now_ms, mm);

    let mut score = 1.0 - recall;
    let sigmoid = |x: f64| 1.0 / (1.0 + (-x).exp());
    score += mm.recall_risk_bonus * sigmoid((mm.recall_risk_threshold - recall) * ws.sigmoid_steepness);

    (score, recall)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoredWord {
    pub word_id: String,
    pub score: f64,
    pub is_new: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SessionSelectionContext {
    pub error_prone_word_ids: Vec<String>,
    pub recently_mastered_word_ids: Vec<String>,
    pub temporal_boost: f64,
}

pub struct SelectionConfigs<'a> {
    pub word_selector: &'a WordSelectorConfig,
    pub elo: &'a EloConfig,
    pub memory_model: &'a MemoryModelConfig,
}

/// 从候选词中选出最优学习批次
pub fn select_words(
    store: &Store,
    user_id: &str,
    candidate_word_ids: &[String],
    strategy: &StrategyParams,
    batch_size: usize,
    context: Option<&SessionSelectionContext>,
    configs: &SelectionConfigs<'_>,
) -> Result<Vec<ScoredWord>, AppError> {
    let ws = configs.word_selector;
    let elo_config = configs.elo;
    let mm = configs.memory_model;
    let now_ms = chrono::Utc::now().timestamp_millis();

    let words_by_id = store
        .get_words_by_ids(candidate_word_ids)
        .map_err(|e| AppError::internal(&e.to_string()))?;
    let word_elo_by_id = store
        .get_word_elos_by_ids(candidate_word_ids)
        .map_err(|e| AppError::internal(&e.to_string()))?;
    let mastery_state_by_id: HashMap<String, MdmState> = store
        .batch_get_engine_mastery_mdm_states(user_id, candidate_word_ids)
        .map_err(|e| AppError::internal(&e.to_string()))?;

    // 预加载词学习状态，后续用 UCB 探索项执行探索-利用平衡。
    let state_by_word_id: HashMap<String, u32> = store
        .get_word_states_batch(user_id, candidate_word_ids)
        .map_err(|e| AppError::internal(&e.to_string()))?
        .into_iter()
        .map(|state| (state.word_id, state.total_attempts))
        .collect();
    let review_population = state_by_word_id.len();
    let mut new_words: Vec<ScoredWord> =
        Vec::with_capacity(candidate_word_ids.len().saturating_sub(review_population));
    let mut review_words: Vec<ScoredWord> = Vec::with_capacity(review_population);
    let default_mdm_state = MdmState::default();

    // 获取用户 ELO（用于新词 ZPD 评分）
    let user_elo = store
        .get_user_elo(user_id)
        .map_err(|e| AppError::internal(&e.to_string()))?;

    // 构建上下文集合用于快速查找
    let error_prone_set: std::collections::HashSet<&str> = context
        .map(|c| c.error_prone_word_ids.iter().map(|s| s.as_str()).collect())
        .unwrap_or_default();
    let recently_mastered_set: std::collections::HashSet<&str> = context
        .map(|c| {
            c.recently_mastered_word_ids
                .iter()
                .map(|s| s.as_str())
                .collect()
        })
        .unwrap_or_default();

    for word_id in candidate_word_ids {
        let attempts = state_by_word_id.get(word_id).copied();

        if attempts.is_none() {
            let Some(word) = words_by_id.get(word_id) else {
                continue;
            };
            let word_elo_rating = word_elo_by_id
                .get(word_id)
                .map(|elo| elo.rating)
                .unwrap_or_default();

            let score = score_new_word_prefetched(
                word,
                word_elo_rating,
                user_elo.rating,
                strategy,
                ws,
                elo_config,
            );
            new_words.push(ScoredWord {
                word_id: word_id.clone(),
                score,
                is_new: true,
            });
        } else {
            // 复习词：回忆风险（利用） + UCB 探索项（探索）
            let mdm_state = mastery_state_by_id
                .get(word_id)
                .unwrap_or(&default_mdm_state);
            let (base_score, recall) = score_review_word_prefetched(mdm_state, now_ms, mm, ws);
            let mut score =
                base_score + review_ucb_bonus(review_population, attempts.unwrap_or_default(), ws);

            // 上下文加权：error_prone 词额外加分
            if error_prone_set.contains(word_id.as_str()) {
                score += ws.error_prone_bonus;
            }

            // 上下文加权：recently_mastered 且回忆概率低的词加分
            if recently_mastered_set.contains(word_id.as_str())
                && recall < ws.recall_mastered_threshold
            {
                score += ws.recently_mastered_bonus;
            }

            review_words.push(ScoredWord {
                word_id: word_id.clone(),
                score,
                is_new: false,
            });
        }
    }

    // 按 new_ratio 混合新词和复习词，应用 temporal_boost 缩放
    let effective_new_ratio = if let Some(ctx) = context {
        (strategy.new_ratio * ctx.temporal_boost).clamp(0.0, 1.0)
    } else {
        strategy.new_ratio
    };
    let new_count = (batch_size as f64 * effective_new_ratio).round() as usize;
    let review_count = batch_size.saturating_sub(new_count);

    // 使用 Top-K 选择而非全量排序：从 O(n log n) 收敛为 O(n + k log k)
    retain_top_k_by_score(&mut new_words, new_count);
    retain_top_k_by_score(&mut review_words, review_count);

    // 交叉混合新词和复习词，按 new_ratio 比例交替排列
    let actual_new = new_words.len();
    let actual_review = review_words.len();
    let total = actual_new + actual_review;
    let mut result: Vec<ScoredWord> = Vec::with_capacity(batch_size);

    if total == 0 {
        return Ok(result);
    }

    let mut selected_new = new_words.into_iter();
    let mut selected_review = review_words.into_iter();

    let mut ni = 0usize;
    let mut ri = 0usize;
    for i in 0..total {
        // 按比例决定当前位置放新词还是复习词
        let new_target = ((i + 1) * actual_new) / total;
        if ni < actual_new && ni < new_target {
            if let Some(w) = selected_new.next() {
                result.push(w);
            }
            ni += 1;
        } else if ri < actual_review {
            if let Some(w) = selected_review.next() {
                result.push(w);
            }
            ri += 1;
        } else if ni < actual_new {
            if let Some(w) = selected_new.next() {
                result.push(w);
            }
            ni += 1;
        }
    }

    result.truncate(batch_size);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn scored_word_serializes_camel_case() {
        let sw = ScoredWord {
            word_id: "w1".to_string(),
            score: 0.8,
            is_new: true,
        };
        let json = serde_json::to_value(&sw).unwrap();
        assert!(json.get("wordId").is_some());
        assert!(json.get("isNew").is_some());
    }

    #[test]
    fn review_ucb_bonus_decreases_with_attempts() {
        let ws = WordSelectorConfig::default();
        let small_attempt_bonus = review_ucb_bonus(50, 1, &ws);
        let high_attempt_bonus = review_ucb_bonus(50, 20, &ws);
        assert!(small_attempt_bonus > high_attempt_bonus);
        assert!(small_attempt_bonus <= ws.review_ucb_max_bonus);
    }

    #[test]
    fn retain_top_k_keeps_highest_scores() {
        let mut words = vec![
            ScoredWord {
                word_id: "w1".to_string(),
                score: 0.2,
                is_new: true,
            },
            ScoredWord {
                word_id: "w2".to_string(),
                score: 0.9,
                is_new: true,
            },
            ScoredWord {
                word_id: "w3".to_string(),
                score: 0.7,
                is_new: true,
            },
        ];

        retain_top_k_by_score(&mut words, 2);

        assert_eq!(words.len(), 2);
        assert_eq!(words[0].word_id, "w2");
        assert_eq!(words[1].word_id, "w3");
    }

    #[test]
    fn score_new_word_prefetched_penalizes_far_difficulty() {
        let ws = WordSelectorConfig::default();
        let elo_config = EloConfig::default();

        let near_word = Word {
            id: "near".to_string(),
            text: "near".to_string(),
            meaning: "near".to_string(),
            pronunciation: None,
            part_of_speech: None,
            difficulty: 0.5,
            examples: vec![],
            tags: vec![],
            embedding: None,
            created_at: Utc::now(),
        };

        let far_word = Word {
            id: "far".to_string(),
            text: "far".to_string(),
            meaning: "far".to_string(),
            pronunciation: None,
            part_of_speech: None,
            difficulty: 0.95,
            examples: vec![],
            tags: vec![],
            embedding: None,
            created_at: Utc::now(),
        };

        let strategy = StrategyParams {
            difficulty: 0.5,
            new_ratio: 0.5,
            batch_size: 20,
            interval_scale: 1.0,
            review_mode: false,
        };

        let near_score =
            score_new_word_prefetched(&near_word, 1200.0, 1200.0, &strategy, &ws, &elo_config);
        let far_score =
            score_new_word_prefetched(&far_word, 1200.0, 1200.0, &strategy, &ws, &elo_config);

        assert!(near_score > far_score);
    }
}
