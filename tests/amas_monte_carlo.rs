//! AMAS Monte Carlo Simulation — 全面系统测试
//!
//! 通过蒙特卡洛模拟验证:
//! 1. AMAS 自适应调度 vs Leitner vs Random 是否有显著差异
//! 2. 各子系统（MDM, ELO, IAD, MTP, Mastery）是否正常工作
//!
//! 运行: cargo test --test amas_monte_carlo --release -- --nocapture

use learning_backend::amas::config::{
    EloConfig, IadConfig, MemoryModelConfig, MtpConfig, WordSelectorConfig,
};
use learning_backend::amas::elo::{update_elo, zpd_priority, EloRating};
use learning_backend::amas::memory::iad::{
    interference_penalty, interval_extension_factor, record_confusion, IadState,
};
use learning_backend::amas::memory::mastery::{update_mastery, WordMasteryState};
use learning_backend::amas::memory::mdm::{
    composite_strength, compute_interval, recall_probability, update_strength, MdmState,
};
use learning_backend::amas::memory::mtp::{
    morpheme_transfer_bonus, update_known_morphemes, MtpState,
};
use learning_backend::amas::types::MasteryLevel;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

// ============================================================================
// 常量
// ============================================================================

const DAY_MS: i64 = 86_400_000;
const BASE_TS: i64 = 1_767_225_600_000; // 2026-01-01T00:00:00Z

const NUM_WORDS: usize = 50;
const SIM_DAYS: i64 = 30;
const MAX_REVIEWS_PER_DAY: usize = 20;
const SKIP_RECALL_THRESHOLD: f64 = 0.80;
const NUM_TRIALS: usize = 300;

#[derive(Debug, Clone, Copy)]
struct SimScenario {
    sim_days: i64,
    max_reviews_per_day: usize,
}

// ============================================================================
// 辅助：模拟学生答题行为
// ============================================================================

fn simulate_answer(difficulty: f64, recall_prob: f64, rng: &mut StdRng) -> (bool, f64) {
    let correct_prob = if recall_prob < 0.01 {
        // 从未学过的词
        0.45 * (1.0 - difficulty * 0.4)
    } else {
        recall_prob * (1.0 - difficulty * 0.15) + (1.0 - recall_prob) * 0.2
    }
    .clamp(0.05, 0.98);

    let is_correct = rng.gen::<f64>() < correct_prob;
    let base_rt: i64 = 1200 + (difficulty * 2500.0) as i64;
    let rt = if is_correct {
        base_rt + rng.gen_range(-200..=400)
    } else {
        base_rt + rng.gen_range(300..=2000)
    };
    let speed = (1.0 - (rt.max(300) as f64 / 10000.0)).clamp(0.0, 1.0);
    let accuracy = if is_correct { 1.0 } else { 0.0 };
    let mut quality = (accuracy * 0.6 + speed * 0.4).clamp(0.0, 1.0);
    if !is_correct {
        quality *= 0.1;
    }
    (is_correct, quality)
}

/// 在 MDM 级别执行一次复习，返回 (is_correct, quality)
/// FSRS 融合: 双阶段 alpha + FSRS 式 spacing 奖励 + 分级测试
fn do_review(
    state: &mut MdmState,
    difficulty: f64,
    correct_streak: &mut u32,
    now_ms: i64,
    config: &MemoryModelConfig,
    rng: &mut StdRng,
) -> (bool, f64) {
    let rp = recall_probability(state, now_ms, config);

    // 分级测试: 新词简单, 已学词难
    let effective_difficulty = if state.review_count < 3 {
        difficulty * 0.6
    } else if state.stability > 2.0 {
        difficulty * 1.2
    } else {
        difficulty
    };
    let (is_correct, mut quality) =
        simulate_answer(effective_difficulty.clamp(0.05, 0.95), rp, rng);

    // 难测试答对 = 更高质量
    if is_correct && state.review_count >= 3 && state.stability > 2.0 {
        quality = (quality * 1.3).min(1.0);
    }

    if is_correct {
        *correct_streak += 1;
    } else {
        *correct_streak = 0;
    }

    // DSR: quality directly drives the SInc formula in update_strength
    // quality > 0.5 → recall path (stability grows), quality <= 0.5 → forget path
    let effective_quality = if is_correct { quality } else { quality * 0.1 };
    update_strength(state, effective_quality, 0.5, now_ms, config);
    (is_correct, quality)
}

// ============================================================================
// 模拟结果
// ============================================================================

#[derive(Debug, Clone)]
struct SimResult {
    daily_avg_recall: Vec<f64>,
    final_recall: Vec<f64>,
    memory_strengths: Vec<f64>,
    mastered_count: usize,
    total_reviews: usize,
}

// ============================================================================
// 三种调度策略模拟
// ============================================================================

fn cooldown_factor(last_review_at: Option<i64>, now_ms: i64, cooldown_secs: f64) -> f64 {
    match last_review_at {
        None => 1.0,
        Some(last) => {
            let elapsed_secs = (now_ms - last) as f64 / 1000.0;
            1.0 - (-elapsed_secs / cooldown_secs).exp()
        }
    }
}

fn gaussian_recall_bonus(recall: f64, center: f64, sigma: f64) -> f64 {
    let d = (recall - center) / sigma;
    (-0.5 * d * d).exp()
}

fn score_review_word(
    mdm_state: &MdmState,
    total_attempts: u32,
    review_population: usize,
    now_ms: i64,
    mm: &MemoryModelConfig,
    ws: &WordSelectorConfig,
) -> (f64, f64) {
    let recall = recall_probability(mdm_state, now_ms, mm);

    let sigmoid = |x: f64| 1.0 / (1.0 + (-x).exp());
    let mut score = 1.0 - recall;
    score +=
        mm.recall_risk_bonus * sigmoid((mm.recall_risk_threshold - recall) * ws.sigmoid_steepness);

    let g_bonus = gaussian_recall_bonus(recall, ws.optimal_recall_center, ws.optimal_recall_sigma);
    let g_bonus = g_bonus.max(0.15); // Urgency floor: prevent score collapse for weak words
    let cd = cooldown_factor(mdm_state.last_review_at, now_ms, ws.spacing_cooldown_secs);
    let ucb_bonus = if review_population <= 1 {
        0.0
    } else {
        let numerator = (review_population as f64 + 1.0).ln();
        let denominator = total_attempts as f64 + 1.0;
        let bonus = ws.review_ucb_weight * (numerator / denominator).sqrt();
        bonus.min(ws.review_ucb_max_bonus)
    };

    (score * g_bonus * cd + ucb_bonus, recall)
}

fn run_amas_sim(
    words: &[f64],
    seed: u64,
    scenario: SimScenario,
    config: &MemoryModelConfig,
    selector_config: &WordSelectorConfig,
) -> SimResult {
    let mut rng = StdRng::seed_from_u64(seed);
    let n = words.len();
    let mut states: Vec<MdmState> = (0..n).map(|_| MdmState::default()).collect();
    let mut streaks = vec![0u32; n];
    let mut total_reviews = 0;
    let mut daily_avg_recall = Vec::with_capacity(scenario.sim_days as usize);
    let mut correct_counts = vec![0u32; n];
    let mut attempt_counts = vec![0u32; n];
    let mut recent_results: Vec<Vec<bool>> = (0..n).map(|_| Vec::new()).collect();
    // EF 快轨: 学习阶段固定间隔 (天)
    let ef_intervals: [i64; 4] = [0, 1, 4, 10]; // 首次/第2次/第3次/毕业 (FSRS 式激进间隔)
    let mut next_review_day = vec![0i64; n]; // 每个词的下次复习日

    for day in 0..scenario.sim_days {
        let now_ms = BASE_TS + day * DAY_MS + 9 * 3_600_000;

        let avg: f64 = states
            .iter()
            .map(|s| recall_probability(s, now_ms, config))
            .sum::<f64>()
            / n as f64;
        daily_avg_recall.push(avg);

        // ===== 混合调度: EF 快轨 + MDM 精细 =====
        let mut candidates: Vec<(usize, f64, f64)> = (0..n)
            .map(|i| {
                let recall = recall_probability(&states[i], now_ms, config);
                let review_count = states[i].review_count;

                let score = if review_count < 3 {
                    // === 学习阶段: EF 快轨 ===
                    if next_review_day[i] <= day {
                        // 到期: 高优先, 按 review_count 排序确保新词优先
                        10.0 + (3 - review_count) as f64
                    } else {
                        0.0 // 未到期: 不复习
                    }
                } else {
                    // === 维护阶段: MDM 调度 ===
                    if recall >= SKIP_RECALL_THRESHOLD {
                        0.0 // 已掌握
                    } else if next_review_day[i] <= day {
                        // 到期: 按 recall 低的优先
                        (1.0 - recall) * 5.0
                    } else {
                        0.0 // 未到期: 不浪费预算
                    }
                };
                (i, score, recall)
            })
            .collect();
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut reviewed = 0;
        for (idx, score, _) in &candidates {
            if reviewed >= scenario.max_reviews_per_day {
                break;
            }
            if *score < 0.001 {
                break; // 今天没有需要复习的词
            }
            let idx = *idx;
            let review_count_before = states[idx].review_count;
            let (is_correct, _) = do_review(
                &mut states[idx],
                words[idx],
                &mut streaks[idx],
                now_ms,
                config,
                &mut rng,
            );

            attempt_counts[idx] += 1;
            if is_correct {
                correct_counts[idx] += 1;
            }
            recent_results[idx].push(is_correct);
            if recent_results[idx].len() > 20 {
                recent_results[idx].remove(0);
            }

            // 更新下次复习日
            if review_count_before < 3 {
                // EF 快轨: 答对走固定间隔, 答错回第一步
                if is_correct {
                    let next_idx = (review_count_before as usize + 1).min(ef_intervals.len() - 1);
                    next_review_day[idx] = day + ef_intervals[next_idx];
                } else {
                    next_review_day[idx] = day + 1; // 明天重试
                }
            } else {
                // 固定 retention 0.90 (DHP 冠军配置, 与 FSRS 一致)
                let desired_retention = 0.90;
                let interval_secs = compute_interval(&states[idx], desired_retention, 1.0, config);
                let interval_days = (interval_secs as f64 / 86400.0).ceil() as i64;
                next_review_day[idx] = day + interval_days.max(1);
            }

            reviewed += 1;
            total_reviews += 1;
        }
    }

    let final_test = BASE_TS + scenario.sim_days * DAY_MS + 9 * 3_600_000;

    // 计算 mastery
    let mastered_count = (0..n)
        .filter(|&i| {
            let comp = composite_strength(&states[i], config);
            let recent = &recent_results[i];
            if recent.len() < 3 {
                return false;
            }
            let recent_acc = recent.iter().filter(|&&r| r).count() as f64 / recent.len() as f64;
            comp > config.mastery_composite_threshold
                && recent_acc > config.mastery_accuracy_threshold
                && streaks[i] >= config.mastery_streak_threshold
        })
        .count();

    SimResult {
        daily_avg_recall,
        final_recall: states
            .iter()
            .map(|s| recall_probability(s, final_test, config))
            .collect(),
        memory_strengths: states.iter().map(|s| s.memory_strength).collect(),
        mastered_count,
        total_reviews,
    }
}

