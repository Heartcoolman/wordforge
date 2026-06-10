use super::*;

impl AMASConfig {
    pub fn validate(&self) -> Result<(), String> {
        // ModelingConfig 参数范围检查
        if !(0.0..=1.0).contains(&self.modeling.attention_smoothing) {
            return Err("modeling.attention_smoothing must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.modeling.fatigue_increase_rate) {
            return Err("modeling.fatigue_increase_rate must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.modeling.confidence_decay) {
            return Err("modeling.confidence_decay must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.modeling.min_confidence) {
            return Err("modeling.min_confidence must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.modeling.motivation_momentum) {
            return Err("modeling.motivation_momentum must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.modeling.fatigue_recovery_rate) {
            return Err("modeling.fatigue_recovery_rate must be in [0,1]".to_string());
        }
        if self.modeling.response_speed_max_ms <= 0.0 {
            return Err("modeling.response_speed_max_ms must be > 0".to_string());
        }
        if !(0.0..=1.0).contains(&self.modeling.fatigue_quit_increase) {
            return Err("modeling.fatigue_quit_increase must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.modeling.cognitive_profile_alpha) {
            return Err("modeling.cognitive_profile_alpha must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.modeling.trend_alpha) {
            return Err("modeling.trend_alpha must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.monitoring.sample_rate) {
            return Err("monitoring.sample_rate must be in [0,1]".to_string());
        }

        if !(0.0..=1.0).contains(&self.constraints.high_fatigue_threshold)
            || !(0.0..=1.0).contains(&self.constraints.low_attention_threshold)
            || !(-1.0..=1.0).contains(&self.constraints.low_motivation_threshold)
        {
            return Err("invalid constraint thresholds".to_string());
        }

        if self.ensemble.base_weight_heuristic <= 0.0
            || self.ensemble.base_weight_ige <= 0.0
            || self.ensemble.base_weight_swd <= 0.0
        {
            return Err("ensemble base weights must be > 0".to_string());
        }

        let base_weight_sum = self.ensemble.base_weight_heuristic
            + self.ensemble.base_weight_ige
            + self.ensemble.base_weight_swd;
        if (base_weight_sum - 1.0).abs() > 0.01 {
            return Err(format!(
                "ensemble base weights should sum to ~1.0 (got {base_weight_sum:.3})"
            ));
        }

        if !(0.0..=1.0).contains(&self.modeling.visual_fatigue_weight) {
            return Err("modeling.visual_fatigue_weight must be in [0,1]".to_string());
        }

        if self.ensemble.min_weight <= 0.0 || self.ensemble.min_weight > 1.0 {
            return Err("ensemble.min_weight must be in (0,1]".to_string());
        }

        if 3.0 * self.ensemble.min_weight > 1.0 {
            return Err("ensemble.min_weight too large: 3 * min_weight must be <= 1.0".to_string());
        }

        if self.objective_weights.retention < 0.0
            || self.objective_weights.accuracy < 0.0
            || self.objective_weights.speed < 0.0
            || self.objective_weights.fatigue < 0.0
            || self.objective_weights.frustration < 0.0
        {
            return Err("objective_weights must be >= 0".to_string());
        }

        let sum = self.objective_weights.retention
            + self.objective_weights.accuracy
            + self.objective_weights.speed
            + self.objective_weights.fatigue
            + self.objective_weights.frustration;
        if sum <= 0.0 {
            return Err("objective_weights sum must be > 0".to_string());
        }
        if (sum - 1.0).abs() > 0.05 {
            return Err(format!(
                "objective_weights should sum to ~1.0 (got {sum:.3}). Normalize or adjust weights."
            ));
        }

        // FeatureConfig
        if !(0.0..=1.0).contains(&self.feature.hint_penalty) {
            return Err("feature.hint_penalty must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.feature.quality_accuracy_weight) {
            return Err("feature.quality_accuracy_weight must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.feature.quality_speed_weight) {
            return Err("feature.quality_speed_weight must be in [0,1]".to_string());
        }
        if !(0.001..=1.0).contains(&self.feature.trust_base_learning_rate) {
            return Err("feature.trust_base_learning_rate must be in [0.001,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.feature.incorrect_quality_scale) {
            return Err("feature.incorrect_quality_scale must be in [0,1]".to_string());
        }
        // temporal_boost_min/max 作为 f64::clamp 的边界，min > max 会 panic 学习热路径。
        if !self.feature.temporal_boost_min.is_finite()
            || !self.feature.temporal_boost_max.is_finite()
            || self.feature.temporal_boost_min > self.feature.temporal_boost_max
        {
            return Err(
                "feature.temporal_boost_min must be finite and <= temporal_boost_max".to_string(),
            );
        }

        // RewardConfig
        if self.reward.speed_reward_scale < 0.0 || self.reward.speed_reward_scale > 10.0 {
            return Err("reward.speed_reward_scale must be in [0,10]".to_string());
        }
        if self.reward.fatigue_penalty_scale < 0.0 || self.reward.fatigue_penalty_scale > 10.0 {
            return Err("reward.fatigue_penalty_scale must be in [0,10]".to_string());
        }
        if self.reward.frustration_penalty_threshold > 0.0 {
            return Err("reward.frustration_penalty_threshold must be <= 0".to_string());
        }

        // ModelingConfig - engagement penalties
        if self.modeling.engagement_pause_penalty < 0.0
            || self.modeling.engagement_pause_penalty > 1.0
        {
            return Err("modeling.engagement_pause_penalty must be in [0,1]".to_string());
        }

        // EloConfig
        if self.elo.k_factor <= 0.0 {
            return Err("elo.k_factor must be > 0".to_string());
        }
        if self.elo.min_elo >= self.elo.max_elo {
            return Err("elo.min_elo must be < elo.max_elo".to_string());
        }
        if self.elo.novice_k_multiplier <= 0.0 {
            return Err("elo.novice_k_multiplier must be > 0".to_string());
        }
        if self.elo.zpd_gaussian_sigma <= 0.0 {
            return Err("elo.zpd_gaussian_sigma must be > 0".to_string());
        }
        if !self.elo.word_k_factor_ratio.is_finite() || self.elo.word_k_factor_ratio <= 0.0 {
            return Err("elo.word_k_factor_ratio must be finite and > 0".to_string());
        }

        // FatigueDecayConfig
        if self.fatigue_decay.full_reset_threshold_secs
            <= self.fatigue_decay.decay_start_threshold_secs
        {
            return Err(
                "fatigue_decay.full_reset_threshold_secs must be > decay_start_threshold_secs"
                    .to_string(),
            );
        }
        if self.fatigue_decay.decay_start_threshold_secs <= 0.0 {
            return Err("fatigue_decay.decay_start_threshold_secs must be > 0".to_string());
        }
        if self.fatigue_decay.decay_time_constant_secs <= 0.0 {
            return Err("fatigue_decay.decay_time_constant_secs must be > 0".to_string());
        }

        // HeuristicConfig
        if !(0.0..=1.0).contains(&self.heuristic.cold_start_difficulty) {
            return Err("heuristic.cold_start_difficulty must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.heuristic.cold_start_new_ratio) {
            return Err("heuristic.cold_start_new_ratio must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.heuristic.confidence_base) {
            return Err("heuristic.confidence_base must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.heuristic.confidence_min) {
            return Err("heuristic.confidence_min must be in [0,1]".to_string());
        }
        if self.heuristic.confidence_decay_scale <= 0.0 {
            return Err("heuristic.confidence_decay_scale must be > 0".to_string());
        }

        // IgeConfig
        if self.ige.ucb_confidence_coeff <= 0.0 {
            return Err("ige.ucb_confidence_coeff must be > 0".to_string());
        }
        if !(0.0..=1.0).contains(&self.ige.default_confidence) {
            return Err("ige.default_confidence must be in [0,1]".to_string());
        }

        // SwdConfig
        if self.swd.max_history_size == 0 {
            return Err("swd.max_history_size must be > 0".to_string());
        }

        // MemoryModelConfig
        if !(0.0..=1.0).contains(&self.memory_model.short_term_learning_rate) {
            return Err("memory_model.short_term_learning_rate must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.memory_model.medium_term_learning_rate) {
            return Err("memory_model.medium_term_learning_rate must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.memory_model.long_term_learning_rate) {
            return Err("memory_model.long_term_learning_rate must be in [0,1]".to_string());
        }
        let composite_sum = self.memory_model.composite_weight_short
            + self.memory_model.composite_weight_medium
            + self.memory_model.composite_weight_long;
        if (composite_sum - 1.0).abs() > 0.01 {
            return Err("memory_model composite weights must sum to ~1.0".to_string());
        }
        if self.memory_model.half_life_time_unit_secs <= 0.0 {
            return Err("memory_model.half_life_time_unit_secs must be > 0".to_string());
        }
        if self.memory_model.half_life_base_epsilon <= 0.0 {
            return Err("memory_model.half_life_base_epsilon must be > 0".to_string());
        }
        if !(0.1..=3.0).contains(&self.memory_model.half_life_power) {
            return Err("memory_model.half_life_power must be in [0.1, 3.0]".to_string());
        }
        if !(0.5..=0.99).contains(&self.memory_model.base_desired_retention) {
            return Err("memory_model.base_desired_retention must be in [0.5,0.99]".to_string());
        }
        if self.memory_model.consolidation_bonus < 0.0 {
            return Err("memory_model.consolidation_bonus must be >= 0".to_string());
        }
        if self.memory_model.passive_decay_half_life_days <= 0.0 {
            return Err("memory_model.passive_decay_half_life_days must be > 0".to_string());
        }
        if self.memory_model.passive_decay_power <= 0.0 {
            return Err("memory_model.passive_decay_power must be > 0".to_string());
        }
        if self.memory_model.mastery_window_size == 0 {
            return Err("memory_model.mastery_window_size must be > 0".to_string());
        }
        if self.memory_model.streak_min_gap_ms < 0 {
            return Err("memory_model.streak_min_gap_ms must be >= 0".to_string());
        }
        if !(0.0..=1.0).contains(&self.memory_model.alpha_min)
            || !(0.0..=1.0).contains(&self.memory_model.alpha_max)
        {
            return Err("memory_model.alpha_min/alpha_max must be in [0,1]".to_string());
        }
        if self.memory_model.alpha_min >= self.memory_model.alpha_max {
            return Err("memory_model.alpha_min must be < alpha_max".to_string());
        }
        if !(0.0..=1.0).contains(&self.memory_model.forgetting_threshold) {
            return Err("memory_model.forgetting_threshold must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.memory_model.retention_min)
            || !(0.0..=1.0).contains(&self.memory_model.retention_max)
        {
            return Err("memory_model.retention_min/retention_max must be in [0,1]".to_string());
        }
        if self.memory_model.retention_min >= self.memory_model.retention_max {
            return Err("memory_model.retention_min must be < retention_max".to_string());
        }
        if self.memory_model.max_interval_days <= 0.0 {
            return Err("memory_model.max_interval_days must be > 0".to_string());
        }
        if self.memory_model.min_interval_secs <= 0 {
            return Err("memory_model.min_interval_secs must be > 0".to_string());
        }

        // FSRS 遗忘曲线参数：被作除数 / 倒数指数 / 渐近线消费，必须有限且在定义域内。
        // factor 是严格正缩放因子（除数）；decay 是衰减幂律指数（必须 < 0 否则遗忘曲线反转）；
        // floor 是非零渐近线 R→floor，须在 [0,1)。
        if !self.memory_model.forgetting_curve_factor.is_finite()
            || self.memory_model.forgetting_curve_factor <= 0.0
        {
            return Err("memory_model.forgetting_curve_factor must be finite and > 0".to_string());
        }
        if !self.memory_model.forgetting_curve_decay.is_finite()
            || self.memory_model.forgetting_curve_decay >= 0.0
        {
            return Err(
                "memory_model.forgetting_curve_decay must be finite and < 0 (FSRS power-law exponent)"
                    .to_string(),
            );
        }
        if !self.memory_model.forgetting_curve_floor.is_finite()
            || !(0.0..1.0).contains(&self.memory_model.forgetting_curve_floor)
        {
            return Err("memory_model.forgetting_curve_floor must be in [0,1)".to_string());
        }
        // 跨字段：floor 必须严格小于 retention_min。运行期 target_recall（desired_retention）被夹到
        // [retention_min, retention_max]，若 floor >= retention_min，则 target_recall 可 <= floor，使
        // compute_interval 的 (target-floor)/(1-floor) 夹到 1e-6、产出退化(max 夹紧)间隔而非报错。
        if self.memory_model.forgetting_curve_floor >= self.memory_model.retention_min {
            return Err(
                "memory_model.forgetting_curve_floor must be < retention_min (else desired_retention can fall at/below the asymptote and degenerate intervals)"
                    .to_string(),
            );
        }
        // FSRS-6 w[] 权重：w[8]/w[11] 等经 .exp() 消费，非有限或过大会 overflow 为 Inf，
        // 进而污染 stability 并以 JSON null 落库；逐元素拒绝非有限及越界值。
        if self
            .memory_model
            .w
            .iter()
            .any(|x| !x.is_finite() || x.abs() > 100.0)
        {
            return Err(
                "memory_model.w[*] must all be finite and within [-100,100]".to_string(),
            );
        }
        // FSRS-6 结构性约束：初始 stability w[0..4] 必须为正（S0 直接查表）
        if self.memory_model.w[..4].iter().any(|&s0| s0 <= 0.0) {
            return Err("memory_model.w[0..4] (initial stability) must be > 0".to_string());
        }
        // w[19] 同日饱和指数：负值会让大 S 同日复习爆增长；官方 clipper 域 [0,1]
        if !(0.0..=1.0).contains(&self.memory_model.w[19]) {
            return Err(
                "memory_model.w[19] (same-day saturation) must be in [0,1]".to_string(),
            );
        }
        // w[20] 遗忘曲线 decay：curve_decay() 运行时钳到 [0.05,2]，配置层拒绝域外值
        if !(0.05..=2.0).contains(&self.memory_model.w[20]) {
            return Err(
                "memory_model.w[20] (forgetting curve decay) must be in [0.05,2.0]".to_string(),
            );
        }

        // EvmConfig — diversity_log_divisor 必须 > 1，否则 ln() 产生 NaN/Inf
        if self.evm.diversity_log_divisor <= 1.0 {
            return Err("evm.diversity_log_divisor must be > 1".to_string());
        }
        if self.evm.diversity_bonus_cap < 0.0 {
            return Err("evm.diversity_bonus_cap must be >= 0".to_string());
        }
        if self.evm.diversity_growth_rate <= 0.0 {
            return Err("evm.diversity_growth_rate must be > 0".to_string());
        }

        // IadConfig
        if !(0.0..=1.0).contains(&self.iad.interference_penalty_factor) {
            return Err("iad.interference_penalty_factor must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.iad.interference_penalty_cap) {
            return Err("iad.interference_penalty_cap must be in [0,1]".to_string());
        }

        // MtpConfig
        if !(0.0..=1.0).contains(&self.mtp.morpheme_transfer_coeff) {
            return Err("mtp.morpheme_transfer_coeff must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.mtp.morpheme_bonus_cap) {
            return Err("mtp.morpheme_bonus_cap must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.mtp.known_morpheme_decay) {
            return Err("mtp.known_morpheme_decay must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.mtp.new_morpheme_initial_coeff) {
            return Err("mtp.new_morpheme_initial_coeff must be in [0,1]".to_string());
        }

        // WordSelectorConfig
        if self.word_selector.review_ucb_weight < 0.0 {
            return Err("word_selector.review_ucb_weight must be >= 0".to_string());
        }
        if self.word_selector.review_ucb_max_bonus < 0.0 {
            return Err("word_selector.review_ucb_max_bonus must be >= 0".to_string());
        }
        if self.word_selector.new_word_gaussian_sigma <= 0.0 {
            return Err("word_selector.new_word_gaussian_sigma must be > 0".to_string());
        }
        // spacing_cooldown_secs / optimal_recall_sigma / sigmoid_steepness 均作除数，=0 注入 NaN 进评分。
        if !self.word_selector.spacing_cooldown_secs.is_finite()
            || self.word_selector.spacing_cooldown_secs <= 0.0
        {
            return Err("word_selector.spacing_cooldown_secs must be finite and > 0".to_string());
        }
        if !self.word_selector.optimal_recall_sigma.is_finite()
            || self.word_selector.optimal_recall_sigma <= 0.0
        {
            return Err("word_selector.optimal_recall_sigma must be finite and > 0".to_string());
        }
        if !self.word_selector.sigmoid_steepness.is_finite()
            || self.word_selector.sigmoid_steepness <= 0.0
        {
            return Err("word_selector.sigmoid_steepness must be finite and > 0".to_string());
        }
        if !(0.0..=1.0).contains(&self.word_selector.recall_mastered_threshold) {
            return Err("word_selector.recall_mastered_threshold must be in [0,1]".to_string());
        }

        // InterventionConfig
        if !(0.0..=1.0).contains(&self.intervention.fatigue_alert_threshold) {
            return Err("intervention.fatigue_alert_threshold must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.intervention.attention_alert_threshold) {
            return Err("intervention.attention_alert_threshold must be in [0,1]".to_string());
        }

        // LearningStrategyConfig
        if !(0.0..=1.0).contains(&self.learning_strategy.cross_session_high_accuracy) {
            return Err(
                "learning_strategy.cross_session_high_accuracy must be in [0,1]".to_string(),
            );
        }
        if !(0.0..=1.0).contains(&self.learning_strategy.sprint_mastery_ratio) {
            return Err("learning_strategy.sprint_mastery_ratio must be in [0,1]".to_string());
        }
        if !(0.0..=1.0).contains(&self.learning_strategy.fatigue_reduction_threshold) {
            return Err(
                "learning_strategy.fatigue_reduction_threshold must be in [0,1]".to_string(),
            );
        }

        // SspConfig
        if self.ssp.recall_cost <= 0.0 {
            return Err("ssp.recall_cost must be > 0".to_string());
        }
        if self.ssp.forget_cost <= 0.0 {
            return Err("ssp.forget_cost must be > 0".to_string());
        }
        if self.ssp.target_stability_days <= 0.0 {
            return Err("ssp.target_stability_days must be > 0".to_string());
        }
        if self.ssp.base <= 1.0 {
            return Err("ssp.base must be > 1.0".to_string());
        }
        if self.ssp.min_index >= self.ssp.max_index {
            return Err("ssp.min_index must be < ssp.max_index".to_string());
        }
        if self.ssp.max_index > 200 {
            return Err("ssp.max_index must be <= 200".to_string());
        }
        if !(0.0..=1.0).contains(&self.ssp.discount_factor) {
            return Err("ssp.discount_factor must be in [0,1]".to_string());
        }
        if !(0.0..1.0).contains(&self.ssp.r_min)
            || !(0.0..=1.0).contains(&self.ssp.r_max)
            || self.ssp.r_min >= self.ssp.r_max
        {
            return Err("ssp.r_min/r_max must be in [0,1] with r_min < r_max".to_string());
        }
        if !(0.0001..=0.5).contains(&self.ssp.r_step) {
            return Err("ssp.r_step must be in [0.0001,0.5]".to_string());
        }
        if self.ssp.dual_grid_enabled && self.ssp.linear_step_days <= 0.0 {
            return Err("ssp.linear_step_days must be > 0".to_string());
        }
        if self.ssp.dual_grid_threshold_days <= 0.0 {
            return Err("ssp.dual_grid_threshold_days must be > 0".to_string());
        }
        // DP 求解器边界：threshold<=0 使早停永不触发，配合超大 max_iterations 让同步 precompute 长时间卡死调用线程。
        if !self.ssp.convergence_threshold.is_finite() || self.ssp.convergence_threshold <= 0.0 {
            return Err("ssp.convergence_threshold must be finite and > 0".to_string());
        }
        if self.ssp.max_iterations == 0 || self.ssp.max_iterations > 1_000_000 {
            return Err("ssp.max_iterations must be in [1, 1_000_000]".to_string());
        }

        Ok(())
    }
}
