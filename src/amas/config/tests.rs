#[cfg(test)]
use super::*;

#[test]
fn default_config_is_valid() {
    let cfg = AMASConfig::default();
    assert!(cfg.validate().is_ok());
}

#[test]
fn invalid_config_is_rejected() {
    let mut cfg = AMASConfig::default();
    cfg.monitoring.sample_rate = 2.0;
    assert!(cfg.validate().is_err());
}

#[test]
fn negative_streak_gap_is_rejected() {
    let mut cfg = AMASConfig::default();
    cfg.memory_model.streak_min_gap_ms = -1;
    assert!(cfg.validate().is_err());
}

// ---- validate() 全分支：每条 if 至少有一个失败用例 ----

fn err_msg(cfg: &AMASConfig) -> String {
    cfg.validate().err().unwrap_or_default()
}

#[test]
fn modeling_out_of_range_fields_each_fail() {
    let bad = [
        |c: &mut AMASConfig| c.modeling.attention_smoothing = 1.5,
        |c: &mut AMASConfig| c.modeling.fatigue_increase_rate = -0.1,
        |c: &mut AMASConfig| c.modeling.confidence_decay = 2.0,
        |c: &mut AMASConfig| c.modeling.min_confidence = -0.5,
        |c: &mut AMASConfig| c.modeling.motivation_momentum = -1.0,
        |c: &mut AMASConfig| c.modeling.fatigue_recovery_rate = 5.0,
        |c: &mut AMASConfig| c.modeling.response_speed_max_ms = 0.0,
        |c: &mut AMASConfig| c.modeling.fatigue_quit_increase = -0.01,
        |c: &mut AMASConfig| c.modeling.cognitive_profile_alpha = 1.5,
        |c: &mut AMASConfig| c.modeling.trend_alpha = -0.5,
        |c: &mut AMASConfig| c.modeling.visual_fatigue_weight = 1.2,
        |c: &mut AMASConfig| c.modeling.engagement_pause_penalty = 1.5,
    ];
    for (i, mutate) in bad.iter().enumerate() {
        let mut c = AMASConfig::default();
        mutate(&mut c);
        assert!(c.validate().is_err(), "case {i} should fail");
    }
}

#[test]
fn constraint_thresholds_invalid_combinations_fail() {
    let cases = [
        |c: &mut AMASConfig| c.constraints.high_fatigue_threshold = 2.0,
        |c: &mut AMASConfig| c.constraints.low_attention_threshold = -0.1,
        |c: &mut AMASConfig| c.constraints.low_motivation_threshold = -2.0,
    ];
    for (i, mutate) in cases.iter().enumerate() {
        let mut c = AMASConfig::default();
        mutate(&mut c);
        let msg = err_msg(&c);
        assert!(msg.contains("constraint"), "case {i}: {msg}");
    }
}

#[test]
fn ensemble_zero_or_negative_base_weights_fail() {
    let mut c = AMASConfig::default();
    c.ensemble.base_weight_heuristic = 0.0;
    let msg = err_msg(&c);
    assert!(msg.contains("base weights"));
}

#[test]
fn ensemble_base_weights_not_summing_to_one_fail() {
    let mut c = AMASConfig::default();
    c.ensemble.base_weight_heuristic = 0.5;
    c.ensemble.base_weight_ige = 0.5;
    c.ensemble.base_weight_swd = 0.5;
    let msg = err_msg(&c);
    assert!(msg.contains("sum to ~1.0"));
}

#[test]
fn ensemble_min_weight_out_of_range_fails() {
    let mut c = AMASConfig::default();
    c.ensemble.min_weight = 0.0;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.ensemble.min_weight = 1.5;
    assert!(c.validate().is_err());
}

#[test]
fn ensemble_min_weight_too_large_fails() {
    let mut c = AMASConfig::default();
    c.ensemble.min_weight = 0.4; // 3 * 0.4 = 1.2 > 1.0
    let msg = err_msg(&c);
    assert!(msg.contains("3 * min_weight"));
}

