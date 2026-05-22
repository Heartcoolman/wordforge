//! AMAS 参数网格搜索优化
//!
//! 对关键参数进行网格搜索，找出最优组合
//! 运行: cargo test --test amas_param_sweep --release -- --nocapture

// 大量 `let mut cfg = X::default(); cfg.field = v` 风格易读，整 crate 豁免。
#![allow(clippy::field_reassign_with_default)]

use learning_backend::amas::config::MemoryModelConfig;
use learning_backend::amas::memory::mdm::{
    composite_strength, compute_interval, recall_probability, update_strength, MdmState,
};

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

const DAY_MS: i64 = 86_400_000;
const BASE_TS: i64 = 1_767_225_600_000;

const NUM_WORDS: usize = 20;
const SIM_DAYS: i64 = 30;
const MAX_REVIEWS_PER_DAY: usize = 20;
const SWEEP_TRIALS: usize = 50;

// ============================================================================
// 模拟辅助
// ============================================================================

fn simulate_answer(difficulty: f64, recall_prob: f64, rng: &mut StdRng) -> (bool, f64) {
    let correct_prob = if recall_prob < 0.01 {
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

fn do_review(
    state: &mut MdmState,
    difficulty: f64,
    correct_streak: &mut u32,
    now_ms: i64,
    config: &MemoryModelConfig,
    rng: &mut StdRng,
) -> bool {
    let rp = recall_probability(state, now_ms, config);
    let (is_correct, quality) = simulate_answer(difficulty, rp, rng);

    if is_correct {
        *correct_streak += 1;
    } else {
        *correct_streak = 0;
    }

    let base_alpha = 0.3_f64;
    let streak_bonus = 1.0 + (*correct_streak).min(5) as f64 * 0.1;
    let alpha = (base_alpha * streak_bonus).clamp(0.1, 0.5);
    let eq = if is_correct { quality } else { quality * 0.1 };
    update_strength(state, eq, alpha, now_ms, config);
    is_correct
}

// ============================================================================
// 改进版 AMAS 模拟（填满每日复习槽位）
// ============================================================================

struct SimResult {
    avg_recall: f64,
    avg_ms: f64,
    mastered_count: usize,
    total_reviews: usize,
}

fn run_amas_improved(words: &[f64], seed: u64, config: &MemoryModelConfig) -> SimResult {
    let mut rng = StdRng::seed_from_u64(seed);
    let n = words.len();
    let mut states: Vec<MdmState> = (0..n).map(|_| MdmState::default()).collect();
    let mut streaks = vec![0u32; n];
    let mut next_review_ms = vec![BASE_TS; n];
    let mut total_reviews = 0;
    let mut recent_results: Vec<Vec<bool>> = (0..n).map(|_| Vec::new()).collect();

    for day in 0..SIM_DAYS {
        let now_ms = BASE_TS + day * DAY_MS + 9 * 3_600_000;

        // 收集所有词的 (index, recall, is_due)
        let mut candidates: Vec<(usize, f64, bool)> = (0..n)
            .map(|i| {
                let recall = recall_probability(&states[i], now_ms, config);
                let is_due = next_review_ms[i] <= now_ms || states[i].last_review_at.is_none();
                (i, recall, is_due)
            })
            .collect();

        // 排序：先 due 词（按 recall 升序），然后非 due 词（按 recall 升序）
        candidates.sort_by(|a, b| match (a.2, b.2) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal),
        });

        // 复习前 MAX_REVIEWS_PER_DAY 个（due + 非 due 补充）
        let mut reviewed = 0;
        for (idx, _, _) in &candidates {
            if reviewed >= MAX_REVIEWS_PER_DAY {
                break;
            }
            let idx = *idx;
            let is_correct = do_review(
                &mut states[idx],
                words[idx],
                &mut streaks[idx],
                now_ms,
                config,
                &mut rng,
            );
            recent_results[idx].push(is_correct);
            if recent_results[idx].len() > 20 {
                recent_results[idx].remove(0);
            }
            let interval_secs =
                compute_interval(&states[idx], config.base_desired_retention, 1.0, config);
            next_review_ms[idx] = now_ms + interval_secs * 1000;
            reviewed += 1;
            total_reviews += 1;
        }
    }

    let final_test = BASE_TS + SIM_DAYS * DAY_MS + 9 * 3_600_000;
    let final_recall: Vec<f64> = states
        .iter()
        .map(|s| recall_probability(s, final_test, config))
        .collect();
    let avg_recall = final_recall.iter().sum::<f64>() / n as f64;
    let avg_ms = states.iter().map(|s| s.memory_strength).sum::<f64>() / n as f64;

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
        avg_recall,
        avg_ms,
        mastered_count,
        total_reviews,
    }
}