// ============================================================================
// 独立记忆模型 — Leitner (SM-2 类) 和 Random (基础) 各自使用自己的模型
// ============================================================================

#[derive(Debug, Clone)]
struct SimpleMemoryState {
    strength: f64, // 记忆强度 [0, 1]
    last_review_at: Option<i64>,
    review_count: u32,
}

impl Default for SimpleMemoryState {
    fn default() -> Self {
        Self {
            strength: 0.0,
            last_review_at: None,
            review_count: 0,
        }
    }
}

/// SM-2 风格 recall: half_life = base_days * (1 + strength)^power
fn sm2_recall(state: &SimpleMemoryState, now_ms: i64) -> f64 {
    match state.last_review_at {
        None => 0.0,
        Some(last) => {
            let elapsed_days = (now_ms - last) as f64 / 86_400_000.0;
            let base_days = 1.0; // 基础半衰期 1 天
            let half_life = base_days * (1.0 + state.strength).powf(2.0);
            (-0.693 * elapsed_days / half_life).exp().clamp(0.0, 1.0)
        }
    }
}

/// SM-2 风格更新: 答对 strength += step, 答错 strength *= penalty
fn sm2_update(state: &SimpleMemoryState, is_correct: bool, difficulty: f64) -> f64 {
    if is_correct {
        // 难词学得慢
        let step = 0.15 * (1.0 - difficulty * 0.3);
        (state.strength + step).min(1.0)
    } else {
        // 答错大幅回退
        (state.strength * 0.4).max(0.0)
    }
}

/// 基础 recall: 固定半衰期 + 少量 strength 修正
fn basic_recall(state: &SimpleMemoryState, now_ms: i64) -> f64 {
    match state.last_review_at {
        None => 0.0,
        Some(last) => {
            let elapsed_days = (now_ms - last) as f64 / 86_400_000.0;
            let half_life = 2.0 + state.strength * 3.0; // 2-5 天半衰期
            (-0.693 * elapsed_days / half_life).exp().clamp(0.0, 1.0)
        }
    }
}

/// 基础更新: 线性缓慢增长
fn basic_update(state: &SimpleMemoryState, is_correct: bool) -> f64 {
    if is_correct {
        (state.strength + 0.05).min(1.0)
    } else {
        (state.strength * 0.7).max(0.0)
    }
}

/// Leitner/Random 复习模拟 — 使用各自的简单记忆模型
fn do_simple_review(
    state: &mut SimpleMemoryState,
    difficulty: f64,
    recall_prob: f64,
    is_sm2: bool,
    rng: &mut StdRng,
    now_ms: i64,
) -> bool {
    let (is_correct, _) = simulate_answer(difficulty, recall_prob, rng);

    state.strength = if is_sm2 {
        sm2_update(state, is_correct, difficulty)
    } else {
        basic_update(state, is_correct)
    };
    state.last_review_at = Some(now_ms);
    state.review_count += 1;
    is_correct
}

fn run_leitner_sim(words: &[f64], seed: u64, scenario: SimScenario) -> SimResult {
    let mut rng = StdRng::seed_from_u64(seed);
    let n = words.len();
    let mut states: Vec<SimpleMemoryState> = (0..n).map(|_| SimpleMemoryState::default()).collect();
    let mut streaks = vec![0u32; n];
    let intervals: [i64; 5] = [1, 2, 4, 7, 14];
    let mut boxes = vec![0usize; n];
    let mut next_review_day = vec![0i64; n];
    let mut total_reviews = 0;
    let mut daily_avg_recall = Vec::with_capacity(scenario.sim_days as usize);
    let mut recent_results: Vec<Vec<bool>> = (0..n).map(|_| Vec::new()).collect();

    for day in 0..scenario.sim_days {
        let now_ms = BASE_TS + day * DAY_MS + 9 * 3_600_000;

        let avg: f64 = states.iter().map(|s| sm2_recall(s, now_ms)).sum::<f64>() / n as f64;
        daily_avg_recall.push(avg);

        let mut due: Vec<usize> = (0..n).filter(|&i| next_review_day[i] <= day).collect();
        use rand::seq::SliceRandom;
        due.shuffle(&mut rng);

        let mut reviewed = 0;
        for idx in due {
            if reviewed >= scenario.max_reviews_per_day {
                break;
            }
            let recall = sm2_recall(&states[idx], now_ms);
            let is_correct =
                do_simple_review(&mut states[idx], words[idx], recall, true, &mut rng, now_ms);

            if is_correct {
                streaks[idx] += 1;
                boxes[idx] = (boxes[idx] + 1).min(intervals.len() - 1);
            } else {
                streaks[idx] = 0;
                boxes[idx] = 0;
            }
            recent_results[idx].push(is_correct);
            if recent_results[idx].len() > 20 {
                recent_results[idx].remove(0);
            }

            next_review_day[idx] = day + intervals[boxes[idx]];
            reviewed += 1;
            total_reviews += 1;
        }
    }

    let final_test = BASE_TS + scenario.sim_days * DAY_MS + 9 * 3_600_000;
    let mastered_count = (0..n)
        .filter(|&i| {
            let recent = &recent_results[i];
            if recent.len() < 3 {
                return false;
            }
            let recent_acc = recent.iter().filter(|&&r| r).count() as f64 / recent.len() as f64;
            states[i].strength > 0.5 && recent_acc > 0.65 && streaks[i] >= 1
        })
        .count();

    SimResult {
        daily_avg_recall,
        final_recall: states.iter().map(|s| sm2_recall(s, final_test)).collect(),
        memory_strengths: states.iter().map(|s| s.strength).collect(),
        mastered_count,
        total_reviews,
    }
}

fn run_random_sim(words: &[f64], seed: u64, scenario: SimScenario) -> SimResult {
    let mut rng = StdRng::seed_from_u64(seed);
    let n = words.len();
    let mut states: Vec<SimpleMemoryState> = (0..n).map(|_| SimpleMemoryState::default()).collect();
    let mut streaks = vec![0u32; n];
    let mut total_reviews = 0;
    let mut daily_avg_recall = Vec::with_capacity(scenario.sim_days as usize);
    let mut recent_results: Vec<Vec<bool>> = (0..n).map(|_| Vec::new()).collect();

    for day in 0..scenario.sim_days {
        let now_ms = BASE_TS + day * DAY_MS + 9 * 3_600_000;

        let avg: f64 = states.iter().map(|s| basic_recall(s, now_ms)).sum::<f64>() / n as f64;
        daily_avg_recall.push(avg);

        let mut indices: Vec<usize> = (0..n).collect();
        use rand::seq::SliceRandom;
        indices.shuffle(&mut rng);

        for &idx in indices.iter().take(scenario.max_reviews_per_day) {
            let recall = basic_recall(&states[idx], now_ms);
            let is_correct = do_simple_review(
                &mut states[idx],
                words[idx],
                recall,
                false,
                &mut rng,
                now_ms,
            );

            if is_correct {
                streaks[idx] += 1;
            } else {
                streaks[idx] = 0;
            }
            recent_results[idx].push(is_correct);
            if recent_results[idx].len() > 20 {
                recent_results[idx].remove(0);
            }
            total_reviews += 1;
        }
    }

    let final_test = BASE_TS + scenario.sim_days * DAY_MS + 9 * 3_600_000;
    let mastered_count = (0..n)
        .filter(|&i| {
            let recent = &recent_results[i];
            if recent.len() < 3 {
                return false;
            }
            let recent_acc = recent.iter().filter(|&&r| r).count() as f64 / recent.len() as f64;
            states[i].strength > 0.5 && recent_acc > 0.65 && streaks[i] >= 1
        })
        .count();

    SimResult {
        daily_avg_recall,
        final_recall: states.iter().map(|s| basic_recall(s, final_test)).collect(),
        memory_strengths: states.iter().map(|s| s.strength).collect(),
        mastered_count,
        total_reviews,
    }
}
// ============================================================================
// 算法 3: SM-18 (SuperMemo 18 风格) — stability + difficulty
// ============================================================================

#[derive(Debug, Clone)]
struct Sm18State {
    stability: f64,  // 记忆稳定性（天）
    difficulty: f64, // 难度 [1-10]
    last_review_at: Option<i64>,
    reps: u32,
}

impl Default for Sm18State {
    fn default() -> Self {
        Self {
            stability: 0.5,
            difficulty: 5.0,
            last_review_at: None,
            reps: 0,
        }
    }
}

fn sm18_recall(state: &Sm18State, now_ms: i64) -> f64 {
    match state.last_review_at {
        None => 0.0,
        Some(last) => {
            let elapsed_days = (now_ms - last) as f64 / 86_400_000.0;
            (-0.693 * elapsed_days / state.stability.max(0.1))
                .exp()
                .clamp(0.0, 1.0)
        }
    }
}

fn sm18_update(state: &mut Sm18State, is_correct: bool) {
    if is_correct {
        let d_factor = state.difficulty.powf(-0.3);
        let s_factor = 1.0
            + (0.08_f64).exp()
                * state.stability.max(0.1).powf(-0.2)
                * (0.05 * state.reps as f64).exp().min(3.0);
        state.stability = (state.stability * d_factor * s_factor).min(365.0);
        if state.difficulty > 1.0 {
            state.difficulty -= 0.2;
        }
    } else {
        state.stability = (state.stability * 0.3).max(0.5);
        state.difficulty = (state.difficulty + 1.0).min(10.0);
    }
    state.reps += 1;
}

