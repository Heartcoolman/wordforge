use serde::{Deserialize, Serialize};

use crate::amas::config::MemoryModelConfig;
use crate::amas::types::*;

use super::mdm::MdmState;
use super::ssp::SspPolicy;

/// ③ 跨题型多痕迹（Phase 2）：已知出题模式集合。仅这些 mode 在 multi_trace 开启时分痕迹；
/// NULL / 未知 mode / flag 关 → 回落单一 `mastery:{word}` 键（bit-exact legacy）。
pub const KNOWN_QUESTION_MODES: &[&str] = &[
    "word-to-meaning",
    "meaning-to-word",
    "audio-to-meaning",
    "meaning-to-spelling",
];

/// 派生 mastery 状态键。`multi_trace` 开启且 `question_mode ∈ KNOWN_QUESTION_MODES` 时返回
/// `mastery:{word}:{mode}`；否则 `mastery:{word}`（legacy，bit-exact）。
/// **写路径（engine::update_memory）与快照/回滚路径（records 路由）必须共用本函数**，
/// 避免键分叉导致回滚错位/状态泄漏。
pub fn mastery_state_key(word_id: &str, question_mode: Option<&str>, multi_trace: bool) -> String {
    match question_mode {
        Some(m) if multi_trace && KNOWN_QUESTION_MODES.contains(&m) => {
            format!("mastery:{word_id}:{m}")
        }
        _ => format!("mastery:{word_id}"),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordMasteryState {
    pub word_id: String,
    pub mdm: MdmState,
    pub mastery_level: MasteryLevel,
    pub correct_streak: u32,
    pub total_attempts: u32,
    pub total_correct: u32,
    #[serde(default)]
    pub recent_results: Vec<bool>,
}

impl WordMasteryState {
    pub fn new(word_id: &str) -> Self {
        Self {
            word_id: word_id.to_string(),
            mdm: MdmState::default(),
            mastery_level: MasteryLevel::New,
            correct_streak: 0,
            total_attempts: 0,
            total_correct: 0,
            recent_results: Vec::new(),
        }
    }
}

pub fn update_mastery(
    state: &mut WordMasteryState,
    is_correct: bool,
    quality: f64,
    interval_scale: f64,
    desired_retention: f64,
    config: &MemoryModelConfig,
    ssp_policy: Option<&SspPolicy>,
) -> WordMasteryDecision {
    update_mastery_at(
        state,
        is_correct,
        quality,
        interval_scale,
        desired_retention,
        chrono::Utc::now().timestamp_millis(),
        config,
        ssp_policy,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn update_mastery_at(
    state: &mut WordMasteryState,
    is_correct: bool,
    quality: f64,
    interval_scale: f64,
    desired_retention: f64,
    now: i64,
    config: &MemoryModelConfig,
    ssp_policy: Option<&SspPolicy>,
    cold_start: Option<&super::mdm::ColdStartPriors>,
) -> WordMasteryDecision {
    let prev_last_review = state.mdm.last_review_at;

    state.total_attempts += 1;
    if is_correct {
        state.total_correct += 1;
        let gap_ok = prev_last_review
            .map(|last| now.saturating_sub(last) >= config.streak_min_gap_ms)
            .unwrap_or(true);
        if gap_ok {
            state.correct_streak += 1;
        }
    } else {
        state.correct_streak = 0;
    }

    let base_alpha =
        (interval_scale * config.alpha_scale).clamp(config.alpha_min, config.alpha_max);
    let streak_bonus = 1.0 + (state.correct_streak.min(5) as f64) * 0.1;
    let alpha = (base_alpha * streak_bonus).clamp(config.alpha_min, config.alpha_max);

    // 双腿信任调度证据（advance-before-update）：streak=本次记账后的 correct_streak；
    // lapses=total_attempts-total_correct（total_attempts 已自增，失败时含本次）
    let lapse_count = state.total_attempts - state.total_correct;
    super::mdm::update_strength_with_evidence_cs(
        &mut state.mdm,
        quality,
        alpha,
        state.correct_streak,
        lapse_count,
        now,
        config,
        cold_start,
    );

    state.recent_results.push(is_correct);
    let window = config.mastery_window_size as usize;
    if state.recent_results.len() > window {
        let drain_count = state.recent_results.len() - window;
        state.recent_results.drain(..drain_count);
    }

    state.mastery_level = determine_level(state, now, config);

    let recall = super::mdm::recall_probability(&state.mdm, now, config);
    let interval = if super::mdm::gsp_schedule_active(config) {
        // GSP 调度策略头（契约 GSP_SPEC §3）：banded retention 求解 base 整数天 → interval_scale
        // → 毕业下限 → 区间帽 → 抖动 → 取整。SSP/MDM 两后端均经 head 收口（post-SSP 施加），
        // 使策略头无论底层求解器为何都统一治理。banded retention 仅替换区间求解目标保持率口径。
        let target_recall =
            super::mdm::gsp_banded_retention(&state.mdm, config).unwrap_or(desired_retention);
        let base_days_int = if let Some(policy) = ssp_policy {
            // SSP 后端：base 取 policy 最优天（顶侧夹 90 + ceil + 底侧 1），与 banded 无关
            let optimal_days =
                policy.optimal_interval(state.mdm.stability, state.mdm.difficulty);
            (optimal_days.min(config.max_interval_days).ceil() as i64).max(1)
        } else {
            super::mdm::compute_interval_base_days(&state.mdm, target_recall, config)
        };
        let days = super::mdm::gsp_schedule_days(
            base_days_int,
            interval_scale,
            state.correct_streak,
            &state.mdm,
            config,
        );
        (days * 86400).max(config.min_interval_secs)
    } else if let Some(policy) = ssp_policy {
        let optimal_days = policy.optimal_interval(state.mdm.stability, state.mdm.difficulty);
        let secs = (optimal_days * 86400.0 * interval_scale).min(config.max_interval_days * 86400.0)
            as i64;
        secs.max(config.min_interval_secs)
    } else {
        super::mdm::compute_interval(&state.mdm, desired_retention, interval_scale, config)
    };

    WordMasteryDecision {
        word_id: state.word_id.clone(),
        memory_strength: state.mdm.memory_strength,
        recall_probability: recall,
        next_review_interval_secs: interval,
        mastery_level: state.mastery_level.clone(),
    }
}

fn determine_level(
    state: &WordMasteryState,
    now_ms: i64,
    config: &MemoryModelConfig,
) -> MasteryLevel {
    let accuracy = if state.recent_results.is_empty() {
        0.0
    } else {
        let correct = state.recent_results.iter().filter(|&&r| r).count();
        correct as f64 / state.recent_results.len() as f64
    };
    let composite = super::mdm::composite_strength(&state.mdm, config);

    if state.total_attempts == 0 {
        MasteryLevel::New
    } else if composite > config.mastery_composite_threshold
        && accuracy > config.mastery_accuracy_threshold
        && state.correct_streak >= config.mastery_streak_threshold
    {
        MasteryLevel::Mastered
    } else {
        let recall = super::mdm::recall_probability(&state.mdm, now_ms, config);
        if recall < config.forgetting_threshold {
            MasteryLevel::Forgotten
        } else if composite > config.reviewing_threshold {
            MasteryLevel::Reviewing
        } else {
            MasteryLevel::Learning
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rewind_last_review(state: &mut WordMasteryState, delta_ms: i64) {
        state.mdm.last_review_at = Some(chrono::Utc::now().timestamp_millis() - delta_ms);
    }

    fn reviewed_state(config: &MemoryModelConfig) -> WordMasteryState {
        let mut state = WordMasteryState::new("w1");
        let _ = update_mastery(&mut state, true, 0.95, 1.0, 0.9, config, None);
        rewind_last_review(&mut state, 3_600_000);
        state
    }

    #[test]
    fn mastery_state_key_derivation() {
        // flag 关 → legacy（无论 mode）
        assert_eq!(
            super::mastery_state_key("w1", Some("word-to-meaning"), false),
            "mastery:w1"
        );
        // flag 开 + 已知 mode → 带 suffix
        assert_eq!(
            super::mastery_state_key("w1", Some("word-to-meaning"), true),
            "mastery:w1:word-to-meaning"
        );
        // flag 开 + 未知 mode → legacy
        assert_eq!(super::mastery_state_key("w1", Some("bogus"), true), "mastery:w1");
        // None → legacy
        assert_eq!(super::mastery_state_key("w1", None, true), "mastery:w1");
    }

    #[test]
    fn level_up_after_correct_streak() {
        let config = MemoryModelConfig::default();
        let mut state = WordMasteryState::new("w1");
        for _ in 0..5 {
            let _ = update_mastery(&mut state, true, 0.95, 1.0, 0.9, &config, None);
            rewind_last_review(&mut state, config.streak_min_gap_ms + 1);
        }
        assert!(matches!(
            state.mastery_level,
            MasteryLevel::Reviewing | MasteryLevel::Mastered
        ));
    }

    #[test]
    fn incorrect_answer_preserves_quality_signal() {
        let config = MemoryModelConfig::default();
        let base = reviewed_state(&config);
        let mut hard_lapse = base.clone();
        let mut again_lapse = base;

        let hard_before = hard_lapse.mdm.stability;
        let again_before = again_lapse.mdm.stability;

        let _ = update_mastery(&mut hard_lapse, false, 0.2, 1.0, 0.9, &config, None);
        let _ = update_mastery(&mut again_lapse, false, 0.02, 1.0, 0.9, &config, None);

        assert!(hard_lapse.mdm.stability < hard_before);
        assert!(again_lapse.mdm.stability < again_before);
        assert!(hard_lapse.mdm.stability > again_lapse.mdm.stability);
        assert!(hard_lapse.mdm.difficulty < again_lapse.mdm.difficulty);
    }

    #[test]
    fn incorrect_and_correct_answers_move_stability_in_opposite_directions() {
        let config = MemoryModelConfig::default();
        let mut base = reviewed_state(&config);
        // 隔 2 天走长期路径：FSRS-6 同日分支对已稳卡片 Good 复习是 S 持平（饱和+G≥3 下限），
        // 本测试意图是长期方向性，需避开同日分支。
        rewind_last_review(&mut base, 2 * 86_400_000);
        let mut correct_state = base.clone();
        let mut incorrect_state = base;

        let correct_before = correct_state.mdm.stability;
        let incorrect_before = incorrect_state.mdm.stability;

        let _ = update_mastery(&mut correct_state, true, 0.8, 1.0, 0.9, &config, None);
        let _ = update_mastery(&mut incorrect_state, false, 0.02, 1.0, 0.9, &config, None);

        assert!(correct_state.mdm.stability > correct_before);
        assert!(incorrect_state.mdm.stability < incorrect_before);
    }

    #[test]
    fn streak_requires_time_gap() {
        let config = MemoryModelConfig::default();
        let mut state = WordMasteryState::new("w1");

        let _ = update_mastery(&mut state, true, 0.95, 1.0, 0.9, &config, None);
        rewind_last_review(&mut state, 60_000);
        let _ = update_mastery(&mut state, true, 0.95, 1.0, 0.9, &config, None);

        assert_eq!(state.correct_streak, 1);
    }

    #[test]
    fn streak_accumulates_with_gap() {
        let config = MemoryModelConfig::default();
        let mut state = WordMasteryState::new("w1");

        let _ = update_mastery(&mut state, true, 0.95, 1.0, 0.9, &config, None);
        rewind_last_review(&mut state, 3_600_000);
        let _ = update_mastery(&mut state, true, 0.95, 1.0, 0.9, &config, None);

        assert_eq!(state.correct_streak, 2);
    }
}