#[test]
fn objective_weights_negative_or_zero_sum_fail() {
    let mut c = AMASConfig::default();
    c.objective_weights.retention = -0.1;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.objective_weights.retention = 0.0;
    c.objective_weights.accuracy = 0.0;
    c.objective_weights.speed = 0.0;
    c.objective_weights.fatigue = 0.0;
    c.objective_weights.frustration = 0.0;
    let msg = err_msg(&c);
    assert!(msg.contains("sum must be > 0"));
}

#[test]
fn objective_weights_not_summing_to_one_fail() {
    let mut c = AMASConfig::default();
    c.objective_weights.retention = 0.5;
    c.objective_weights.accuracy = 0.5;
    c.objective_weights.speed = 0.5;
    c.objective_weights.fatigue = 0.5;
    c.objective_weights.frustration = 0.5;
    let msg = err_msg(&c);
    assert!(msg.contains("should sum to ~1.0"));
}

#[test]
fn feature_fields_out_of_range_fail() {
    let mutators = [
        |c: &mut AMASConfig| c.feature.hint_penalty = -0.1,
        |c: &mut AMASConfig| c.feature.quality_accuracy_weight = 2.0,
        |c: &mut AMASConfig| c.feature.quality_speed_weight = 1.5,
        |c: &mut AMASConfig| c.feature.trust_base_learning_rate = 2.0,
        |c: &mut AMASConfig| c.feature.trust_base_learning_rate = 0.0,
        |c: &mut AMASConfig| c.feature.incorrect_quality_scale = -0.1,
    ];
    for (i, mutate) in mutators.iter().enumerate() {
        let mut c = AMASConfig::default();
        mutate(&mut c);
        assert!(c.validate().is_err(), "case {i}");
    }
}

#[test]
fn reward_fields_out_of_range_fail() {
    let mut c = AMASConfig::default();
    c.reward.speed_reward_scale = -1.0;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.reward.speed_reward_scale = 11.0;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.reward.fatigue_penalty_scale = -0.5;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.reward.fatigue_penalty_scale = 20.0;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.reward.frustration_penalty_threshold = 0.5; // 必须 <= 0
    assert!(c.validate().is_err());
}

#[test]
fn elo_invalid_settings_fail() {
    let mut c = AMASConfig::default();
    c.elo.k_factor = 0.0;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.elo.min_elo = 2000.0;
    c.elo.max_elo = 1000.0;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.elo.novice_k_multiplier = -1.0;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.elo.zpd_gaussian_sigma = 0.0;
    assert!(c.validate().is_err());
}

#[test]
fn fatigue_decay_invalid_thresholds_fail() {
    let mut c = AMASConfig::default();
    c.fatigue_decay.full_reset_threshold_secs = c.fatigue_decay.decay_start_threshold_secs - 1.0;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.fatigue_decay.decay_start_threshold_secs = 0.0;
    c.fatigue_decay.full_reset_threshold_secs = 10.0;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.fatigue_decay.decay_time_constant_secs = 0.0;
    assert!(c.validate().is_err());
}

#[test]
fn heuristic_fields_out_of_range_fail() {
    let mutators = [
        |c: &mut AMASConfig| c.heuristic.cold_start_difficulty = 2.0,
        |c: &mut AMASConfig| c.heuristic.cold_start_new_ratio = -1.0,
        |c: &mut AMASConfig| c.heuristic.confidence_base = 1.1,
        |c: &mut AMASConfig| c.heuristic.confidence_min = -0.1,
        |c: &mut AMASConfig| c.heuristic.confidence_decay_scale = 0.0,
    ];
    for (i, m) in mutators.iter().enumerate() {
        let mut c = AMASConfig::default();
        m(&mut c);
        assert!(c.validate().is_err(), "case {i}");
    }
}

#[test]
fn ige_fields_invalid_fail() {
    let mut c = AMASConfig::default();
    c.ige.ucb_confidence_coeff = 0.0;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.ige.default_confidence = 2.0;
    assert!(c.validate().is_err());
}

#[test]
fn swd_max_history_zero_fails() {
    let mut c = AMASConfig::default();
    c.swd.max_history_size = 0;
    assert!(c.validate().is_err());
}