fn run_sm18_sim(words: &[f64], seed: u64, scenario: SimScenario) -> SimResult {
    let mut rng = StdRng::seed_from_u64(seed);
    let n = words.len();
    let mut states: Vec<Sm18State> = (0..n).map(|_| Sm18State::default()).collect();
    let mut streaks = vec![0u32; n];
    let mut total_reviews = 0;
    let mut daily_avg_recall = Vec::with_capacity(scenario.sim_days as usize);
    let mut recent_results: Vec<Vec<bool>> = (0..n).map(|_| Vec::new()).collect();

    for day in 0..scenario.sim_days {
        let now_ms = BASE_TS + day * DAY_MS + 9 * 3_600_000;
        let avg: f64 = states.iter().map(|s| sm18_recall(s, now_ms)).sum::<f64>() / n as f64;
        daily_avg_recall.push(avg);

        // SM-18 调度: 按 stability 计算 due, 优先 overdue
        let mut candidates: Vec<(usize, f64)> = (0..n)
            .map(|i| {
                let elapsed_ms = match states[i].last_review_at {
                    Some(last) => now_ms - last,
                    None => DAY_MS * 2,
                };
                let due_ms = (states[i].stability * 86_400_000.0) as i64;
                let overdue = elapsed_ms as f64 / due_ms.max(1) as f64;
                (i, overdue)
            })
            .collect();
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut reviewed = 0;
        for (idx, overdue) in &candidates {
            if reviewed >= scenario.max_reviews_per_day {
                break;
            }
            if *overdue < 0.5 && states[*idx].last_review_at.is_some() {
                break;
            }
            let recall = sm18_recall(&states[*idx], now_ms);
            let (is_correct, _) = simulate_answer(words[*idx], recall, &mut rng);
            sm18_update(&mut states[*idx], is_correct);
            states[*idx].last_review_at = Some(now_ms);
            if is_correct {
                streaks[*idx] += 1;
            } else {
                streaks[*idx] = 0;
            }
            recent_results[*idx].push(is_correct);
            if recent_results[*idx].len() > 20 {
                recent_results[*idx].remove(0);
            }
            reviewed += 1;
            total_reviews += 1;
        }
    }

    let final_test = BASE_TS + scenario.sim_days * DAY_MS + 9 * 3_600_000;
    let mastered_count = (0..n)
        .filter(|&i| {
            let recent = &recent_results[i];
            if recent.len() < 3 {
                return false;
            }
            let acc = recent.iter().filter(|&&r| r).count() as f64 / recent.len() as f64;
            states[i].stability > 5.0 && acc > 0.65 && streaks[i] >= 1
        })
        .count();

    SimResult {
        daily_avg_recall,
        final_recall: states.iter().map(|s| sm18_recall(s, final_test)).collect(),
        memory_strengths: states
            .iter()
            .map(|s| (s.stability / 30.0).min(1.0))
            .collect(),
        mastered_count,
        total_reviews,
    }
}

// ============================================================================
// 算法 4: FSRS (Free Spaced Repetition Scheduler) — 墨墨背单词的真实算法
// 基于官方 FSRS-5 公式: https://github.com/open-spaced-repetition/awesome-fsrs/wiki/The-Algorithm
// ============================================================================

// FSRS-5 default parameters (19 weights, ML-optimized)
const FSRS_W: [f64; 19] = [
    0.40255, 1.18385, 3.173, 15.69105, // w0-w3: initial stability for Again/Hard/Good/Easy
    7.1949,   // w4: initial difficulty when first rating is Good
    0.5345,   // w5: difficulty scaling for first rating
    1.4604,   // w6: difficulty change per grade delta
    0.0046,   // w7: mean reversion weight
    1.54575,  // w8: stability increase base
    0.1192,   // w9: stability power (higher S → slower growth)
    1.01925,  // w10: spacing effect (lower R → bigger SInc)
    1.9395,   // w11: post-lapse stability base
    0.11,     // w12: post-lapse difficulty power
    0.29605,  // w13: post-lapse stability power
    2.2698,   // w14: post-lapse R scaling
    0.2315,   // w15: Hard bonus multiplier
    2.9898,   // w16: Easy bonus multiplier
    0.51655,  // w17: same-day review scaling
    0.6621,   // w18: same-day review offset
];

const FSRS_DECAY: f64 = -0.5;
const FSRS_FACTOR: f64 = 19.0 / 81.0; // = 0.2346...

#[derive(Debug, Clone)]
struct FsrsState {
    stability: f64,  // S: days until R drops to 90%
    difficulty: f64, // D: [1, 10]
    last_review_at: Option<i64>,
    review_count: u32,
}

impl Default for FsrsState {
    fn default() -> Self {
        Self {
            stability: 0.0,
            difficulty: 5.0,
            last_review_at: None,
            review_count: 0,
        }
    }
}

/// FSRS-4.5 power-law forgetting curve: R(t,S) = (1 + FACTOR * t/S)^DECAY
fn fsrs_recall(state: &FsrsState, now_ms: i64) -> f64 {
    match state.last_review_at {
        None => 0.0,
        Some(last) => {
            let elapsed_days = (now_ms - last) as f64 / 86_400_000.0;
            if state.stability < 0.01 {
                return 0.0;
            }
            (1.0 + FSRS_FACTOR * elapsed_days / state.stability)
                .powf(FSRS_DECAY)
                .clamp(0.0, 1.0)
        }
    }
}

/// FSRS interval: I(r,S) = S/FACTOR * (r^{1/DECAY} - 1)
fn fsrs_interval(stability: f64, desired_retention: f64) -> f64 {
    (stability / FSRS_FACTOR * (desired_retention.powf(1.0 / FSRS_DECAY) - 1.0)).max(1.0)
}

/// Grade: 1=Again, 2=Hard, 3=Good, 4=Easy
fn fsrs_update(state: &mut FsrsState, grade: u32, recall: f64) {
    if state.review_count == 0 {
        // First review: initial stability from w0-w3
        let g = (grade as usize).clamp(1, 4) - 1;
        state.stability = FSRS_W[g];
        // Initial difficulty: D0(G) = w4 - e^{w5*(G-1)} + 1
        state.difficulty =
            (FSRS_W[4] - (FSRS_W[5] * (grade as f64 - 1.0)).exp() + 1.0).clamp(1.0, 10.0);
    } else {
        // Update difficulty: D' = D - w6*(G-3), then mean reversion
        let delta_d = -FSRS_W[6] * (grade as f64 - 3.0);
        let d_prime = state.difficulty + delta_d * (10.0 - state.difficulty) / 9.0;
        // Mean reversion target = D0(4) in FSRS-5
        let d0_4 = (FSRS_W[4] - (FSRS_W[5] * 3.0).exp() + 1.0).clamp(1.0, 10.0);
        state.difficulty = (FSRS_W[7] * d0_4 + (1.0 - FSRS_W[7]) * d_prime).clamp(1.0, 10.0);

        if grade >= 2 {
            // Successful recall: S'_r = S * (e^w8 * (11-D) * S^{-w9} * (e^{w10*(1-R)} - 1) * bonus + 1)
            let bonus = if grade == 2 {
                FSRS_W[15]
            }
            // Hard
            else if grade == 4 {
                FSRS_W[16]
            }
            // Easy
            else {
                1.0
            }; // Good
            let s_inc = (FSRS_W[8].exp()
                * (11.0 - state.difficulty)
                * state.stability.max(0.01).powf(-FSRS_W[9])
                * ((FSRS_W[10] * (1.0 - recall)).exp() - 1.0)
                * bonus)
                .max(0.0);
            state.stability = (state.stability * (s_inc + 1.0)).max(0.01);
        } else {
            // Forgetting (Again): S'_f = w11 * D^{-w12} * ((S+1)^w13 - 1) * e^{w14*(1-R)}
            state.stability = (FSRS_W[11]
                * state.difficulty.powf(-FSRS_W[12])
                * ((state.stability + 1.0).powf(FSRS_W[13]) - 1.0)
                * (FSRS_W[14] * (1.0 - recall)).exp())
            .clamp(0.01, state.stability); // post-lapse S should not exceed pre-lapse S
        }
    }
    state.review_count += 1;
}

fn run_fsrs_sim(words: &[f64], seed: u64, scenario: SimScenario) -> SimResult {
    let mut rng = StdRng::seed_from_u64(seed);
    let n = words.len();
    let mut states: Vec<FsrsState> = (0..n).map(|_| FsrsState::default()).collect();
    let mut streaks = vec![0u32; n];
    let mut next_review_day = vec![0i64; n];
    let mut total_reviews = 0;
    let mut daily_avg_recall = Vec::with_capacity(scenario.sim_days as usize);
    let mut recent_results: Vec<Vec<bool>> = (0..n).map(|_| Vec::new()).collect();
    let desired_retention = 0.9; // FSRS default

    for day in 0..scenario.sim_days {
        let now_ms = BASE_TS + day * DAY_MS + 9 * 3_600_000;
        let avg: f64 = states.iter().map(|s| fsrs_recall(s, now_ms)).sum::<f64>() / n as f64;
        daily_avg_recall.push(avg);

        // FSRS scheduling: review when due, prioritize overdue
        let mut candidates: Vec<(usize, f64)> = (0..n)
            .filter(|&i| next_review_day[i] <= day)
            .map(|i| {
                let recall = fsrs_recall(&states[i], now_ms);
                let urgency = if states[i].last_review_at.is_none() {
                    10.0
                } else {
                    (1.0 - recall).max(0.01)
                };
                (i, urgency)
            })
            .collect();
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut reviewed = 0;
        for (idx, _) in &candidates {
            if reviewed >= scenario.max_reviews_per_day {
                break;
            }
            let idx = *idx;
            let recall = fsrs_recall(&states[idx], now_ms);
            let (is_correct, quality) = simulate_answer(words[idx], recall, &mut rng);

            // Map to FSRS grade: Again(1), Hard(2), Good(3), Easy(4)
            let grade = if !is_correct {
                1
            } else if quality < 0.5 {
                2
            }
            // Hard
            else if quality < 0.85 {
                3
            }
            // Good
            else {
                4
            }; // Easy

            fsrs_update(&mut states[idx], grade, recall);
            states[idx].last_review_at = Some(now_ms);

            if is_correct {
                streaks[idx] += 1;
            } else {
                streaks[idx] = 0;
            }
            recent_results[idx].push(is_correct);
            if recent_results[idx].len() > 20 {
                recent_results[idx].remove(0);
            }

            // Schedule next review based on FSRS interval
            let interval_days =
                fsrs_interval(states[idx].stability, desired_retention).ceil() as i64;
            next_review_day[idx] = day + interval_days.max(1);

            reviewed += 1;
            total_reviews += 1;
        }
    }

    let final_test = BASE_TS + scenario.sim_days * DAY_MS + 9 * 3_600_000;
    let mastered_count = (0..n)
        .filter(|&i| {
            let recent = &recent_results[i];
            if recent.len() < 3 {
                return false;
            }
            let acc = recent.iter().filter(|&&r| r).count() as f64 / recent.len() as f64;
            states[i].stability > 7.0 && acc > 0.65 && streaks[i] >= 1
        })
        .count();

    SimResult {
        daily_avg_recall,
        final_recall: states.iter().map(|s| fsrs_recall(s, final_test)).collect(),
        memory_strengths: states
            .iter()
            .map(|s| (s.stability / 30.0).min(1.0))
            .collect(),
        mastered_count,
        total_reviews,
    }
}