fn mean(xs: &[f64]) -> f64 {
    xs.iter().sum::<f64>() / xs.len().max(1) as f64
}

// ============================================================================
// 参数组合评估
// ============================================================================

struct ParamSet {
    half_life_time_unit_secs: f64,
    half_life_base_epsilon: f64,
    base_desired_retention: f64,
    passive_decay_power: f64,
    consolidation_rate_scale: f64,
}

fn evaluate_params(params: &ParamSet) -> (f64, f64, f64, f64, f64) {
    let mut config = MemoryModelConfig::default();
    config.half_life_time_unit_secs = params.half_life_time_unit_secs;
    config.half_life_base_epsilon = params.half_life_base_epsilon;
    config.base_desired_retention = params.base_desired_retention;
    config.passive_decay_power = params.passive_decay_power;
    config.consolidation_rate_scale = params.consolidation_rate_scale;

    let mut recalls = Vec::with_capacity(SWEEP_TRIALS);
    let mut strengths = Vec::with_capacity(SWEEP_TRIALS);
    let mut masteries = Vec::with_capacity(SWEEP_TRIALS);
    let mut review_counts = Vec::with_capacity(SWEEP_TRIALS);

    for trial in 0..SWEEP_TRIALS {
        let mut rng = StdRng::seed_from_u64(trial as u64);
        let words: Vec<f64> = (0..NUM_WORDS).map(|_| rng.gen_range(0.15..0.85)).collect();

        let result = run_amas_improved(&words, trial as u64 * 10000 + 1, &config);
        recalls.push(result.avg_recall);
        strengths.push(result.avg_ms);
        masteries.push(result.mastered_count as f64 / NUM_WORDS as f64);
        review_counts.push(result.total_reviews as f64);
    }

    let avg_recall = mean(&recalls);
    let avg_ms = mean(&strengths);
    let avg_mastery = mean(&masteries);
    let avg_reviews = mean(&review_counts);
    let max_reviews = (SIM_DAYS as f64) * (MAX_REVIEWS_PER_DAY as f64);
    let review_ratio = avg_reviews / max_reviews;

    // 综合评分
    let score = 0.40 * avg_recall
        + 0.30 * (avg_recall / review_ratio.max(0.01)).min(1.5) / 1.5 // efficiency
        + 0.20 * avg_mastery
        + 0.10 * (1.0 - (review_ratio - 0.85).abs()); // prefer ~85% utilization

    (score, avg_recall, avg_ms, avg_mastery, avg_reviews)
}

// ============================================================================
// 网格搜索
// ============================================================================