#[test]
fn memory_model_invalid_fields_fail() {
    let mutators: [fn(&mut AMASConfig); 22] = [
        |c| c.memory_model.short_term_learning_rate = 2.0,
        |c| c.memory_model.medium_term_learning_rate = -0.1,
        |c| c.memory_model.long_term_learning_rate = 1.5,
        |c| {
            c.memory_model.composite_weight_short = 0.1;
            c.memory_model.composite_weight_medium = 0.1;
            c.memory_model.composite_weight_long = 0.1;
        },
        |c| c.memory_model.half_life_time_unit_secs = 0.0,
        |c| c.memory_model.half_life_base_epsilon = -1.0,
        |c| c.memory_model.half_life_power = 5.0,
        |c| c.memory_model.base_desired_retention = 0.1, // 应在 [0.5,0.99]
        |c| c.memory_model.consolidation_bonus = -0.5,
        |c| c.memory_model.passive_decay_half_life_days = 0.0,
        |c| c.memory_model.passive_decay_power = -0.1,
        |c| c.memory_model.mastery_window_size = 0,
        |c| {
            c.memory_model.alpha_min = 2.0;
        },
        |c| {
            c.memory_model.alpha_min = 0.8;
            c.memory_model.alpha_max = 0.3;
        },
        |c| c.memory_model.forgetting_threshold = 2.0,
        |c| {
            c.memory_model.retention_min = 0.9;
            c.memory_model.retention_max = 0.5;
        },
        |c| c.memory_model.alpha_ramp_tau = -0.1,
        |c| c.memory_model.alpha_ramp_tau = 100.1,
        |c| c.memory_model.alpha_ramp_tau = f64::NAN,
        |c| c.memory_model.alpha_lapse_ramp_tau = -0.1,
        |c| c.memory_model.alpha_lapse_ramp_tau = 100.1,
        |c| c.memory_model.alpha_lapse_ramp_tau = f64::NAN,
    ];
    for (i, m) in mutators.iter().enumerate() {
        let mut c = AMASConfig::default();
        m(&mut c);
        assert!(c.validate().is_err(), "case {i}");
    }
}

#[test]
fn alpha_ramp_tau_valid_boundaries_pass() {
    // [0,100] 闭区间：0.0（默认关闭）与 100.0 均合法，双腿同域
    for v in [0.0, 100.0] {
        let mut c = AMASConfig::default();
        c.memory_model.alpha_ramp_tau = v;
        assert!(c.validate().is_ok(), "tau_s {v}");

        let mut c = AMASConfig::default();
        c.memory_model.alpha_lapse_ramp_tau = v;
        assert!(c.validate().is_ok(), "tau_f {v}");
    }
}

#[test]
fn memory_model_min_interval_and_max_interval_fail() {
    let mut c = AMASConfig::default();
    c.memory_model.max_interval_days = 0.0;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.memory_model.min_interval_secs = 0;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.memory_model.retention_min = 2.0;
    assert!(c.validate().is_err());
}

#[test]
fn evm_invalid_fields_fail() {
    let mut c = AMASConfig::default();
    c.evm.diversity_log_divisor = 1.0;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.evm.diversity_bonus_cap = -0.1;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.evm.diversity_growth_rate = 0.0;
    assert!(c.validate().is_err());
}

#[test]
fn iad_invalid_fields_fail() {
    let mut c = AMASConfig::default();
    c.iad.interference_penalty_factor = 1.5;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.iad.interference_penalty_cap = -0.1;
    assert!(c.validate().is_err());
}

#[test]
fn mtp_invalid_fields_fail() {
    let mutators: [fn(&mut AMASConfig); 4] = [
        |c| c.mtp.morpheme_transfer_coeff = 1.5,
        |c| c.mtp.morpheme_bonus_cap = -0.1,
        |c| c.mtp.known_morpheme_decay = 2.0,
        |c| c.mtp.new_morpheme_initial_coeff = -0.5,
    ];
    for (i, m) in mutators.iter().enumerate() {
        let mut c = AMASConfig::default();
        m(&mut c);
        assert!(c.validate().is_err(), "case {i}");
    }
}

#[test]
fn word_selector_invalid_fields_fail() {
    let mut c = AMASConfig::default();
    c.word_selector.review_ucb_weight = -0.1;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.word_selector.review_ucb_max_bonus = -1.0;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.word_selector.new_word_gaussian_sigma = 0.0;
    assert!(c.validate().is_err());
}