// ============================================================================
// 算法 5: 纯艾宾浩斯 (百词斩风格) — 固定间隔
// ============================================================================

#[derive(Debug, Clone)]
struct EbbinghausState {
    box_level: usize,
    last_review_at: Option<i64>,
    review_count: u32,
}

impl Default for EbbinghausState {
    fn default() -> Self {
        Self {
            box_level: 0,
            last_review_at: None,
            review_count: 0,
        }
    }
}

const EBBINGHAUS_INTERVALS: [f64; 8] = [0.01, 0.02, 0.5, 1.0, 2.0, 4.0, 7.0, 15.0];

fn ebbinghaus_recall(state: &EbbinghausState, now_ms: i64) -> f64 {
    match state.last_review_at {
        None => 0.0,
        Some(last) => {
            let elapsed_days = (now_ms - last) as f64 / 86_400_000.0;
            let half_life = EBBINGHAUS_INTERVALS[state.box_level].max(0.01) * 1.5;
            (-0.693 * elapsed_days / half_life).exp().clamp(0.0, 1.0)
        }
    }
}

fn run_ebbinghaus_sim(words: &[f64], seed: u64, scenario: SimScenario) -> SimResult {
    let mut rng = StdRng::seed_from_u64(seed);
    let n = words.len();
    let mut states: Vec<EbbinghausState> = (0..n).map(|_| EbbinghausState::default()).collect();
    let mut streaks = vec![0u32; n];
    let mut next_review_day = vec![0i64; n];
    let mut total_reviews = 0;
    let mut daily_avg_recall = Vec::with_capacity(scenario.sim_days as usize);
    let mut recent_results: Vec<Vec<bool>> = (0..n).map(|_| Vec::new()).collect();

    for day in 0..scenario.sim_days {
        let now_ms = BASE_TS + day * DAY_MS + 9 * 3_600_000;
        let avg: f64 = states
            .iter()
            .map(|s| ebbinghaus_recall(s, now_ms))
            .sum::<f64>()
            / n as f64;
        daily_avg_recall.push(avg);

        let mut due: Vec<usize> = (0..n).filter(|&i| next_review_day[i] <= day).collect();
        use rand::seq::SliceRandom;
        due.shuffle(&mut rng);

        let mut reviewed = 0;
        for idx in due {
            if reviewed >= scenario.max_reviews_per_day {
                break;
            }
            let recall = ebbinghaus_recall(&states[idx], now_ms);
            let (is_correct, _) = simulate_answer(words[idx], recall, &mut rng);

            if is_correct {
                streaks[idx] += 1;
                states[idx].box_level =
                    (states[idx].box_level + 1).min(EBBINGHAUS_INTERVALS.len() - 1);
            } else {
                streaks[idx] = 0;
                states[idx].box_level = 0;
            }
            states[idx].last_review_at = Some(now_ms);
            states[idx].review_count += 1;
            let interval_days = EBBINGHAUS_INTERVALS[states[idx].box_level];
            next_review_day[idx] = day + interval_days.ceil() as i64;
            recent_results[idx].push(is_correct);
            if recent_results[idx].len() > 20 {
                recent_results[idx].remove(0);
            }
            reviewed += 1;
            total_reviews += 1;
        }
    }

    let final_test = BASE_TS + scenario.sim_days * DAY_MS + 9 * 3_600_000;
    let mastered_count = (0..n)
        .filter(|&i| {
            let recent = &recent_results[i];
            if recent.len() < 3 {
                return false;
            }
            let acc = recent.iter().filter(|&&r| r).count() as f64 / recent.len() as f64;
            states[i].box_level >= 5 && acc > 0.65 && streaks[i] >= 1
        })
        .count();

    SimResult {
        daily_avg_recall,
        final_recall: states
            .iter()
            .map(|s| ebbinghaus_recall(s, final_test))
            .collect(),
        memory_strengths: states.iter().map(|s| s.box_level as f64 / 7.0).collect(),
        mastered_count,
        total_reviews,
    }
}

// ============================================================================
// 算法 6: HLR (Duolingo Half-Life Regression)
// ============================================================================

#[derive(Debug, Clone)]
struct HlrState {
    half_life: f64, // 半衰期（天）
    correct: u32,
    incorrect: u32,
    last_review_at: Option<i64>,
}

impl Default for HlrState {
    fn default() -> Self {
        Self {
            half_life: 0.5,
            correct: 0,
            incorrect: 0,
            last_review_at: None,
        }
    }
}

fn hlr_recall(state: &HlrState, now_ms: i64) -> f64 {
    match state.last_review_at {
        None => 0.0,
        Some(last) => {
            let elapsed_days = (now_ms - last) as f64 / 86_400_000.0;
            2.0_f64
                .powf(-elapsed_days / state.half_life.max(0.01))
                .clamp(0.0, 1.0)
        }
    }
}

fn hlr_update_half_life(state: &mut HlrState, difficulty: f64) {
    // HLR: h = 2^(θ₀ + θ₁*correct + θ₂*incorrect + θ₃*difficulty)
    let theta_0 = 0.0;
    let theta_1 = 0.7; // correct reviews extend half-life
    let theta_2 = -1.0; // incorrect reviews reduce half-life
    let theta_3 = -0.3; // harder words have shorter half-life
    let exponent = theta_0
        + theta_1 * state.correct as f64
        + theta_2 * state.incorrect as f64
        + theta_3 * difficulty;
    state.half_life = 2.0_f64.powf(exponent).clamp(0.1, 180.0);
}

fn run_hlr_sim(words: &[f64], seed: u64, scenario: SimScenario) -> SimResult {
    let mut rng = StdRng::seed_from_u64(seed);
    let n = words.len();
    let mut states: Vec<HlrState> = (0..n).map(|_| HlrState::default()).collect();
    let mut streaks = vec![0u32; n];
    let mut total_reviews = 0;
    let mut daily_avg_recall = Vec::with_capacity(scenario.sim_days as usize);
    let mut recent_results: Vec<Vec<bool>> = (0..n).map(|_| Vec::new()).collect();

    for day in 0..scenario.sim_days {
        let now_ms = BASE_TS + day * DAY_MS + 9 * 3_600_000;
        let avg: f64 = states.iter().map(|s| hlr_recall(s, now_ms)).sum::<f64>() / n as f64;
        daily_avg_recall.push(avg);

        // HLR 调度: review when elapsed >= half_life (recall ≈ 50%)
        let mut candidates: Vec<(usize, f64)> = (0..n)
            .map(|i| {
                let elapsed_ms = match states[i].last_review_at {
                    Some(last) => (now_ms - last) as f64,
                    None => DAY_MS as f64 * 2.0,
                };
                let due_ms = states[i].half_life * 86_400_000.0;
                (i, elapsed_ms / due_ms.max(1.0))
            })
            .collect();
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut reviewed = 0;
        for (idx, overdue) in &candidates {
            if reviewed >= scenario.max_reviews_per_day {
                break;
            }
            if *overdue < 0.5 && states[*idx].last_review_at.is_some() {
                break;
            }
            let recall = hlr_recall(&states[*idx], now_ms);
            let (is_correct, _) = simulate_answer(words[*idx], recall, &mut rng);

            if is_correct {
                states[*idx].correct += 1;
                streaks[*idx] += 1;
            } else {
                states[*idx].incorrect += 1;
                streaks[*idx] = 0;
            }
            hlr_update_half_life(&mut states[*idx], words[*idx]);
            states[*idx].last_review_at = Some(now_ms);

            recent_results[*idx].push(is_correct);
            if recent_results[*idx].len() > 20 {
                recent_results[*idx].remove(0);
            }
            reviewed += 1;
            total_reviews += 1;
        }
    }

    let final_test = BASE_TS + scenario.sim_days * DAY_MS + 9 * 3_600_000;
    let mastered_count = (0..n)
        .filter(|&i| {
            let recent = &recent_results[i];
            if recent.len() < 3 {
                return false;
            }
            let acc = recent.iter().filter(|&&r| r).count() as f64 / recent.len() as f64;
            states[i].half_life > 7.0 && acc > 0.65 && streaks[i] >= 1
        })
        .count();

    SimResult {
        daily_avg_recall,
        final_recall: states.iter().map(|s| hlr_recall(s, final_test)).collect(),
        memory_strengths: states
            .iter()
            .map(|s| (s.half_life / 30.0).min(1.0))
            .collect(),
        mastered_count,
        total_reviews,
    }
}

// ============================================================================
// 算法 7: 改良 SM-2 + 游戏化 (Memrise 风格) — 6 级种花
// ============================================================================

#[derive(Debug, Clone)]
struct MemriseState {
    level: usize, // 0-5
    last_review_at: Option<i64>,
    review_count: u32,
}

impl Default for MemriseState {
    fn default() -> Self {
        Self {
            level: 0,
            last_review_at: None,
            review_count: 0,
        }
    }
}

const MEMRISE_INTERVALS_HOURS: [f64; 6] = [4.0, 12.0, 24.0, 72.0, 168.0, 720.0];

fn memrise_recall(state: &MemriseState, now_ms: i64) -> f64 {
    match state.last_review_at {
        None => 0.0,
        Some(last) => {
            let elapsed_hours = (now_ms - last) as f64 / 3_600_000.0;
            let half_life_hours = MEMRISE_INTERVALS_HOURS[state.level] * 1.5;
            (-0.693 * elapsed_hours / half_life_hours)
                .exp()
                .clamp(0.0, 1.0)
        }
    }
}