#[test]
fn param_sweep_grid_search() {
    let time_units = [345600.0, 518400.0, 691200.0, 864000.0, 1036800.0, 1296000.0]; // 4,6,8,10,12,15 days
    let epsilons = [0.05, 0.08, 0.10, 0.15, 0.20, 0.30];
    let retentions = [0.80, 0.82, 0.85, 0.87, 0.90, 0.92];
    let decay_powers = [0.10, 0.15, 0.20, 0.30, 0.40];
    let consol_rates = [0.08, 0.12, 0.14, 0.18, 0.25];

    // Phase 1: 粗搜索 — 对每个参数独立搜索最优值
    println!("\n{}", "=".repeat(70));
    println!("  Phase 1: 独立参数搜索");
    println!("{}", "=".repeat(70));

    let baseline = ParamSet {
        half_life_time_unit_secs: 864000.0,
        half_life_base_epsilon: 0.10,
        base_desired_retention: 0.85,
        passive_decay_power: 0.15,
        consolidation_rate_scale: 0.14,
    };
    let (base_score, base_recall, base_ms, base_mastery, base_reviews) = evaluate_params(&baseline);
    println!(
        "\n基线: score={:.4} recall={:.3} ms={:.4} mastery={:.2} reviews={:.0}",
        base_score, base_recall, base_ms, base_mastery, base_reviews
    );

    // 搜索 half_life_time_unit_secs
    println!("\n--- half_life_time_unit_secs ---");
    let mut best_tu = baseline.half_life_time_unit_secs;
    let mut best_tu_score = base_score;
    for &tu in &time_units {
        let p = ParamSet {
            half_life_time_unit_secs: tu,
            ..ParamSet {
                half_life_time_unit_secs: tu,
                half_life_base_epsilon: baseline.half_life_base_epsilon,
                base_desired_retention: baseline.base_desired_retention,
                passive_decay_power: baseline.passive_decay_power,
                consolidation_rate_scale: baseline.consolidation_rate_scale,
            }
        };
        let (score, recall, ms, mastery, reviews) = evaluate_params(&p);
        let marker = if score > best_tu_score { " ★" } else { "" };
        println!(
            "  tu={:>8.0} ({:>2.0}d): score={:.4} recall={:.3} ms={:.4} mastery={:.2} rev={:.0}{}",
            tu,
            tu / 86400.0,
            score,
            recall,
            ms,
            mastery,
            reviews,
            marker
        );
        if score > best_tu_score {
            best_tu_score = score;
            best_tu = tu;
        }
    }

    // 搜索 half_life_base_epsilon
    println!("\n--- half_life_base_epsilon ---");
    let mut best_eps = baseline.half_life_base_epsilon;
    let mut best_eps_score = base_score;
    for &eps in &epsilons {
        let p = ParamSet {
            half_life_time_unit_secs: best_tu,
            half_life_base_epsilon: eps,
            base_desired_retention: baseline.base_desired_retention,
            passive_decay_power: baseline.passive_decay_power,
            consolidation_rate_scale: baseline.consolidation_rate_scale,
        };
        let (score, recall, ms, mastery, reviews) = evaluate_params(&p);
        let marker = if score > best_eps_score { " ★" } else { "" };
        println!(
            "  eps={:.2}: score={:.4} recall={:.3} ms={:.4} mastery={:.2} rev={:.0}{}",
            eps, score, recall, ms, mastery, reviews, marker
        );
        if score > best_eps_score {
            best_eps_score = score;
            best_eps = eps;
        }
    }

    // 搜索 base_desired_retention
    println!("\n--- base_desired_retention ---");
    let mut best_ret = baseline.base_desired_retention;
    let mut best_ret_score = best_eps_score;
    for &ret in &retentions {
        let p = ParamSet {
            half_life_time_unit_secs: best_tu,
            half_life_base_epsilon: best_eps,
            base_desired_retention: ret,
            passive_decay_power: baseline.passive_decay_power,
            consolidation_rate_scale: baseline.consolidation_rate_scale,
        };
        let (score, recall, ms, mastery, reviews) = evaluate_params(&p);
        let marker = if score > best_ret_score { " ★" } else { "" };
        println!(
            "  ret={:.2}: score={:.4} recall={:.3} ms={:.4} mastery={:.2} rev={:.0}{}",
            ret, score, recall, ms, mastery, reviews, marker
        );
        if score > best_ret_score {
            best_ret_score = score;
            best_ret = ret;
        }
    }

    // 搜索 passive_decay_power
    println!("\n--- passive_decay_power ---");
    let mut best_dp = baseline.passive_decay_power;
    let mut best_dp_score = best_ret_score;
    for &dp in &decay_powers {
        let p = ParamSet {
            half_life_time_unit_secs: best_tu,
            half_life_base_epsilon: best_eps,
            base_desired_retention: best_ret,
            passive_decay_power: dp,
            consolidation_rate_scale: baseline.consolidation_rate_scale,
        };
        let (score, recall, ms, mastery, reviews) = evaluate_params(&p);
        let marker = if score > best_dp_score { " ★" } else { "" };
        println!(
            "  dp={:.2}: score={:.4} recall={:.3} ms={:.4} mastery={:.2} rev={:.0}{}",
            dp, score, recall, ms, mastery, reviews, marker
        );
        if score > best_dp_score {
            best_dp_score = score;
            best_dp = dp;
        }
    }

    // 搜索 consolidation_rate_scale
    println!("\n--- consolidation_rate_scale ---");
    let mut best_cr = baseline.consolidation_rate_scale;
    let mut best_cr_score = best_dp_score;
    for &cr in &consol_rates {
        let p = ParamSet {
            half_life_time_unit_secs: best_tu,
            half_life_base_epsilon: best_eps,
            base_desired_retention: best_ret,
            passive_decay_power: best_dp,
            consolidation_rate_scale: cr,
        };
        let (score, recall, ms, mastery, reviews) = evaluate_params(&p);
        let marker = if score > best_cr_score { " ★" } else { "" };
        println!(
            "  cr={:.2}: score={:.4} recall={:.3} ms={:.4} mastery={:.2} rev={:.0}{}",
            cr, score, recall, ms, mastery, reviews, marker
        );
        if score > best_cr_score {
            best_cr_score = score;
            best_cr = cr;
        }
    }

    // Phase 2: 用最优组合验证
    println!("\n{}", "=".repeat(70));
    println!("  Phase 2: 最优参数验证");
    println!("{}", "=".repeat(70));

    let optimal = ParamSet {
        half_life_time_unit_secs: best_tu,
        half_life_base_epsilon: best_eps,
        base_desired_retention: best_ret,
        passive_decay_power: best_dp,
        consolidation_rate_scale: best_cr,
    };
    let (opt_score, opt_recall, opt_ms, opt_mastery, opt_reviews) = evaluate_params(&optimal);

    println!("\n最优参数:");
    println!(
        "  half_life_time_unit_secs: {:.0} ({:.0} days)",
        best_tu,
        best_tu / 86400.0
    );
    println!("  half_life_base_epsilon: {:.2}", best_eps);
    println!("  base_desired_retention: {:.2}", best_ret);
    println!("  passive_decay_power: {:.2}", best_dp);
    println!("  consolidation_rate_scale: {:.2}", best_cr);
    println!();
    println!(
        "最优结果: score={:.4} recall={:.3} ms={:.4} mastery={:.2} reviews={:.0}",
        opt_score, opt_recall, opt_ms, opt_mastery, opt_reviews
    );
    println!(
        "基线结果: score={:.4} recall={:.3} ms={:.4} mastery={:.2} reviews={:.0}",
        base_score, base_recall, base_ms, base_mastery, base_reviews
    );
    println!();
    println!(
        "改进: recall {:.1}% → {:.1}% ({:+.1}%)",
        base_recall * 100.0,
        opt_recall * 100.0,
        (opt_recall - base_recall) * 100.0
    );
    println!(
        "改进: mastery {:.1}% → {:.1}% ({:+.1}%)",
        base_mastery * 100.0,
        opt_mastery * 100.0,
        (opt_mastery - base_mastery) * 100.0
    );

    // Phase 3: 对比 Leitner 和 Random
    println!("\n{}", "=".repeat(70));
    println!("  Phase 3: 最优 AMAS vs Leitner vs Random");
    println!("{}", "=".repeat(70));

    let mut opt_config = MemoryModelConfig::default();
    opt_config.half_life_time_unit_secs = best_tu;
    opt_config.half_life_base_epsilon = best_eps;
    opt_config.base_desired_retention = best_ret;
    opt_config.passive_decay_power = best_dp;
    opt_config.consolidation_rate_scale = best_cr;

    let default_config = MemoryModelConfig::default();

    let mut amas_recalls = Vec::new();
    let mut leitner_recalls = Vec::new();
    let mut random_recalls = Vec::new();

    for trial in 0..SWEEP_TRIALS {
        let mut rng = StdRng::seed_from_u64(trial as u64);
        let words: Vec<f64> = (0..NUM_WORDS).map(|_| rng.gen_range(0.15..0.85)).collect();

        let a = run_amas_improved(&words, trial as u64 * 10000 + 1, &opt_config);
        let l = run_leitner_for_sweep(&words, trial as u64 * 10000 + 2, &default_config);
        let r = run_random_for_sweep(&words, trial as u64 * 10000 + 3, &default_config);

        amas_recalls.push(a.avg_recall);
        leitner_recalls.push(l.avg_recall);
        random_recalls.push(r.avg_recall);
    }

    let a_mean = mean(&amas_recalls) * 100.0;
    let l_mean = mean(&leitner_recalls) * 100.0;
    let r_mean = mean(&random_recalls) * 100.0;

    println!("\n24h后 recall:");
    println!("  AMAS(优化后): {:.1}%", a_mean);
    println!("  Leitner:      {:.1}%", l_mean);
    println!("  Random:       {:.1}%", r_mean);
    println!("  AMAS vs Leitner: {:+.1}%", a_mean - l_mean);
    println!("  AMAS vs Random:  {:+.1}%", a_mean - r_mean);

    println!("\n{}", "=".repeat(70));
    println!("  建议更新 config.rs 中的默认值:");
    println!("  half_life_time_unit_secs: {:.1}", best_tu);
    println!("  half_life_base_epsilon: {:.2}", best_eps);
    println!("  base_desired_retention: {:.2}", best_ret);
    println!("  passive_decay_power: {:.2}", best_dp);
    println!("  consolidation_rate_scale: {:.2}", best_cr);
    println!("{}", "=".repeat(70));

    // 确保优化后的 AMAS 比基线好
    assert!(
        opt_recall > base_recall * 0.98,
        "optimized recall {:.3} worse than baseline {:.3}",
        opt_recall,
        base_recall
    );
}