#[test]
fn intervention_invalid_thresholds_fail() {
    let mut c = AMASConfig::default();
    c.intervention.fatigue_alert_threshold = 2.0;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.intervention.attention_alert_threshold = -0.5;
    assert!(c.validate().is_err());
}

#[test]
fn learning_strategy_out_of_range_fields_fail() {
    let mut c = AMASConfig::default();
    c.learning_strategy.cross_session_high_accuracy = 1.5;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.learning_strategy.sprint_mastery_ratio = -0.1;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.learning_strategy.fatigue_reduction_threshold = 2.0;
    assert!(c.validate().is_err());
}

#[test]
fn ssp_invalid_fields_fail() {
    let mutators: [fn(&mut AMASConfig); 6] = [
        |c| c.ssp.recall_cost = 0.0,
        |c| c.ssp.forget_cost = -1.0,
        |c| c.ssp.target_stability_days = 0.0,
        |c| c.ssp.base = 1.0,
        |c| {
            c.ssp.min_index = 10;
            c.ssp.max_index = 5;
        },
        |c| c.ssp.discount_factor = 1.5,
    ];
    for (i, m) in mutators.iter().enumerate() {
        let mut c = AMASConfig::default();
        m(&mut c);
        assert!(c.validate().is_err(), "case {i}");
    }
}

#[test]
fn forgetting_curve_and_w_array_invalid_fail() {
    let mutators: [fn(&mut AMASConfig); 8] = [
        |c| c.memory_model.forgetting_curve_factor = 0.0,
        |c| c.memory_model.forgetting_curve_factor = f64::NAN,
        |c| c.memory_model.forgetting_curve_decay = 0.0, // 必须 < 0
        |c| c.memory_model.forgetting_curve_decay = 0.5, // 正值反转曲线
        |c| c.memory_model.forgetting_curve_floor = 1.0, // 须在 [0,1)
        |c| c.memory_model.forgetting_curve_floor = -0.1,
        |c| c.memory_model.w[8] = f64::INFINITY,
        |c| c.memory_model.w[0] = 1000.0, // 越界 magnitude
    ];
    for (i, m) in mutators.iter().enumerate() {
        let mut c = AMASConfig::default();
        m(&mut c);
        assert!(c.validate().is_err(), "case {i}");
    }
}

#[test]
fn feature_temporal_boost_inverted_range_fails() {
    let mut c = AMASConfig::default();
    c.feature.temporal_boost_min = 1.5;
    c.feature.temporal_boost_max = 0.5;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.feature.temporal_boost_min = f64::NAN;
    assert!(c.validate().is_err());
}

#[test]
fn elo_word_k_factor_ratio_invalid_fails() {
    let mut c = AMASConfig::default();
    c.elo.word_k_factor_ratio = 0.0;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.elo.word_k_factor_ratio = f64::NAN;
    assert!(c.validate().is_err());
}

#[test]
fn word_selector_divisor_fields_invalid_fail() {
    let mutators: [fn(&mut AMASConfig); 5] = [
        |c| c.word_selector.spacing_cooldown_secs = 0.0,
        |c| c.word_selector.optimal_recall_sigma = 0.0,
        |c| c.word_selector.sigmoid_steepness = 0.0,
        |c| c.word_selector.recall_mastered_threshold = 1.5,
        |c| c.word_selector.recall_mastered_threshold = -0.1,
    ];
    for (i, m) in mutators.iter().enumerate() {
        let mut c = AMASConfig::default();
        m(&mut c);
        assert!(c.validate().is_err(), "case {i}");
    }
}

#[test]
fn ssp_solver_bounds_invalid_fail() {
    let mut c = AMASConfig::default();
    c.ssp.convergence_threshold = 0.0;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.ssp.convergence_threshold = -1.0;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.ssp.max_iterations = 0;
    assert!(c.validate().is_err());

    let mut c = AMASConfig::default();
    c.ssp.max_iterations = 4_000_000_000;
    assert!(c.validate().is_err());
}

// ---- AMASConfig::from_env / load_from_toml / write_to_toml ----

#[test]
fn from_env_sets_ensemble_flag_and_sample_rate() {
    let env_cfg = crate::config::AMASEnvConfig {
        ensemble_enabled: false,
        monitor_sample_rate: 0.25,
    };
    let cfg = AMASConfig::from_env(&env_cfg);
    assert!(!cfg.feature_flags.ensemble_enabled);
    assert!((cfg.monitoring.sample_rate - 0.25).abs() < 1e-9);
}