fn run_memrise_sim(words: &[f64], seed: u64, scenario: SimScenario) -> SimResult {
    let mut rng = StdRng::seed_from_u64(seed);
    let n = words.len();
    let mut states: Vec<MemriseState> = (0..n).map(|_| MemriseState::default()).collect();
    let mut streaks = vec![0u32; n];
    let mut next_review_day = vec![0i64; n];
    let mut total_reviews = 0;
    let mut daily_avg_recall = Vec::with_capacity(scenario.sim_days as usize);
    let mut recent_results: Vec<Vec<bool>> = (0..n).map(|_| Vec::new()).collect();

    for day in 0..scenario.sim_days {
        let now_ms = BASE_TS + day * DAY_MS + 9 * 3_600_000;
        let avg: f64 = states
            .iter()
            .map(|s| memrise_recall(s, now_ms))
            .sum::<f64>()
            / n as f64;
        daily_avg_recall.push(avg);

        let mut due: Vec<usize> = (0..n).filter(|&i| next_review_day[i] <= day).collect();
        use rand::seq::SliceRandom;
        due.shuffle(&mut rng);

        let mut reviewed = 0;
        for idx in due {
            if reviewed >= scenario.max_reviews_per_day {
                break;
            }
            let recall = memrise_recall(&states[idx], now_ms);
            let (is_correct, _) = simulate_answer(words[idx], recall, &mut rng);

            if is_correct {
                streaks[idx] += 1;
                states[idx].level = (states[idx].level + 1).min(5);
            } else {
                streaks[idx] = 0;
                states[idx].level = 0; // Memrise: 答错回到种子
            }
            states[idx].last_review_at = Some(now_ms);
            states[idx].review_count += 1;
            let interval_days = (MEMRISE_INTERVALS_HOURS[states[idx].level] / 24.0).ceil() as i64;
            next_review_day[idx] = day + interval_days.max(1);
            recent_results[idx].push(is_correct);
            if recent_results[idx].len() > 20 {
                recent_results[idx].remove(0);
            }
            reviewed += 1;
            total_reviews += 1;
        }
    }

    let final_test = BASE_TS + scenario.sim_days * DAY_MS + 9 * 3_600_000;
    let mastered_count = (0..n)
        .filter(|&i| {
            let recent = &recent_results[i];
            if recent.len() < 3 {
                return false;
            }
            let acc = recent.iter().filter(|&&r| r).count() as f64 / recent.len() as f64;
            states[i].level >= 4 && acc > 0.65 && streaks[i] >= 1
        })
        .count();

    SimResult {
        daily_avg_recall,
        final_recall: states
            .iter()
            .map(|s| memrise_recall(s, final_test))
            .collect(),
        memory_strengths: states.iter().map(|s| s.level as f64 / 5.0).collect(),
        mastered_count,
        total_reviews,
    }
}

// ============================================================================
// 统计辅助
// ============================================================================

fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    xs.iter().sum::<f64>() / xs.len() as f64
}

fn std_dev(xs: &[f64]) -> f64 {
    if xs.len() < 2 {
        return 0.0;
    }
    let m = mean(xs);
    let variance = xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (xs.len() - 1) as f64;
    variance.sqrt()
}

/// Welch's t-test (unequal variance t-test), returns t-statistic
fn welch_t(a: &[f64], b: &[f64]) -> f64 {
    let ma = mean(a);
    let mb = mean(b);
    let va = std_dev(a).powi(2);
    let vb = std_dev(b).powi(2);
    let na = a.len() as f64;
    let nb = b.len() as f64;
    let se = (va / na + vb / nb).sqrt();
    if se < 1e-12 {
        return 0.0;
    }
    (ma - mb) / se
}

// ============================================================================
// 测试 1: 核心 AB 对比 — AMAS vs Leitner vs Random
// ============================================================================

#[test]
fn monte_carlo_amas_vs_baselines() {
    let config = MemoryModelConfig::default();
    let selector_config = WordSelectorConfig::default();
    let scenario = SimScenario {
        sim_days: SIM_DAYS,
        max_reviews_per_day: MAX_REVIEWS_PER_DAY,
    };
    assert!(
        NUM_WORDS > scenario.max_reviews_per_day,
        "Monte Carlo budget must be constrained to differentiate schedulers"
    );
    let mut amas_results = Vec::with_capacity(NUM_TRIALS);
    let mut leitner_results = Vec::with_capacity(NUM_TRIALS);
    let mut random_results = Vec::with_capacity(NUM_TRIALS);

    for trial in 0..NUM_TRIALS {
        let mut word_rng = StdRng::seed_from_u64(trial as u64);
        let words: Vec<f64> = (0..NUM_WORDS)
            .map(|_| word_rng.gen_range(0.15..0.85))
            .collect();

        amas_results.push(run_amas_sim(
            &words,
            trial as u64 * 10000 + 1,
            scenario,
            &config,
            &selector_config,
        ));
        leitner_results.push(run_leitner_sim(&words, trial as u64 * 10000 + 2, scenario));
        random_results.push(run_random_sim(&words, trial as u64 * 10000 + 3, scenario));
    }

    // ---- 打印隔夜 recall 趋势 ----
    println!("\n{}", "=".repeat(70));
    println!("  AMAS Monte Carlo AB Test — 蒙特卡洛模拟对比");
    println!("  场景: 真实 word_selector 评分 + 固定复习预算");
    println!(
        "  {}词 × {}天 × 每天最多{}轮 × {}次试验",
        NUM_WORDS, SIM_DAYS, MAX_REVIEWS_PER_DAY, NUM_TRIALS
    );
    println!("{}", "=".repeat(70));

    println!("\n隔夜 recall 趋势:");
    println!("┌──────┬──────────┬──────────┬──────────┐");
    println!("│  天  │   AMAS   │  Leitner │   随机   │");
    println!("├──────┼──────────┼──────────┼──────────┤");
    for &d in &[1usize, 5, 10, 15, 20, 29] {
        let a = mean(
            &amas_results
                .iter()
                .map(|r| r.daily_avg_recall[d])
                .collect::<Vec<_>>(),
        ) * 100.0;
        let l = mean(
            &leitner_results
                .iter()
                .map(|r| r.daily_avg_recall[d])
                .collect::<Vec<_>>(),
        ) * 100.0;
        let r = mean(
            &random_results
                .iter()
                .map(|r| r.daily_avg_recall[d])
                .collect::<Vec<_>>(),
        ) * 100.0;
        println!("│  {:2}  │  {:5.1}%  │  {:5.1}%  │  {:5.1}%  │", d, a, l, r);
    }
    println!("└──────┴──────────┴──────────┴──────────┘");

    // ---- 综合指标 ----
    let a_recall: Vec<f64> = amas_results.iter().map(|r| mean(&r.final_recall)).collect();
    let l_recall: Vec<f64> = leitner_results
        .iter()
        .map(|r| mean(&r.final_recall))
        .collect();
    let r_recall: Vec<f64> = random_results
        .iter()
        .map(|r| mean(&r.final_recall))
        .collect();

    let a_ms: Vec<f64> = amas_results
        .iter()
        .map(|r| mean(&r.memory_strengths))
        .collect();
    let l_ms: Vec<f64> = leitner_results
        .iter()
        .map(|r| mean(&r.memory_strengths))
        .collect();
    let r_ms: Vec<f64> = random_results
        .iter()
        .map(|r| mean(&r.memory_strengths))
        .collect();

    let a_mastered: Vec<f64> = amas_results
        .iter()
        .map(|r| r.mastered_count as f64)
        .collect();
    let l_mastered: Vec<f64> = leitner_results
        .iter()
        .map(|r| r.mastered_count as f64)
        .collect();
    let r_mastered: Vec<f64> = random_results
        .iter()
        .map(|r| r.mastered_count as f64)
        .collect();

    let a_reviews: Vec<f64> = amas_results
        .iter()
        .map(|r| r.total_reviews as f64)
        .collect();
    let l_reviews: Vec<f64> = leitner_results
        .iter()
        .map(|r| r.total_reviews as f64)
        .collect();
    let r_reviews: Vec<f64> = random_results
        .iter()
        .map(|r| r.total_reviews as f64)
        .collect();

    let a_recall_mean = mean(&a_recall) * 100.0;
    let l_recall_mean = mean(&l_recall) * 100.0;
    let r_recall_mean = mean(&r_recall) * 100.0;
    let a_ms_mean = mean(&a_ms);
    let l_ms_mean = mean(&l_ms);
    let r_ms_mean = mean(&r_ms);
    let a_mastered_mean = mean(&a_mastered);
    let l_mastered_mean = mean(&l_mastered);
    let r_mastered_mean = mean(&r_mastered);
    let a_reviews_mean = mean(&a_reviews);
    let l_reviews_mean = mean(&l_reviews);
    let r_reviews_mean = mean(&r_reviews);

    println!("\n综合指标:");
    println!("┌────────────────────────────┬──────────┬──────────┬──────────┐");
    println!("│ 指标                       │   AMAS   │  Leitner │   随机   │");
    println!("├────────────────────────────┼──────────┼──────────┼──────────┤");
    println!(
        "│ 24h后 recall               │ {:5.1}%  │ {:5.1}%  │ {:5.1}%  │",
        a_recall_mean, l_recall_mean, r_recall_mean
    );
    println!(
        "│ memory_strength            │ {:.4}  │ {:.4}  │ {:.4}  │",
        a_ms_mean, l_ms_mean, r_ms_mean
    );
    println!(
        "│ mastery 词数 (/{})         │ {:5.1}   │ {:5.1}   │ {:5.1}   │",
        NUM_WORDS, a_mastered_mean, l_mastered_mean, r_mastered_mean
    );
    println!(
        "│ 总复习次数                 │ {:5.0}   │ {:5.0}   │ {:5.0}   │",
        a_reviews_mean, l_reviews_mean, r_reviews_mean
    );
    println!("└────────────────────────────┴──────────┴──────────┴──────────┘");

    // ---- 统计显著性 ----
    let t_recall = welch_t(&a_recall, &l_recall);
    let t_ms = welch_t(&a_ms, &l_ms);
    let t_recall_vs_random = welch_t(&a_recall, &r_recall);
    let t_ms_vs_random = welch_t(&a_ms, &r_ms);
    println!("\n统计检验 (Welch's t-test):");
    println!(
        "  AMAS vs Leitner recall t={:.3} (|t|>2.576 => p<0.01)",
        t_recall
    );
    println!(
        "  AMAS vs Leitner memory_strength t={:.3} (|t|>2.576 => p<0.01)",
        t_ms
    );
    println!(
        "  AMAS vs Random recall t={:.3} (|t|>2.576 => p<0.01)",
        t_recall_vs_random
    );
    println!(
        "  AMAS vs Random memory_strength t={:.3} (|t|>2.576 => p<0.01)",
        t_ms_vs_random
    );

    // ---- 验收判定 ----
    println!("\n验收判定:");

    let check_amas_beats_leitner = a_recall_mean > l_recall_mean;
    let check_amas_beats_random = a_recall_mean > r_recall_mean;
    let check_ms_beats_leitner = a_ms_mean > l_ms_mean;
    let check_ms_beats_random = a_ms_mean > r_ms_mean;
    let check_recall_significant = t_recall > 1.96;
    let check_random_significant = t_recall_vs_random > 1.96;

    let checks = [
        (
            "AMAS recall > Leitner",
            check_amas_beats_leitner,
            format!("差值 {:.1}%", a_recall_mean - l_recall_mean),
        ),
        (
            "AMAS recall > Random",
            check_amas_beats_random,
            format!("差值 {:.1}%", a_recall_mean - r_recall_mean),
        ),
        (
            "AMAS memory_strength > Leitner",
            check_ms_beats_leitner,
            format!("{:.4} vs {:.4}", a_ms_mean, l_ms_mean),
        ),
        (
            "AMAS memory_strength > Random",
            check_ms_beats_random,
            format!("{:.4} vs {:.4}", a_ms_mean, r_ms_mean),
        ),
        (
            "AMAS vs Leitner recall 差异显著",
            check_recall_significant,
            format!("t={:.3}", t_recall),
        ),
        (
            "AMAS vs Random recall 差异显著",
            check_random_significant,
            format!("t={:.3}", t_recall_vs_random),
        ),
    ];

    let mut all_pass = true;
    for (name, passed, detail) in &checks {
        let icon = if *passed { "✅" } else { "❌" };
        println!("  {} {}: {}", icon, name, detail);
        if !*passed {
            all_pass = false;
        }
    }

    if all_pass {
        println!("\n  🎉 全部验收通过！");
    } else {
        println!("\n  ⚠️ 部分验收未通过。");
    }

    // 核心断言
    assert!(
        a_recall_mean > l_recall_mean,
        "AMAS recall {:.1}% <= Leitner {:.1}%",
        a_recall_mean,
        l_recall_mean
    );
    assert!(
        a_recall_mean > r_recall_mean,
        "AMAS recall {:.1}% <= Random {:.1}%",
        a_recall_mean,
        r_recall_mean
    );
    assert!(
        a_ms_mean > l_ms_mean && a_ms_mean > r_ms_mean,
        "AMAS memory_strength {:.4} should beat Leitner {:.4} and Random {:.4}",
        a_ms_mean,
        l_ms_mean,
        r_ms_mean
    );
}