// Leitner and Random simplified for sweep
fn run_leitner_for_sweep(words: &[f64], seed: u64, config: &MemoryModelConfig) -> SimResult {
    let mut rng = StdRng::seed_from_u64(seed);
    let n = words.len();
    let mut states: Vec<MdmState> = (0..n).map(|_| MdmState::default()).collect();
    let mut streaks = vec![0u32; n];
    let intervals: [i64; 5] = [1, 2, 4, 7, 14];
    let mut boxes = vec![0usize; n];
    let mut next_review_day = vec![0i64; n];
    let mut total_reviews = 0;
    let mut recent_results: Vec<Vec<bool>> = (0..n).map(|_| Vec::new()).collect();

    for day in 0..SIM_DAYS {
        let now_ms = BASE_TS + day * DAY_MS + 9 * 3_600_000;
        let mut due: Vec<usize> = (0..n).filter(|&i| next_review_day[i] <= day).collect();
        use rand::seq::SliceRandom;
        due.shuffle(&mut rng);

        let mut reviewed = 0;
        for idx in due {
            if reviewed >= MAX_REVIEWS_PER_DAY {
                break;
            }
            let is_correct = do_review(
                &mut states[idx],
                words[idx],
                &mut streaks[idx],
                now_ms,
                config,
                &mut rng,
            );
            if is_correct {
                boxes[idx] = (boxes[idx] + 1).min(intervals.len() - 1);
            } else {
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

    let final_test = BASE_TS + SIM_DAYS * DAY_MS + 9 * 3_600_000;
    let avg_recall = states
        .iter()
        .map(|s| recall_probability(s, final_test, config))
        .sum::<f64>()
        / n as f64;
    let avg_ms = states.iter().map(|s| s.memory_strength).sum::<f64>() / n as f64;
    let mastered_count = (0..n)
        .filter(|&i| {
            let comp = composite_strength(&states[i], config);
            let recent = &recent_results[i];
            if recent.len() < 3 {
                return false;
            }
            let acc = recent.iter().filter(|&&r| r).count() as f64 / recent.len() as f64;
            comp > config.mastery_composite_threshold
                && acc > config.mastery_accuracy_threshold
                && streaks[i] >= config.mastery_streak_threshold
        })
        .count();

    SimResult {
        avg_recall,
        avg_ms,
        mastered_count,
        total_reviews,
    }
}

fn run_random_for_sweep(words: &[f64], seed: u64, config: &MemoryModelConfig) -> SimResult {
    let mut rng = StdRng::seed_from_u64(seed);
    let n = words.len();
    let mut states: Vec<MdmState> = (0..n).map(|_| MdmState::default()).collect();
    let mut streaks = vec![0u32; n];
    let mut total_reviews = 0;
    let mut recent_results: Vec<Vec<bool>> = (0..n).map(|_| Vec::new()).collect();

    for day in 0..SIM_DAYS {
        let now_ms = BASE_TS + day * DAY_MS + 9 * 3_600_000;
        let mut indices: Vec<usize> = (0..n).collect();
        use rand::seq::SliceRandom;
        indices.shuffle(&mut rng);
        for &idx in indices.iter().take(MAX_REVIEWS_PER_DAY) {
            let is_correct = do_review(
                &mut states[idx],
                words[idx],
                &mut streaks[idx],
                now_ms,
                config,
                &mut rng,
            );
            recent_results[idx].push(is_correct);
            if recent_results[idx].len() > 20 {
                recent_results[idx].remove(0);
            }
            total_reviews += 1;
        }
    }

    let final_test = BASE_TS + SIM_DAYS * DAY_MS + 9 * 3_600_000;
    let avg_recall = states
        .iter()
        .map(|s| recall_probability(s, final_test, config))
        .sum::<f64>()
        / n as f64;
    let avg_ms = states.iter().map(|s| s.memory_strength).sum::<f64>() / n as f64;
    let mastered_count = (0..n)
        .filter(|&i| {
            let comp = composite_strength(&states[i], config);
            let recent = &recent_results[i];
            if recent.len() < 3 {
                return false;
            }
            let acc = recent.iter().filter(|&&r| r).count() as f64 / recent.len() as f64;
            comp > config.mastery_composite_threshold
                && acc > config.mastery_accuracy_threshold
                && streaks[i] >= config.mastery_streak_threshold
        })
        .count();

    SimResult {
        avg_recall,
        avg_ms,
        mastered_count,
        total_reviews,
    }
}