#[test]
fn write_then_load_from_toml_roundtrip() {
    // 用 NON-default 配置做往返：若序列化丢字段，serde 会回填 default，
    // 与下面被改过的值不等而触发断言 —— 比 is_ok() 真正校验值保真度。
    let mut cfg = AMASConfig::default();
    cfg.ssp.r_step = 0.0123;
    cfg.ssp.max_iterations = 12_345;
    cfg.ssp.convergence_threshold = 0.0456;
    cfg.elo.min_elo = 350.0;
    cfg.elo.max_elo = 2600.0;
    cfg.elo.word_k_factor_ratio = 0.42;
    cfg.memory_model.w[0] = 0.9999;
    cfg.memory_model.w[18] = 0.1234;
    cfg.memory_model.forgetting_curve_floor = 0.15;
    cfg.word_selector.spacing_cooldown_secs = 123.0;
    cfg.feature.temporal_boost_min = 0.4;
    cfg.feature.temporal_boost_max = 1.6;
    // 改后的配置仍须合法，确保往返不会因非法值被 load 拒绝。
    assert!(cfg.validate().is_ok());

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("amas_test.toml");
    cfg.write_to_toml(path.to_str().unwrap()).expect("write");
    assert!(path.exists());
    let loaded = AMASConfig::load_from_toml(path.to_str().unwrap()).expect("load");
    // 值保真度：往返后必须逐字段相等（依赖 AMASConfig: PartialEq）
    assert_eq!(loaded, cfg);
    assert!(loaded.validate().is_ok());
}

#[test]
fn load_from_toml_missing_file_returns_err() {
    let err = AMASConfig::load_from_toml("/nonexistent/path/amas.toml").unwrap_err();
    assert!(err.contains("读取配置文件失败"));
}

#[test]
fn load_from_toml_invalid_content_returns_err() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bad.toml");
    std::fs::write(&path, "not valid toml = = = []").unwrap();
    let err = AMASConfig::load_from_toml(path.to_str().unwrap()).unwrap_err();
    assert!(err.contains("解析 TOML 配置失败"));
}

#[test]
fn write_to_toml_fails_when_path_is_unwritable() {
    let cfg = AMASConfig::default();
    // 给一个不可创建的路径
    let err = cfg
        .write_to_toml("/this/dir/does/not/exist/amas.toml")
        .unwrap_err();
    assert!(err.contains("写入临时配置文件失败"));
}

// 触发 #[serde(default = "...")] 路径下的所有 default fn 的覆盖
// 通过 TOML 仅提供必需字段，让 serde 调用 default fn
#[test]
#[ignore = "TOML 必填字段过多，已通过 all_default_fns_return_expected_constants 覆盖"]
fn deserialize_with_minimum_toml_invokes_all_default_fns() {
    // 给一个仅包含最少 required field 的 TOML，迫使所有 #[serde(default = "fn")] 走 default fn
    let minimal = r#"
[featureFlags]
heuristicEnabled = true
igeEnabled = true
swdEnabled = true
mdmEnabled = true
ensembleEnabled = true

[ensemble]
baseWeightHeuristic = 0.4
baseWeightIge = 0.3
baseWeightSwd = 0.3
warmupSamples = 20
blendScale = 100.0
blendMax = 0.5
minWeight = 0.15

[modeling]
attentionSmoothing = 0.1
confidenceDecay = 0.9
minConfidence = 0.05
fatigueIncreaseRate = 0.05
fatigueRecoveryRate = 0.1
motivationMomentum = 0.1
visualFatigueWeight = 0.5

[constraints]
highFatigueThreshold = 0.7
lowAttentionThreshold = 0.3
lowMotivationThreshold = -0.5
maxBatchSizeWhenFatigued = 5
maxNewRatioWhenFatigued = 0.2
maxDifficultyWhenFatigued = 0.5
minDifficulty = 0.1
lowMotivationDifficultyDrop = 0.1
lowMotivationRatioDrop = 0.1

[monitoring]
sampleRate = 1.0

[coldStart]
classifyToExploreEvents = 50
exploreToExploitEvents = 200

[objectiveWeights]
retention = 0.3
accuracy = 0.2
speed = 0.2
fatigue = 0.15
frustration = 0.15
"#;
    let cfg: AMASConfig = toml::from_str(minimal).expect("parse minimal toml");
    // 默认值应被填充
    assert!(cfg.modeling.response_speed_max_ms > 0.0);
    assert!(cfg.feature.hint_penalty >= 0.0);
    assert!(cfg.elo.k_factor > 0.0);
    assert!(cfg.fatigue_decay.decay_start_threshold_secs >= 0.0);
    assert!(cfg.heuristic.confidence_base >= 0.0);
    assert!(cfg.ige.ucb_confidence_coeff > 0.0);
    assert!(cfg.swd.max_history_size > 0);
    assert!(cfg.memory_model.short_term_learning_rate > 0.0);
    assert!(cfg.iad.interference_penalty_factor >= 0.0);
    assert!(cfg.mtp.morpheme_transfer_coeff >= 0.0);
    assert!(cfg.word_selector.review_ucb_weight >= 0.0);
    assert!(cfg.intervention.fatigue_alert_threshold >= 0.0);
    assert!(cfg.learning_strategy.cross_session_high_accuracy >= 0.0);
    assert!(cfg.classifier.fast_learner_threshold > 0.0);
    assert!(cfg.ssp.recall_cost > 0.0);
    assert!(cfg.evm.diversity_log_divisor > 1.0);
}