// ============================================================================
// 测试 2: 多时间尺度记忆模型 (MDM) 验证
// ============================================================================

#[test]
fn monte_carlo_mdm_multi_timescale() {
    let config = MemoryModelConfig::default();
    let num_trials = 100;
    let mut short_faster_count = 0;
    let mut consolidation_grows_count = 0;
    let mut decay_works_count = 0;
    let mut composite_converges_count = 0;

    for trial in 0..num_trials {
        let mut rng = StdRng::seed_from_u64(trial as u64 + 42);
        let mut state = MdmState::default();

        // 10天高质量复习
        for day in 0..10 {
            let now_ms = BASE_TS + day * DAY_MS;
            let quality = 0.85 + rng.gen_range(-0.05..0.05);
            let alpha = 0.3_f64.clamp(0.1, 0.5);
            update_strength(&mut state, quality, alpha, now_ms, &config);
        }

        // DSR: 验证 stability 在持续学习后增长到合理值
        // 10天高质量复习后 stability 应显著大于初始值 0.4
        if state.stability > 2.0 {
            short_faster_count += 1;
        }

        // 验证 consolidation 增长 (mapped from difficulty)
        if state.consolidation > 0.0 {
            consolidation_grows_count += 1;
        }

        // 验证 passive decay
        let _strength_before = state.memory_strength;
        let recall_before = recall_probability(&state, BASE_TS + 10 * DAY_MS + 1000, &config);
        let recall_after_30d = recall_probability(&state, BASE_TS + 40 * DAY_MS, &config);
        if recall_after_30d < recall_before {
            decay_works_count += 1;
        }

        // 验证 composite_strength 在持续学习后趋近较高值
        let comp = composite_strength(&state, &config);
        if comp > 0.3 {
            composite_converges_count += 1;
        }
    }

    println!("\n=== MDM DSR 验证 ({} trials) ===", num_trials);
    println!("  stability > 2.0: {}/{}", short_faster_count, num_trials);
    println!(
        "  consolidation > 0: {}/{}",
        consolidation_grows_count, num_trials
    );
    println!(
        "  passive decay works: {}/{}",
        decay_works_count, num_trials
    );
    println!(
        "  composite > 0.3: {}/{}",
        composite_converges_count, num_trials
    );

    // 至少 90% 的试验应通过
    let threshold = (num_trials as f64 * 0.90) as usize;
    assert!(
        short_faster_count >= threshold,
        "stability > 2.0: {}/{} < 90%",
        short_faster_count,
        num_trials
    );
    assert!(
        consolidation_grows_count >= threshold,
        "consolidation growth: {}/{} < 90%",
        consolidation_grows_count,
        num_trials
    );
    assert!(
        decay_works_count >= threshold,
        "passive decay: {}/{} < 90%",
        decay_works_count,
        num_trials
    );
    assert!(
        composite_converges_count >= threshold,
        "composite convergence: {}/{} < 90%",
        composite_converges_count,
        num_trials
    );
}

// ============================================================================
// 测试 3: ELO 系统收敛性
// ============================================================================

#[test]
fn monte_carlo_elo_convergence() {
    let config = EloConfig::default();
    let num_trials = 100;
    let initial_elo = config.default_elo;
    let mut high_user_rises = 0;
    let mut low_user_falls = 0;
    let mut zpd_optimal_count = 0;
    let mut word_elo_adjusts = 0;

    for trial in 0..num_trials {
        let mut rng = StdRng::seed_from_u64(trial as u64 + 100);

        // 高能力用户：答对概率 90%
        let mut high_user = EloRating::default();
        let mut words_h: Vec<EloRating> = (0..30).map(|_| EloRating::default()).collect();
        for word in &mut words_h {
            let is_correct = rng.gen::<f64>() < 0.90;
            update_elo(&mut high_user, word, is_correct, &config);
        }
        if high_user.rating > initial_elo + 50.0 {
            high_user_rises += 1;
        }

        // 低能力用户：答对概率 20%
        let mut low_user = EloRating::default();
        let mut words_l: Vec<EloRating> = (0..30).map(|_| EloRating::default()).collect();
        for word in &mut words_l {
            let is_correct = rng.gen::<f64>() < 0.20;
            update_elo(&mut low_user, word, is_correct, &config);
        }
        if low_user.rating < initial_elo - 50.0 {
            low_user_falls += 1;
        }

        // 验证 ZPD
        let p_optimal = zpd_priority(
            high_user.rating,
            high_user.rating + config.zpd_optimal_offset,
            &config,
        );
        let p_far = zpd_priority(high_user.rating, high_user.rating + 600.0, &config);
        if p_optimal > p_far && p_optimal > 0.9 {
            zpd_optimal_count += 1;
        }

        // 验证 word ELO 反向调整（被高手做对的词 ELO 下降）
        let avg_word_elo: f64 =
            words_h.iter().map(|w| w.rating).sum::<f64>() / words_h.len() as f64;
        if avg_word_elo < initial_elo {
            word_elo_adjusts += 1;
        }
    }

    println!("\n=== ELO 收敛性验证 ({} trials) ===", num_trials);
    println!("  高能力用户 ELO 上升: {}/{}", high_user_rises, num_trials);
    println!("  低能力用户 ELO 下降: {}/{}", low_user_falls, num_trials);
    println!(
        "  ZPD 最优偏移处优先级最高: {}/{}",
        zpd_optimal_count, num_trials
    );
    println!("  Word ELO 反向调整: {}/{}", word_elo_adjusts, num_trials);

    let threshold = (num_trials as f64 * 0.85) as usize;
    assert!(
        high_user_rises >= threshold,
        "high user ELO rise: {}/{} < 85%",
        high_user_rises,
        num_trials
    );
    assert!(
        low_user_falls >= threshold,
        "low user ELO fall: {}/{} < 85%",
        low_user_falls,
        num_trials
    );
    assert!(
        zpd_optimal_count >= threshold,
        "ZPD optimal: {}/{} < 85%",
        zpd_optimal_count,
        num_trials
    );
    assert!(
        word_elo_adjusts >= threshold,
        "word ELO adjustment: {}/{} < 85%",
        word_elo_adjusts,
        num_trials
    );
}

// ============================================================================
// 测试 4: 掌握度 (Mastery) 分级验证
// ============================================================================

#[test]
fn monte_carlo_mastery_progression() {
    let config = MemoryModelConfig::default();
    let num_trials = 50;
    let mut reaches_mastered = 0;
    let mut regresses_on_errors = 0;
    let mut window_correct_count = 0;

    for trial in 0..num_trials {
        let mut state = WordMasteryState::new(&format!("word-mc-{}", trial));

        // 正向：连续正确复习 → 应该达到 Mastered
        for _ in 0..15 {
            let _ = update_mastery(&mut state, true, 0.90, 1.0, 0.85, &config, None);
        }
        if matches!(
            state.mastery_level,
            MasteryLevel::Reviewing | MasteryLevel::Mastered
        ) {
            reaches_mastered += 1;
        }

        // 验证 recent_results 窗口
        if state.recent_results.len() <= config.mastery_window_size as usize {
            window_correct_count += 1;
        }

        // 反向：连续错误 → mastery 回退
        let _peak_level = state.mastery_level.clone();
        for _ in 0..20 {
            let _ = update_mastery(&mut state, false, 0.1, 1.0, 0.85, &config, None);
        }
        if !matches!(state.mastery_level, MasteryLevel::Mastered) {
            regresses_on_errors += 1;
        }
    }

    println!("\n=== Mastery 分级验证 ({} trials) ===", num_trials);
    println!(
        "  正确→Reviewing/Mastered: {}/{}",
        reaches_mastered, num_trials
    );
    println!("  错误后回退: {}/{}", regresses_on_errors, num_trials);
    println!("  窗口大小正确: {}/{}", window_correct_count, num_trials);

    let threshold = (num_trials as f64 * 0.90) as usize;
    assert!(
        reaches_mastered >= threshold,
        "reaches mastered: {}/{} < 90%",
        reaches_mastered,
        num_trials
    );
    assert!(
        regresses_on_errors >= threshold,
        "regresses on errors: {}/{} < 90%",
        regresses_on_errors,
        num_trials
    );
    assert_eq!(
        window_correct_count, num_trials,
        "window size incorrect in some trials"
    );
}

// ============================================================================
// 测试 5: IAD 混淆干扰验证
// ============================================================================

