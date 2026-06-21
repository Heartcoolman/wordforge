//! 选词评分模块：综合 AMAS 算法输出，对候选词进行评分排序

use std::cmp::Ordering;
use std::collections::HashMap;

use rand::Rng;
use serde::Serialize;

/// 选词评分微噪声幅度：仅用于打破完全同分时的固定排序，
/// 远小于任意单项评分项的最小绝对值（mastery_dampen ~1e-2 起），
/// 不影响 ZPD/UCB/recall/cooldown 数学不变量。
const SCORE_TIEBREAK_JITTER: f64 = 1e-5;

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

fn cooldown_factor(last_review_at: Option<i64>, now_ms: i64, cooldown_secs: f64) -> f64 {
    match last_review_at {
        None => 1.0,
        Some(last) => {
            // 钳非负：时钟回拨(NTP step / VM 校时)致 now_ms < last 时，elapsed 为负会让 exp>1、
            // 冷却因子变负，翻转高优先复习词排序。未来/相等时间戳视作 0 elapsed → 因子 0。
            let elapsed_secs = (now_ms - last).max(0) as f64 / 1000.0;
            1.0 - (-elapsed_secs / cooldown_secs).exp()
        }
    }
}

fn gaussian_recall_bonus(recall: f64, center: f64, sigma: f64) -> f64 {
    let d = (recall - center) / sigma;
    (-0.5 * d * d).exp()
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ReviewWordScore {
    score: f64,
    recall: f64,
    suppress_extras: bool,
}

fn is_review_candidate(mdm_state: Option<&MdmState>) -> bool {
    mdm_state.is_some_and(|state| state.review_count > 0 || state.last_review_at.is_some())
}

/// 词内在难度（DB [0,1]）→ FSRS [1,10] 标度，注入 difflogit 预测层 logit 项。
/// DB 默认 0.0 = 未填充难度 → None（no-op，bit-exact 纯 FSRS），避免缺值被映射成 D=1（最易档）
/// 对所有复习词恒定上推召回 β·(REF−1)（见 amas_config.toml difficultyLogit 注释③ 缺值回落中性）。
fn word_difficulty_logit_scale(difficulty: f64) -> Option<f64> {
    let d = difficulty.clamp(0.0, 1.0);
    (d > 0.0).then(|| 1.0 + 9.0 * d)
}

fn score_review_word_prefetched(
    mdm_state: &MdmState,
    now_ms: i64,
    mm: &MemoryModelConfig,
    ws: &WordSelectorConfig,
    word_difficulty: Option<f64>,
) -> ReviewWordScore {
    // v6 预测层：urgency 用 difficulty-aware recall 读出（β=0 或无难度时退化为纯 recall）。
    let recall =
        crate::amas::memory::mdm::recall_probability_predicted(mdm_state, now_ms, mm, word_difficulty);
    if recall >= ws.recall_mastered_threshold {
        return ReviewWordScore {
            score: 0.001,
            recall,
            suppress_extras: true,
        };
    }

    let mut score = 1.0 - recall;
    let sigmoid = |x: f64| 1.0 / (1.0 + (-x).exp());
    score +=
        mm.recall_risk_bonus * sigmoid((mm.recall_risk_threshold - recall) * ws.sigmoid_steepness);

    let g_bonus = gaussian_recall_bonus(recall, ws.optimal_recall_center, ws.optimal_recall_sigma);
    let g_bonus = g_bonus.max(0.15);
    let cd = cooldown_factor(mdm_state.last_review_at, now_ms, ws.spacing_cooldown_secs);
    let mastery_dampen =
        1.0 - sigmoid((recall - ws.recall_mastered_threshold) * ws.sigmoid_steepness);

    score *= g_bonus * cd * mastery_dampen;

    ReviewWordScore {
        score,
        recall,
        suppress_extras: false,
    }
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
    /// ② 混淆隔离（Phase 1b）：本 session 已出现/已选词的高分混淆对端。命中者评分被
    /// `confusion_isolation_dampen` 惩罚（非硬排除，防候选稀少掏空批次）。空 → no-op。
    pub confusion_exclude_word_ids: Vec<String>,
    pub temporal_boost: f64,
}

pub struct SelectionConfigs<'a> {
    pub word_selector: &'a WordSelectorConfig,
    pub elo: &'a EloConfig,
    pub memory_model: &'a MemoryModelConfig,
    /// ③ 多痕迹（Phase 2）：开启时按 min-recall 跨 per-mode 痕迹聚合出代表态；默认 false → 单痕迹 legacy。
    pub multi_trace_enabled: bool,
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
    // T1.1 Parallel Elo：开启时 ZPD 选词读「选词链」(rating_select 延迟快照)，与估计链(rating)解耦，
    // 消除「选择依赖被估计量」的方差膨胀/偏差。默认 off → 走估计链，bit-exact 不变。
    let select_rating_by_id = if elo_config.parallel_elo_enabled {
        Some(
            store
                .get_word_select_ratings_by_ids(candidate_word_ids)
                .map_err(|e| AppError::internal(&e.to_string()))?,
        )
    } else {
        None
    };
    // ③ 多痕迹（Phase 2）：开启时取每词全部痕迹、按 min-recall（最弱题型）选代表态注入下游打分；
    // 关闭时走单一 `mastery:{word}`（bit-exact legacy）。下游评分路径不变。
    let mastery_state_by_id: HashMap<String, MdmState> = if configs.multi_trace_enabled {
        let traces = store
            .batch_get_engine_mastery_mdm_traces(user_id, candidate_word_ids)
            .map_err(|e| AppError::internal(&e.to_string()))?;
        traces
            .into_iter()
            .filter_map(|(word_id, states)| {
                states
                    .into_iter()
                    .min_by(|a, b| {
                        let ra = crate::amas::memory::mdm::recall_probability(a, now_ms, mm);
                        let rb = crate::amas::memory::mdm::recall_probability(b, now_ms, mm);
                        ra.partial_cmp(&rb).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|state| (word_id, state))
            })
            .collect()
    } else {
        store
            .batch_get_engine_mastery_mdm_states(user_id, candidate_word_ids)
            .map_err(|e| AppError::internal(&e.to_string()))?
    };

    let review_population = mastery_state_by_id
        .values()
        .filter(|state| is_review_candidate(Some(state)))
        .count();
    let mut new_words: Vec<ScoredWord> =
        Vec::with_capacity(candidate_word_ids.len().saturating_sub(review_population));
    let mut review_words: Vec<ScoredWord> = Vec::with_capacity(review_population);
    let mut rng = rand::thread_rng();

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
    // ② 混淆隔离（Phase 1b）：命中集合的词评分乘 dampen（默认 1.0=no-op），降低与已出现易混词
    // 共排概率。空集合 / dampen=1.0 双保险 → bit-exact 不变。
    let confusion_exclude_set: std::collections::HashSet<&str> = context
        .map(|c| {
            c.confusion_exclude_word_ids
                .iter()
                .map(|s| s.as_str())
                .collect()
        })
        .unwrap_or_default();
    let confusion_dampen = ws.confusion_isolation_dampen;

    for word_id in candidate_word_ids {
        let mdm_state = mastery_state_by_id.get(word_id);

        if !is_review_candidate(mdm_state) {
            let Some(word) = words_by_id.get(word_id) else {
                continue;
            };
            let word_elo_rating = match &select_rating_by_id {
                Some(sel) => sel.get(word_id).copied().unwrap_or_default(),
                None => word_elo_by_id
                    .get(word_id)
                    .map(|elo| elo.rating)
                    .unwrap_or_default(),
            };

            let mut score = score_new_word_prefetched(
                word,
                word_elo_rating,
                user_elo.rating,
                strategy,
                ws,
                elo_config,
            );
            if confusion_exclude_set.contains(word_id.as_str()) {
                score *= confusion_dampen;
            }
            score += rng.gen_range(0.0..SCORE_TIEBREAK_JITTER);
            new_words.push(ScoredWord {
                word_id: word_id.clone(),
                score,
                is_new: true,
            });
        } else {
            // 复习词：回忆风险（利用） + UCB 探索项（探索）
            let mdm_state = mdm_state.expect("review candidates must have mastery state");
            // v6：词内在难度（DB [0,1]）映射到 FSRS [1,10] 注入预测层 logit 项；
            // 缺词 或 默认 0.0（未填充）→ None=no-op，避免 D=1 强上推召回。
            let word_difficulty = words_by_id
                .get(word_id)
                .and_then(|w| word_difficulty_logit_scale(w.difficulty));
            let review_score = score_review_word_prefetched(mdm_state, now_ms, mm, ws, word_difficulty);
            let mut score = review_score.score;

            if !review_score.suppress_extras {
                score += review_ucb_bonus(review_population, mdm_state.review_count, ws);

                // 上下文加权：error_prone 词额外加分
                if error_prone_set.contains(word_id.as_str()) {
                    score += ws.error_prone_bonus;
                }

                // 上下文加权：recently_mastered 且回忆概率低的词加分
                if recently_mastered_set.contains(word_id.as_str())
                    && review_score.recall < ws.recall_mastered_threshold
                {
                    score += ws.recently_mastered_bonus;
                }
            }
            // ② 混淆隔离惩罚（默认 dampen=1.0 → no-op）。对 suppress_extras 的已掌握词同样适用。
            if confusion_exclude_set.contains(word_id.as_str()) {
                score *= confusion_dampen;
            }
            score += rng.gen_range(0.0..SCORE_TIEBREAK_JITTER);

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
    let available_new = new_words.len();
    let available_review = review_words.len();
    let keep_new = available_new.min(new_count + review_count.saturating_sub(available_review));
    let keep_review = available_review.min(review_count + new_count.saturating_sub(available_new));

    // 使用 Top-K 选择而非全量排序：从 O(n log n) 收敛为 O(n + k log k)
    retain_top_k_by_score(&mut new_words, keep_new);
    retain_top_k_by_score(&mut review_words, keep_review);

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
    use crate::amas::memory::mastery::WordMasteryState;
    use crate::amas::types::MasteryLevel;
    use crate::store::operations::words::Word;
    use crate::store::Store;
    use chrono::Utc;

    #[test]
    fn word_difficulty_logit_scale_treats_default_zero_as_missing() {
        // DB 默认 0.0（未填充）→ None：避免 difflogit β·(REF−1) 对所有复习词恒定上推召回
        assert_eq!(word_difficulty_logit_scale(0.0), None);
        // 真实难度 → 映射到 FSRS [1,10]
        assert_eq!(word_difficulty_logit_scale(1.0), Some(10.0));
        assert_eq!(word_difficulty_logit_scale(0.5), Some(5.5));
        // 越界钳制：负值视作未填充，>1 钳到 10
        assert_eq!(word_difficulty_logit_scale(-0.3), None);
        assert_eq!(word_difficulty_logit_scale(2.0), Some(10.0));
    }

    fn review_state_for_recall(
        target_recall: f64,
        now_ms: i64,
        mm: &MemoryModelConfig,
    ) -> MdmState {
        let stability = 10.0;
        let floor = mm.forgetting_curve_floor;
        let adjusted_target = ((target_recall - floor) / (1.0 - floor).max(1e-9)).clamp(1e-6, 1.0);
        let elapsed_days =
            stability / mm.curve_factor() * (adjusted_target.powf(-1.0 / mm.curve_decay()) - 1.0);

        MdmState {
            stability,
            difficulty: 5.0,
            memory_strength: stability,
            last_review_at: Some(now_ms - (elapsed_days * 86_400_000.0).round() as i64),
            review_count: 1,
            ..MdmState::default()
        }
    }

    fn test_store() -> Store {
        let store = Store::open(":memory:", 5000, 1).unwrap();
        store.run_migrations().unwrap();
        store
    }

    fn sample_word(id: &str, difficulty: f64) -> Word {
        Word {
            id: id.to_string(),
            text: id.to_string(),
            meaning: format!("meaning-{id}"),
            pronunciation: None,
            part_of_speech: None,
            difficulty,
            examples: vec![],
            tags: vec![],
            embedding: None,
            created_at: Utc::now(),
        }
    }

    fn persist_review_state(store: &Store, user_id: &str, word_id: &str, mdm: MdmState) {
        let mut mastery = WordMasteryState::new(word_id);
        mastery.mdm = mdm;
        mastery.mastery_level = MasteryLevel::Reviewing;
        mastery.total_attempts = mastery.mdm.review_count;
        mastery.total_correct = mastery.mdm.review_count;
        store
            .set_engine_algo_state(
                user_id,
                &format!("mastery:{word_id}"),
                &serde_json::to_value(&mastery).unwrap(),
            )
            .unwrap();
    }

    fn default_selection_configs<'a>(
        mm: &'a MemoryModelConfig,
        ws: &'a WordSelectorConfig,
        elo: &'a EloConfig,
    ) -> SelectionConfigs<'a> {
        SelectionConfigs {
            word_selector: ws,
            elo,
            memory_model: mm,
            multi_trace_enabled: false,
        }
    }

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

    #[test]
    fn score_review_is_continuous_around_mastered_threshold() {
        let mm = MemoryModelConfig::default();
        let ws = WordSelectorConfig::default();
        let now_ms = Utc::now().timestamp_millis();
        let recall_799 = review_state_for_recall(0.799, now_ms, &mm);
        let recall_801 = review_state_for_recall(0.801, now_ms, &mm);

        let score_799 = score_review_word_prefetched(&recall_799, now_ms, &mm, &ws, None);
        let score_801 = score_review_word_prefetched(&recall_801, now_ms, &mm, &ws, None);

        assert!((score_799.recall - 0.799).abs() < 0.002);
        assert!((score_801.recall - 0.801).abs() < 0.002);
        assert!((score_799.score - score_801.score).abs() < 0.2);
    }

    #[test]
    fn mastered_words_still_ranked_lowest() {
        let mm = MemoryModelConfig::default();
        let ws = WordSelectorConfig::default();
        let now_ms = Utc::now().timestamp_millis();
        let mastered = review_state_for_recall(0.95, now_ms, &mm);
        let review_candidate = review_state_for_recall(0.4, now_ms, &mm);

        let mastered_score = score_review_word_prefetched(&mastered, now_ms, &mm, &ws, None);
        let review_score = score_review_word_prefetched(&review_candidate, now_ms, &mm, &ws, None);

        assert!(mastered_score.score < review_score.score);
        assert!(mastered_score.suppress_extras);
    }

    #[test]
    fn select_words_backfills_from_review_pool_when_new_words_are_short() {
        let store = test_store();
        let user_id = "u1";
        let mm = MemoryModelConfig::default();
        let ws = WordSelectorConfig::default();
        let elo = EloConfig::default();
        let configs = default_selection_configs(&mm, &ws, &elo);
        let strategy = StrategyParams {
            difficulty: 0.5,
            batch_size: 10,
            new_ratio: 0.5,
            interval_scale: 1.0,
            review_mode: false,
        };
        let now_ms = Utc::now().timestamp_millis();

        let mut candidate_ids = Vec::new();
        for idx in 0..2 {
            let word_id = format!("new-{idx}");
            store.upsert_word(&sample_word(&word_id, 0.4)).unwrap();
            candidate_ids.push(word_id);
        }
        for idx in 0..12 {
            let word_id = format!("review-{idx}");
            store.upsert_word(&sample_word(&word_id, 0.6)).unwrap();
            persist_review_state(
                &store,
                user_id,
                &word_id,
                review_state_for_recall(0.45 + idx as f64 * 0.01, now_ms, &mm),
            );
            candidate_ids.push(word_id);
        }

        let selected = select_words(
            &store,
            user_id,
            &candidate_ids,
            &strategy,
            10,
            None,
            &configs,
        )
        .unwrap();

        assert_eq!(selected.len(), 10);
        assert_eq!(selected.iter().filter(|word| word.is_new).count(), 2);
        assert_eq!(selected.iter().filter(|word| !word.is_new).count(), 8);
    }

    #[test]
    fn select_words_returns_all_available_when_total_pool_is_small() {
        let store = test_store();
        let user_id = "u1";
        let mm = MemoryModelConfig::default();
        let ws = WordSelectorConfig::default();
        let elo = EloConfig::default();
        let configs = default_selection_configs(&mm, &ws, &elo);
        let strategy = StrategyParams {
            difficulty: 0.5,
            batch_size: 10,
            new_ratio: 0.5,
            interval_scale: 1.0,
            review_mode: false,
        };
        let now_ms = Utc::now().timestamp_millis();

        let mut candidate_ids = Vec::new();
        for idx in 0..2 {
            let word_id = format!("new-small-{idx}");
            store.upsert_word(&sample_word(&word_id, 0.3)).unwrap();
            candidate_ids.push(word_id);
        }
        for idx in 0..3 {
            let word_id = format!("review-small-{idx}");
            store.upsert_word(&sample_word(&word_id, 0.7)).unwrap();
            persist_review_state(
                &store,
                user_id,
                &word_id,
                review_state_for_recall(0.35 + idx as f64 * 0.05, now_ms, &mm),
            );
            candidate_ids.push(word_id);
        }

        let selected = select_words(
            &store,
            user_id,
            &candidate_ids,
            &strategy,
            10,
            None,
            &configs,
        )
        .unwrap();

        assert_eq!(selected.len(), 5);
    }

    #[test]
    fn select_words_treats_mastery_state_without_word_learning_state_as_review() {
        let store = test_store();
        let user_id = "u1";
        let mm = MemoryModelConfig::default();
        let ws = WordSelectorConfig::default();
        let elo = EloConfig::default();
        let configs = default_selection_configs(&mm, &ws, &elo);
        let strategy = StrategyParams {
            difficulty: 0.5,
            batch_size: 1,
            new_ratio: 0.0,
            interval_scale: 1.0,
            review_mode: true,
        };
        let now_ms = Utc::now().timestamp_millis();
        let word_id = "review-only";
        store.upsert_word(&sample_word(word_id, 0.5)).unwrap();
        persist_review_state(
            &store,
            user_id,
            word_id,
            review_state_for_recall(0.45, now_ms, &mm),
        );

        let selected = select_words(
            &store,
            user_id,
            &[word_id.to_string()],
            &strategy,
            1,
            None,
            &configs,
        )
        .unwrap();

        assert_eq!(selected.len(), 1);
        assert!(!selected[0].is_new);
    }

    #[test]
    fn high_recall_review_words_do_not_receive_context_bonus() {
        let store = test_store();
        let user_id = "u1";
        let mm = MemoryModelConfig::default();
        let ws = WordSelectorConfig::default();
        let elo = EloConfig::default();
        let configs = default_selection_configs(&mm, &ws, &elo);
        let strategy = StrategyParams {
            difficulty: 0.5,
            batch_size: 2,
            new_ratio: 0.0,
            interval_scale: 1.0,
            review_mode: true,
        };
        let now_ms = Utc::now().timestamp_millis();

        let high_word = "review-high";
        let low_word = "review-low";
        store.upsert_word(&sample_word(high_word, 0.6)).unwrap();
        store.upsert_word(&sample_word(low_word, 0.6)).unwrap();
        persist_review_state(
            &store,
            user_id,
            high_word,
            review_state_for_recall(0.95, now_ms, &mm),
        );
        persist_review_state(
            &store,
            user_id,
            low_word,
            review_state_for_recall(0.45, now_ms, &mm),
        );

        let context = SessionSelectionContext {
            error_prone_word_ids: vec![high_word.to_string()],
            recently_mastered_word_ids: vec![high_word.to_string()],
            confusion_exclude_word_ids: Vec::new(),
            temporal_boost: 1.0,
        };
        let selected = select_words(
            &store,
            user_id,
            &[high_word.to_string(), low_word.to_string()],
            &strategy,
            2,
            Some(&context),
            &configs,
        )
        .unwrap();

        assert_eq!(selected[0].word_id, low_word);
    }

    #[test]
    fn confusion_isolation_dampens_excluded_word_rank() {
        let store = test_store();
        let user_id = "u1";
        let mm = MemoryModelConfig::default();
        let mut ws = WordSelectorConfig::default();
        ws.confusion_isolation_dampen = 0.1; // <1 启用惩罚
        let elo = EloConfig::default();
        let configs = default_selection_configs(&mm, &ws, &elo);
        let strategy = StrategyParams {
            difficulty: 0.5,
            batch_size: 2,
            new_ratio: 0.0,
            interval_scale: 1.0,
            review_mode: true,
        };
        let now_ms = Utc::now().timestamp_millis();

        let a = "review-a";
        let b = "review-b";
        store.upsert_word(&sample_word(a, 0.6)).unwrap();
        store.upsert_word(&sample_word(b, 0.6)).unwrap();
        // 同等 recall → 同等 base score；仅靠混淆隔离区分
        persist_review_state(&store, user_id, a, review_state_for_recall(0.45, now_ms, &mm));
        persist_review_state(&store, user_id, b, review_state_for_recall(0.45, now_ms, &mm));

        let context = SessionSelectionContext {
            error_prone_word_ids: Vec::new(),
            recently_mastered_word_ids: Vec::new(),
            confusion_exclude_word_ids: vec![a.to_string()],
            temporal_boost: 1.0,
        };
        let selected = select_words(
            &store,
            user_id,
            &[a.to_string(), b.to_string()],
            &strategy,
            2,
            Some(&context),
            &configs,
        )
        .unwrap();

        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].word_id, b, "未隔离的 b 应排第一");
        assert_eq!(selected[1].word_id, a, "被隔离的 a 应被压到末位");
        let sa = selected.iter().find(|w| w.word_id == a).unwrap().score;
        let sb = selected.iter().find(|w| w.word_id == b).unwrap().score;
        assert!(sa < sb, "dampen 后 a({sa}) 应 < b({sb})");
    }

    #[test]
    fn multi_trace_min_recall_aggregation_uses_weakest_mode() {
        let store = test_store();
        let user_id = "u1";
        let mm = MemoryModelConfig::default();
        let ws = WordSelectorConfig::default();
        let elo = EloConfig::default();
        let mut configs = default_selection_configs(&mm, &ws, &elo);
        configs.multi_trace_enabled = true; // ③ 开启
        let strategy = StrategyParams {
            difficulty: 0.5,
            batch_size: 1,
            new_ratio: 0.0,
            interval_scale: 1.0,
            review_mode: true,
        };
        let now_ms = Utc::now().timestamp_millis();

        let word = "w-multi";
        store.upsert_word(&sample_word(word, 0.6)).unwrap();
        // 强痕迹（recall 0.95，已掌握会被 suppress 成 0.001）+ 弱痕迹（recall 0.40，高 urgency）。
        // min-recall 聚合必须取弱者 → 最终 score 远大于 0.001。
        let strong = review_state_for_recall(0.95, now_ms, &mm);
        let weak = review_state_for_recall(0.40, now_ms, &mm);
        store
            .set_engine_algo_state(
                user_id,
                "mastery:w-multi:word-to-meaning",
                &serde_json::to_value(&strong).unwrap(),
            )
            .unwrap();
        store
            .set_engine_algo_state(
                user_id,
                "mastery:w-multi:meaning-to-word",
                &serde_json::to_value(&weak).unwrap(),
            )
            .unwrap();

        let selected = select_words(
            &store,
            user_id,
            &[word.to_string()],
            &strategy,
            1,
            None,
            &configs,
        )
        .unwrap();
        assert_eq!(selected.len(), 1);
        assert!(!selected[0].is_new, "有痕迹 → 复习词");
        assert!(
            selected[0].score > 0.01,
            "应按最弱题型(recall 0.40)打 urgency，而非被强题型已掌握抑制；实际 score={}",
            selected[0].score
        );
    }
}