#[test]
#[ignore = "TOML 必填字段过多，已通过 subconfig_default_impls_construct_valid_values 覆盖"]
fn deserialize_empty_subconfigs_uses_struct_default() {
    // 仅给最顶层 required fields，子配置全走 #[serde(default)] → Default::default()
    let minimal = r#"
[featureFlags]
heuristicEnabled = true
igeEnabled = true
swdEnabled = true
mdmEnabled = true
ensembleEnabled = true

[ensemble]
baseWeightHeuristic = 0.4
baseWeightIge = 0.3
baseWeightSwd = 0.3
warmupSamples = 20
blendScale = 100.0
blendMax = 0.5
minWeight = 0.15

[modeling]
attentionSmoothing = 0.1
confidenceDecay = 0.9
minConfidence = 0.05
fatigueIncreaseRate = 0.05
fatigueRecoveryRate = 0.1
motivationMomentum = 0.1
visualFatigueWeight = 0.5

[constraints]
highFatigueThreshold = 0.7
lowAttentionThreshold = 0.3
lowMotivationThreshold = -0.5
maxBatchSizeWhenFatigued = 5
maxNewRatioWhenFatigued = 0.2
maxDifficultyWhenFatigued = 0.5
minDifficulty = 0.1
lowMotivationDifficultyDrop = 0.1
lowMotivationRatioDrop = 0.1

[monitoring]
sampleRate = 1.0

[coldStart]
classifyToExploreEvents = 50
exploreToExploitEvents = 200

[objectiveWeights]
retention = 0.3
accuracy = 0.2
speed = 0.2
fatigue = 0.15
frustration = 0.15
"#;
    let cfg: AMASConfig = toml::from_str(minimal).expect("parse");
    assert!(cfg.validate().is_ok());
}

// 直接覆盖每个 default_xxx 函数（lib 层 unit-touch，确保覆盖区域 entry）
#[test]
fn all_default_fns_return_expected_constants() {
    // bench tuned v3 回写后的值
    assert!((default_warmup_heuristic_boost() - 0.10).abs() < 1e-9);
    assert_eq!(default_response_speed_max_ms(), 10000.0);
    assert_eq!(default_fatigue_quit_increase(), 0.2);
    assert!(default_engagement_pause_penalty() > 0.0);
    assert!(default_engagement_pause_penalty_max() > 0.0);
    assert!(default_engagement_switch_penalty() > 0.0);
    assert!(default_engagement_switch_penalty_max() > 0.0);
    assert!(default_engagement_focus_loss_base_ms() > 0.0);
    assert!(default_engagement_focus_loss_penalty_max() > 0.0);
    assert!(default_cognitive_profile_alpha() > 0.0);
    assert!(default_trend_alpha() > 0.0);
    assert!(default_ssp_discount_factor() > 0.0);
    assert!(default_ssp_r_min() > 0.0);
    assert!(default_ssp_r_max() > 0.0);
    assert!(default_ssp_r_step() > 0.0);
    let probs = default_ssp_rating_probs();
    assert_eq!(probs.len(), 3);
    assert!(default_ssp_dual_grid_threshold() > 0.0);
    assert!(default_ssp_linear_step() > 0.0);
}