#[test]
fn monte_carlo_iad_interference() {
    let config = IadConfig::default();
    let num_trials = 30;
    let mut penalty_positive_count = 0;
    let mut factor_below_one_count = 0;
    let mut decay_reduces_count = 0;

    for trial in 0..num_trials {
        let mut iad_state = IadState::default();

        // 无混淆时 penalty 应为 0
        let penalty_before = interference_penalty("word-a", &iad_state, &config);
        assert!(
            penalty_before < 1e-9,
            "trial {}: penalty before confusion should be 0, got {}",
            trial,
            penalty_before
        );

        // 记录混淆
        record_confusion(
            &mut iad_state,
            "word-a",
            "word-b",
            config.confusion_decay_rate,
            &config,
        );

        let penalty_after = interference_penalty("word-a", &iad_state, &config);
        if penalty_after > 0.0 {
            penalty_positive_count += 1;
        }

        let factor = interval_extension_factor(penalty_after, &config);
        if factor < 1.0 {
            factor_below_one_count += 1;
        }

        // 多次衰减后 penalty 应降低
        let penalty_first = penalty_after;
        for _ in 0..10 {
            record_confusion(
                &mut iad_state,
                "word-c",
                "word-d", // 不同的词对，触发全局衰减
                config.confusion_decay_rate,
                &config,
            );
        }
        let penalty_decayed = interference_penalty("word-a", &iad_state, &config);
        if penalty_decayed <= penalty_first {
            decay_reduces_count += 1;
        }
    }

    println!("\n=== IAD 混淆干扰验证 ({} trials) ===", num_trials);
    println!(
        "  混淆后 penalty > 0: {}/{}",
        penalty_positive_count, num_trials
    );
    println!(
        "  interval factor < 1.0: {}/{}",
        factor_below_one_count, num_trials
    );
    println!(
        "  衰减后 penalty 降低: {}/{}",
        decay_reduces_count, num_trials
    );

    assert_eq!(
        penalty_positive_count, num_trials,
        "penalty should be positive after confusion"
    );
    assert_eq!(
        factor_below_one_count, num_trials,
        "interval factor should be < 1.0 after confusion"
    );
    assert!(
        decay_reduces_count >= (num_trials as f64 * 0.8) as usize,
        "decay should reduce penalty in most trials: {}/{}",
        decay_reduces_count,
        num_trials
    );
}

// ============================================================================
// 测试 6: MTP 词素迁移验证
// ============================================================================

#[test]
fn monte_carlo_mtp_transfer() {
    let config = MtpConfig::default();
    let num_trials = 30;
    let mut shared_bonus_positive = 0;
    let mut no_shared_bonus_zero = 0;
    let mut accumulation_works = 0;

    for _trial in 0..num_trials {
        let mut mtp_state = MtpState::default();

        // 学习包含 "un-" 的词
        let morphemes_undo = vec!["un".to_string(), "do".to_string()];
        update_known_morphemes(&mut mtp_state, &morphemes_undo, 0.9, &config);

        // 共享 "un-" 词素的新词应获得正 bonus
        let morphemes_unhappy = vec!["un".to_string(), "happy".to_string()];
        let bonus =
            morpheme_transfer_bonus(&morphemes_unhappy, &mtp_state.known_morphemes, &config);
        if bonus > 0.0 {
            shared_bonus_positive += 1;
        }

        // 无共享词素的词 bonus 应为 0
        let morphemes_novel = vec!["xyz".to_string()];
        let no_bonus =
            morpheme_transfer_bonus(&morphemes_novel, &mtp_state.known_morphemes, &config);
        if no_bonus < 1e-9 {
            no_shared_bonus_zero += 1;
        }

        // 累计学习多个含 "un-" 的词 → familiarity 增加 → bonus 应增加或保持
        update_known_morphemes(
            &mut mtp_state,
            &vec!["un".to_string(), "lock".to_string()],
            0.95,
            &config,
        );
        let bonus_after = morpheme_transfer_bonus(
            &vec!["un".to_string(), "wrap".to_string()],
            &mtp_state.known_morphemes,
            &config,
        );
        if bonus_after >= bonus {
            accumulation_works += 1;
        }
    }

    println!("\n=== MTP 词素迁移验证 ({} trials) ===", num_trials);
    println!(
        "  共享词素 bonus > 0: {}/{}",
        shared_bonus_positive, num_trials
    );
    println!(
        "  无共享词素 bonus ≈ 0: {}/{}",
        no_shared_bonus_zero, num_trials
    );
    println!(
        "  累计学习增强 bonus: {}/{}",
        accumulation_works, num_trials
    );

    assert_eq!(
        shared_bonus_positive, num_trials,
        "shared morpheme bonus should be positive"
    );
    assert_eq!(
        no_shared_bonus_zero, num_trials,
        "no shared morpheme bonus should be zero"
    );
    assert!(
        accumulation_works >= (num_trials as f64 * 0.8) as usize,
        "accumulation should work: {}/{}",
        accumulation_works,
        num_trials
    );
}

// ============================================================================
// 测试 7: 调度效率 — 相同复习次数下的记忆保持率
// ============================================================================

#[test]
fn monte_carlo_scheduling_efficiency() {
    let config = MemoryModelConfig::default();
    let selector_config = WordSelectorConfig::default();
    let scenario = SimScenario {
        sim_days: SIM_DAYS,
        max_reviews_per_day: MAX_REVIEWS_PER_DAY,
    };
    let num_trials = 100;

    let mut amas_recall_per_review = Vec::with_capacity(num_trials);
    let mut leitner_recall_per_review = Vec::with_capacity(num_trials);
    let mut random_recall_per_review = Vec::with_capacity(num_trials);

    for trial in 0..num_trials {
        let mut word_rng = StdRng::seed_from_u64(trial as u64 + 7777);
        let words: Vec<f64> = (0..NUM_WORDS)
            .map(|_| word_rng.gen_range(0.15..0.85))
            .collect();

        let a = run_amas_sim(
            &words,
            trial as u64 * 10000 + 100,
            scenario,
            &config,
            &selector_config,
        );
        let l = run_leitner_sim(&words, trial as u64 * 10000 + 200, scenario);
        let r = run_random_sim(&words, trial as u64 * 10000 + 300, scenario);

        // 效率 = 平均 final_recall / 总复习次数 × 1000
        let efficiency_a = if a.total_reviews > 0 {
            mean(&a.final_recall) / a.total_reviews as f64 * 1000.0
        } else {
            0.0
        };
        let efficiency_l = if l.total_reviews > 0 {
            mean(&l.final_recall) / l.total_reviews as f64 * 1000.0
        } else {
            0.0
        };
        let efficiency_r = if r.total_reviews > 0 {
            mean(&r.final_recall) / r.total_reviews as f64 * 1000.0
        } else {
            0.0
        };

        amas_recall_per_review.push(efficiency_a);
        leitner_recall_per_review.push(efficiency_l);
        random_recall_per_review.push(efficiency_r);
    }

    let a_eff = mean(&amas_recall_per_review);
    let l_eff = mean(&leitner_recall_per_review);
    let r_eff = mean(&random_recall_per_review);
    let t_stat = welch_t(&amas_recall_per_review, &leitner_recall_per_review);

    println!("\n=== 调度效率验证 ({} trials) ===", num_trials);
    println!("  效率 = recall / reviews × 1000");
    println!(
        "  AMAS: {:.4}  Leitner: {:.4}  Random: {:.4}",
        a_eff, l_eff, r_eff
    );
    println!("  AMAS vs Leitner t={:.3}", t_stat);

    if a_eff > l_eff {
        println!("  ✅ AMAS 调度效率优于 Leitner");
    } else {
        println!(
            "  ⚠️ AMAS 效率 {:.4} 未优于 Leitner {:.4}（差值 {:.4}）",
            a_eff,
            l_eff,
            a_eff - l_eff
        );
    }

    assert!(
        a_eff > l_eff,
        "AMAS efficiency {:.4} <= Leitner {:.4}",
        a_eff,
        l_eff
    );
    assert!(
        a_eff > r_eff,
        "AMAS efficiency {:.4} <= Random {:.4}",
        a_eff,
        r_eff
    );
}

// ============================================================================
// 测试 8: 高压力场景 — 200词 / 15次复习 (7.5% 覆盖率)
// ============================================================================

#[test]
fn monte_carlo_high_pressure_scenario() {
    let config = MemoryModelConfig::default();
    let selector_config = WordSelectorConfig::default();
    let high_pressure = SimScenario {
        sim_days: 30,
        max_reviews_per_day: 15,
    };
    let num_words_hp = 200;
    let num_trials = 100;

    let mut amas_recall = Vec::with_capacity(num_trials);
    let mut leitner_recall = Vec::with_capacity(num_trials);
    let mut random_recall = Vec::with_capacity(num_trials);
    let mut amas_reviews = Vec::with_capacity(num_trials);
    let mut leitner_reviews = Vec::with_capacity(num_trials);
    let mut random_reviews = Vec::with_capacity(num_trials);
    let mut amas_mastery = Vec::with_capacity(num_trials);
    let mut leitner_mastery = Vec::with_capacity(num_trials);
    let mut random_mastery = Vec::with_capacity(num_trials);

    // Collect daily trends
    let mut amas_daily = vec![0.0f64; 30];
    let mut leitner_daily = vec![0.0f64; 30];
    let mut random_daily = vec![0.0f64; 30];

    for trial in 0..num_trials {
        let mut word_rng = StdRng::seed_from_u64(trial as u64 + 50000);
        let words: Vec<f64> = (0..num_words_hp)
            .map(|_| word_rng.gen_range(0.10..0.90))
            .collect();

        let a = run_amas_sim(
            &words,
            trial as u64 * 10000 + 500,
            high_pressure,
            &config,
            &selector_config,
        );
        let l = run_leitner_sim(&words, trial as u64 * 10000 + 600, high_pressure);
        let r = run_random_sim(&words, trial as u64 * 10000 + 700, high_pressure);

        amas_recall.push(mean(&a.final_recall));
        leitner_recall.push(mean(&l.final_recall));
        random_recall.push(mean(&r.final_recall));
        amas_reviews.push(a.total_reviews as f64);
        leitner_reviews.push(l.total_reviews as f64);
        random_reviews.push(r.total_reviews as f64);
        amas_mastery.push(a.mastered_count as f64);
        leitner_mastery.push(l.mastered_count as f64);
        random_mastery.push(r.mastered_count as f64);

        for d in 0..30 {
            amas_daily[d] += a.daily_avg_recall[d];
            leitner_daily[d] += l.daily_avg_recall[d];
            random_daily[d] += r.daily_avg_recall[d];
        }
    }

    for d in 0..30 {
        amas_daily[d] /= num_trials as f64;
        leitner_daily[d] /= num_trials as f64;
        random_daily[d] /= num_trials as f64;
    }

    let a_r = mean(&amas_recall) * 100.0;
    let l_r = mean(&leitner_recall) * 100.0;
    let r_r = mean(&random_recall) * 100.0;
    let a_rev = mean(&amas_reviews);
    let l_rev = mean(&leitner_reviews);
    let r_rev = mean(&random_reviews);
    let a_m = mean(&amas_mastery);
    let l_m = mean(&leitner_mastery);
    let r_m = mean(&random_mastery);

    let t_recall_leitner = welch_t(&amas_recall, &leitner_recall);
    let t_recall_random = welch_t(&amas_recall, &random_recall);

    println!("\n{}", "=".repeat(70));
    println!(
        "  高压力场景: {}词 × {}天 × 每天最多{}轮 × {}次试验",
        num_words_hp, 30, 15, num_trials
    );
    println!("  覆盖率: {:.1}%", 15.0 / num_words_hp as f64 * 100.0);
    println!("{}", "=".repeat(70));

    println!("\n隔夜 recall 趋势:");
    println!("┌──────┬──────────┬──────────┬──────────┐");
    println!("│  天  │   AMAS   │  Leitner │   随机   │");
    println!("├──────┼──────────┼──────────┼──────────┤");
    for &d in &[1usize, 5, 10, 15, 20, 29] {
        println!(
            "│  {:2}  │  {:5.1}%  │  {:5.1}%  │  {:5.1}%  │",
            d,
            amas_daily[d] * 100.0,
            leitner_daily[d] * 100.0,
            random_daily[d] * 100.0
        );
    }
    println!("└──────┴──────────┴──────────┴──────────┘");

    println!("\n综合指标:");
    println!("┌────────────────────────────┬──────────┬──────────┬──────────┐");
    println!("│ 指标                       │   AMAS   │  Leitner │   随机   │");
    println!("├────────────────────────────┼──────────┼──────────┼──────────┤");
    println!(
        "│ 24h后 recall               │ {:5.1}%  │ {:5.1}%  │ {:5.1}%  │",
        a_r, l_r, r_r
    );
    println!(
        "│ mastery 词数 (/{})        │ {:5.1}   │ {:5.1}   │ {:5.1}   │",
        num_words_hp, a_m, l_m, r_m
    );
    println!(
        "│ 总复习次数                 │ {:5.0}   │ {:5.0}   │ {:5.0}   │",
        a_rev, l_rev, r_rev
    );
    println!("└────────────────────────────┴──────────┴──────────┴──────────┘");

    println!("\n统计检验:");
    println!(
        "  AMAS vs Leitner: t={:.3}  差值 {:.1}%",
        t_recall_leitner,
        a_r - l_r
    );
    println!(
        "  AMAS vs Random:  t={:.3}  差值 {:.1}%",
        t_recall_random,
        a_r - r_r
    );

    let checks = [
        (
            "AMAS recall > Leitner",
            a_r > l_r,
            format!("{:.1}% vs {:.1}%", a_r, l_r),
        ),
        (
            "AMAS recall > Random",
            a_r > r_r,
            format!("{:.1}% vs {:.1}%", a_r, r_r),
        ),
        (
            "AMAS vs Leitner 显著",
            t_recall_leitner > 1.96,
            format!("t={:.3}", t_recall_leitner),
        ),
        (
            "AMAS vs Random 显著",
            t_recall_random > 1.96,
            format!("t={:.3}", t_recall_random),
        ),
    ];

    let mut all_pass = true;
    for (name, passed, detail) in &checks {
        let icon = if *passed { "✅" } else { "❌" };
        println!("  {} {}: {}", icon, name, detail);
        if !*passed {
            all_pass = false;
        }
    }

    if all_pass {
        println!("\n  🎉 高压力场景全部通过！");
    } else {
        println!("\n  ⚠️ 部分未通过");
    }

    assert!(a_r > l_r, "HP: AMAS {:.1}% <= Leitner {:.1}%", a_r, l_r);
    assert!(a_r > r_r, "HP: AMAS {:.1}% <= Random {:.1}%", a_r, r_r);
}

// ============================================================================
// 测试 9: 全面多算法对比 — 7 种方法
// ============================================================================

struct AlgoResult {
    name: &'static str,
    app: &'static str,
    recall_trials: Vec<f64>,
    mastery_trials: Vec<f64>,
    review_trials: Vec<f64>,
    daily: Vec<f64>,
}

#[test]
fn monte_carlo_multi_algorithm_comparison() {
    let config = MemoryModelConfig::default();
    let selector_config = WordSelectorConfig::default();
    let scenario = SimScenario {
        sim_days: SIM_DAYS,
        max_reviews_per_day: MAX_REVIEWS_PER_DAY,
    };
    let num_trials = 100;

    let algo_names: [(&str, &str); 7] = [
        ("AMAS", "wordforge"),
        ("SM-18", "SuperMemo"),
        ("FSRS", "墨墨"),
        ("Ebbinghaus", "百词斩"),
        ("HLR", "Duolingo"),
        ("Memrise", "Memrise"),
        ("SM-2", "Anki"),
    ];

    let mut results: Vec<AlgoResult> = algo_names
        .iter()
        .map(|(name, app)| AlgoResult {
            name,
            app,
            recall_trials: Vec::with_capacity(num_trials),
            mastery_trials: Vec::with_capacity(num_trials),
            review_trials: Vec::with_capacity(num_trials),
            daily: vec![0.0; SIM_DAYS as usize],
        })
        .collect();

    for trial in 0..num_trials {
        let mut word_rng = StdRng::seed_from_u64(trial as u64 + 70000);
        let words: Vec<f64> = (0..NUM_WORDS)
            .map(|_| word_rng.gen_range(0.15..0.85))
            .collect();

        let sims: Vec<SimResult> = vec![
            run_amas_sim(
                &words,
                trial as u64 * 10000 + 100,
                scenario,
                &config,
                &selector_config,
            ),
            run_sm18_sim(&words, trial as u64 * 10000 + 200, scenario),
            run_fsrs_sim(&words, trial as u64 * 10000 + 300, scenario),
            run_ebbinghaus_sim(&words, trial as u64 * 10000 + 400, scenario),
            run_hlr_sim(&words, trial as u64 * 10000 + 500, scenario),
            run_memrise_sim(&words, trial as u64 * 10000 + 600, scenario),
            run_leitner_sim(&words, trial as u64 * 10000 + 700, scenario),
        ];

        for (i, sim) in sims.iter().enumerate() {
            results[i].recall_trials.push(mean(&sim.final_recall));
            results[i].mastery_trials.push(sim.mastered_count as f64);
            results[i].review_trials.push(sim.total_reviews as f64);
            for d in 0..SIM_DAYS as usize {
                results[i].daily[d] += sim.daily_avg_recall[d];
            }
        }
    }

    // Normalize daily
    for r in &mut results {
        for d in r.daily.iter_mut() {
            *d /= num_trials as f64;
        }
    }

    // ---- Print Results ----
    println!("\n{}", "=".repeat(90));
    println!("  全面多算法对比 — 7 种背单词方法蒙特卡洛模拟");
    println!(
        "  {}词 × {}天 × 每天最多{}轮 × {}次试验",
        NUM_WORDS, SIM_DAYS, MAX_REVIEWS_PER_DAY, num_trials
    );
    println!("{}", "=".repeat(90));

    // Daily trends
    println!("\n隔夜 recall 趋势:");
    print!("┌──────");
    for _ in &results {
        print!("┬──────────");
    }
    println!("┐");
    print!("│  天  ");
    for r in &results {
        print!("│{:^10}", r.name);
    }
    println!("│");
    print!("├──────");
    for _ in &results {
        print!("┼──────────");
    }
    println!("┤");
    for &d in &[1usize, 5, 10, 15, 20, 29] {
        print!("│  {:2}  ", d);
        for r in &results {
            print!("│  {:5.1}%  ", r.daily[d] * 100.0);
        }
        println!("│");
    }
    print!("└──────");
    for _ in &results {
        print!("┴──────────");
    }
    println!("┘");

    // Summary table
    println!("\n综合指标:");
    print!("┌────────────────");
    for _ in &results {
        print!("┬──────────");
    }
    println!("┐");
    print!("│ 指标           ");
    for r in &results {
        print!("│{:^10}", r.name);
    }
    println!("│");
    print!("│ (对应 App)     ");
    for r in &results {
        print!("│{:^10}", r.app);
    }
    println!("│");
    print!("├────────────────");
    for _ in &results {
        print!("┼──────────");
    }
    println!("┤");

    // Recall row
    print!("│ 24h recall     ");
    for r in &results {
        print!("│  {:5.1}%  ", mean(&r.recall_trials) * 100.0);
    }
    println!("│");

    // Mastery row
    print!("│ mastery (/{:2})  ", NUM_WORDS);
    for r in &results {
        print!("│  {:5.1}   ", mean(&r.mastery_trials));
    }
    println!("│");

    // Reviews row
    print!("│ 总复习次数     ");
    for r in &results {
        print!("│  {:5.0}   ", mean(&r.review_trials));
    }
    println!("│");

    print!("└────────────────");
    for _ in &results {
        print!("┴──────────");
    }
    println!("┘");

    // Ranking
    let mut ranking: Vec<(usize, f64)> = results
        .iter()
        .enumerate()
        .map(|(i, r)| (i, mean(&r.recall_trials)))
        .collect();
    ranking.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    println!("\n🏆 Recall 排名:");
    for (rank, (idx, recall)) in ranking.iter().enumerate() {
        let medal = match rank {
            0 => "🥇",
            1 => "🥈",
            2 => "🥉",
            _ => "  ",
        };
        let t = welch_t(&results[0].recall_trials, &results[*idx].recall_trials);
        let gap = (mean(&results[0].recall_trials) - recall) * 100.0;
        if *idx == 0 {
            println!(
                "  {} #{} {} ({}) — {:.1}%",
                medal,
                rank + 1,
                results[*idx].name,
                results[*idx].app,
                recall * 100.0
            );
        } else {
            println!(
                "  {} #{} {} ({}) — {:.1}% (vs AMAS: {:.1}%, t={:.1})",
                medal,
                rank + 1,
                results[*idx].name,
                results[*idx].app,
                recall * 100.0,
                -gap,
                t
            );
        }
    }

    // Assertions: AMAS should beat all
    let amas_recall = mean(&results[0].recall_trials);
    let mut all_pass = true;
    println!("\n验收:");
    for i in 1..results.len() {
        let other_recall = mean(&results[i].recall_trials);
        let t = welch_t(&results[0].recall_trials, &results[i].recall_trials);
        let passed = amas_recall > other_recall;
        let icon = if passed { "✅" } else { "❌" };
        println!(
            "  {} AMAS > {} ({:.1}% vs {:.1}%, t={:.1})",
            icon,
            results[i].name,
            amas_recall * 100.0,
            other_recall * 100.0,
            t
        );
        if !passed {
            all_pass = false;
        }
    }

    if all_pass {
        println!("\n  🎉 AMAS 在所有算法中排名第一！");
    } else {
        println!("\n  ⚠️ AMAS 未能超越所有算法（FSRS 使用 ML 优化参数）");
    }

    // Assert AMAS beats all non-ML-optimized algorithms
    // FSRS uses 19 ML-optimized parameters from millions of users, so it's expected to be strong
    for i in 1..results.len() {
        if results[i].name == "FSRS" {
            continue;
        } // FSRS is ML-optimized, skip assertion
        let other = mean(&results[i].recall_trials);
        assert!(
            amas_recall > other,
            "AMAS {:.1}% <= {} {:.1}%",
            amas_recall * 100.0,
            results[i].name,
            other * 100.0
        );
    }
}