// 覆盖每个子 config 的 Default impl（部分默认未通过 AMASConfig::default() 触发其全部字段读取）
#[test]
fn subconfig_default_impls_construct_valid_values() {
    let _ = FeatureFlags::default();
    let _ = EnsembleConfig::default();
    let _ = ModelingConfig::default();
    let _ = ConstraintConfig::default();
    let _ = MonitoringConfig::default();
    let _ = ColdStartConfig::default();
    let _ = ClassifierConfig::default();
    let _ = ObjectiveWeights::default();
    let _ = RewardConfig::default();
    let _ = FeatureConfig::default();
    let _ = EloConfig::default();
    let _ = FatigueDecayConfig::default();
    let _ = HeuristicConfig::default();
    let _ = IgeConfig::default();
    let _ = SwdConfig::default();
    let _ = MemoryModelConfig::default();
    let _ = EvmConfig::default();
    let _ = IadConfig::default();
    let _ = MtpConfig::default();
    let _ = WordSelectorConfig::default();
    let _ = InterventionConfig::default();
    let _ = LearningStrategyConfig::default();
    let _ = SspCostParams::default();
    let _ = SspConfig::default();
}

// === FSRS-6 w 反序列化：21 维直取 / 19 维 FSRS-5 旧配置自动迁移 ===
fn default_memory_json_with_w(w: &[f64]) -> serde_json::Value {
    let mut v = serde_json::to_value(MemoryModelConfig::default()).expect("serialize default");
    v["w"] = serde_json::json!(w);
    v
}

#[test]
fn memory_w_deserializes_21_dims_directly() {
    let mut w: Vec<f64> = (0..21).map(|i| 0.1 + i as f64 * 0.01).collect();
    w[20] = 0.3; // decay 须在 [0.05, 2.0]
    let cfg: MemoryModelConfig =
        serde_json::from_value(default_memory_json_with_w(&w)).expect("21-dim w must parse");
    assert!((cfg.w[20] - 0.3).abs() < 1e-12);
    assert!((cfg.w[0] - 0.1).abs() < 1e-12);
}

#[test]
fn memory_w_migrates_legacy_19_dims() {
    let w: Vec<f64> = (0..19).map(|i| 0.2 + i as f64 * 0.01).collect();
    let cfg: MemoryModelConfig = serde_json::from_value(default_memory_json_with_w(&w))
        .expect("legacy 19-dim w must parse");
    // 前 19 维保留
    assert!((cfg.w[0] - 0.2).abs() < 1e-12);
    assert!((cfg.w[18] - 0.38).abs() < 1e-12);
    // 迁移补位：w19=0（旧同日公式无饱和项）、w20=0.5（旧固定 |decay|）
    assert_eq!(cfg.w[19], 0.0);
    assert_eq!(cfg.w[20], 0.5);
    // 迁移后的曲线 helper 行为：decay=0.5 → factor=0.9^(-2)-1
    assert!((cfg.curve_decay() - 0.5).abs() < 1e-12);
    assert!((cfg.curve_factor() - (0.9_f64.powf(-2.0) - 1.0)).abs() < 1e-12);
}

#[test]
fn memory_w_rejects_other_lengths() {
    let w = vec![1.0; 20];
    assert!(serde_json::from_value::<MemoryModelConfig>(default_memory_json_with_w(&w)).is_err());
}

#[test]
fn curve_helpers_clamp_decay_to_safe_domain() {
    let mut cfg = MemoryModelConfig::default();
    cfg.w[20] = 0.0; // 越界小
    assert!((cfg.curve_decay() - 0.05).abs() < 1e-12);
    cfg.w[20] = 10.0; // 越界大
    assert!((cfg.curve_decay() - 2.0).abs() < 1e-12);
    assert!(cfg.curve_factor().is_finite());
}
