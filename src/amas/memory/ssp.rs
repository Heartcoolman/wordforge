use std::path::Path;

use crate::amas::config::SspConfig;

/// SSP 最优策略查找表。
/// 每个 difficulty 等级对应一组 (stability_bin_index → optimal_interval_days) 映射。
pub struct SspPolicy {
    /// tables[d_index][s_index] = optimal_interval_days
    tables: Vec<Vec<f64>>,
    base: f64,
    min_index: i32,
    index_len: usize,
    /// 非均匀 bins 时存储完整 stability 列表（双网格模式）
    stability_bins: Option<Vec<f64>>,
}

impl SspPolicy {
    /// 从目录加载预计算的策略表。
    /// 目录中应包含 `ssp_policy_d{1..10}.csv`，每行格式: `stability,interval`
    pub fn load(dir: &Path, config: &SspConfig) -> Result<Self, String> {
        let index_len = (config.max_index - config.min_index) as usize;
        let mut tables = Vec::with_capacity(10);

        for d in 1..=10 {
            let path = dir.join(format!("ssp_policy_d{d}.csv"));
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

            let mut intervals = vec![1.0_f64; index_len];
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() < 3 {
                    continue;
                }
                let idx: usize = parts[0]
                    .parse()
                    .map_err(|e| format!("Invalid index in {}: {e}", path.display()))?;
                let interval: f64 = parts[2]
                    .parse()
                    .map_err(|e| format!("Invalid interval in {}: {e}", path.display()))?;
                if idx < index_len {
                    intervals[idx] = interval.max(1.0);
                }
            }
            tables.push(intervals);
        }

        Ok(Self {
            tables,
            base: config.base,
            min_index: config.min_index,
            index_len,
            stability_bins: None,
        })
    }

    /// 从内存中的 DP 结果直接构建（用于 precompute 后直接使用）
    pub fn from_tables(tables: Vec<Vec<f64>>, config: &SspConfig) -> Self {
        let index_len = (config.max_index - config.min_index) as usize;
        Self {
            tables,
            base: config.base,
            min_index: config.min_index,
            index_len,
            stability_bins: None,
        }
    }

    /// 从双网格 precompute 结果构建
    pub fn from_tables_with_bins(tables: Vec<Vec<f64>>, bins: Vec<f64>) -> Self {
        let index_len = bins.len();
        Self {
            tables,
            base: 1.05,
            min_index: 0,
            index_len,
            stability_bins: Some(bins),
        }
    }

    /// 查询 SSP 最优间隔（天）
    pub fn optimal_interval(&self, stability: f64, difficulty: f64) -> f64 {
        let d_index = ((difficulty.round() as i32).clamp(1, 10) - 1) as usize;
        let s_index = self.stability_to_index(stability);

        self.tables
            .get(d_index)
            .and_then(|t| t.get(s_index))
            .copied()
            .unwrap_or(1.0)
    }

    fn stability_to_index(&self, stability: f64) -> usize {
        if stability <= 0.0 {
            return 0;
        }
        if let Some(bins) = &self.stability_bins {
            bins.partition_point(|&b| b < stability)
                .min(bins.len().saturating_sub(1))
        } else {
            let raw = (stability.ln() / self.base.ln()) as i32 - self.min_index;
            (raw.max(0) as usize).min(self.index_len.saturating_sub(1))
        }
    }
}

pub struct PrecomputeResult {
    pub tables: Vec<Vec<f64>>,
    pub stability_list: Vec<f64>,
    pub dual_grid: bool,
}

fn build_dual_grid_stability_list(config: &SspConfig) -> Vec<f64> {
    let s_min = config.base.powi(config.min_index);
    let s_max = config.base.powi(config.max_index);
    let threshold = config.dual_grid_threshold_days.max(s_min);
    let step = config.linear_step_days;

    let mut list = Vec::new();
    // log 间距区间
    let mut s = s_min;
    while s < threshold && s < s_max {
        list.push(s);
        s *= config.base;
    }
    // 线性间距区间
    let last_log = list.last().copied().unwrap_or(s_min);
    s = last_log + step;
    while s <= s_max {
        list.push(s);
        s += step;
    }
    if list.is_empty() {
        list.push(s_min);
    }
    list
}

/// 从 (S, R) 计算间隔天数（FSRS-5 幂律遗忘曲线的反函数）
fn fsrs5_next_interval(s: f64, r: f64, config: &crate::amas::config::MemoryModelConfig) -> f64 {
    let factor = config.forgetting_curve_factor;
    let decay = config.forgetting_curve_decay;
    let ivl = s / factor * (r.powf(1.0 / decay) - 1.0);
    ivl.max(1.0).floor()
}

/// 评分后的新难度（FSRS-5 mean reversion 公式）
fn fsrs5_next_difficulty(
    d: f64,
    grade: u32,
    config: &crate::amas::config::MemoryModelConfig,
) -> f64 {
    let w = &config.w;
    let delta_d = -w[6] * (grade as f64 - 3.0);
    let d_prime = d + delta_d * (10.0 - d) / 9.0;
    let d0_4 = (w[4] - (w[5] * 3.0).exp() + 1.0).clamp(1.0, 10.0);
    (w[7] * d0_4 + (1.0 - w[7]) * d_prime).clamp(1.0, 10.0)
}

/// 3D Bellman 求解器：action = 选择目标保持率 R，建模 4 种评分结果。
pub fn precompute(
    config: &SspConfig,
    memory_config: &crate::amas::config::MemoryModelConfig,
) -> PrecomputeResult {
    let base = config.base;
    let min_idx = config.min_index;
    let gamma = config.discount_factor;
    let dual_grid = config.dual_grid_enabled;

    let stability_list: Vec<f64> = if dual_grid {
        build_dual_grid_stability_list(config)
    } else {
        let index_len = (config.max_index - config.min_index) as usize;
        (0..index_len)
            .map(|i| base.powi(i as i32 + min_idx))
            .collect()
    };
    let s_len = stability_list.len();

    let s_to_idx = |s: f64| -> usize {
        if dual_grid {
            stability_list
                .partition_point(|&b| b < s)
                .min(s_len.saturating_sub(1))
        } else {
            stability_to_raw_index(s, base, min_idx).min(s_len.saturating_sub(1))
        }
    };

    // R 离散化
    let r_count = ((config.r_max - config.r_min) / config.r_step).round() as usize + 1;
    let r_values: Vec<f64> = (0..r_count)
        .map(|i| (config.r_min + i as f64 * config.r_step).min(config.r_max))
        .collect();

    let [prob_hard, prob_good, prob_easy] = config.review_rating_probs;
    let cost_hard = config.recall_cost * 1.5;
    let cost_good = config.recall_cost;
    let cost_easy = config.recall_cost * 0.75;

    // V(d, s)：全局值函数（10 × s_len）
    let init_cost = 1_000_000.0_f64;
    let mut value: Vec<Vec<f64>> = (0..10).map(|_| vec![init_cost; s_len]).collect();
    let mut optimal_r: Vec<Vec<f64>> = (0..10).map(|_| vec![0.9; s_len]).collect();

    // 终止条件：S >= target 的状态 cost = 0
    let target_s_idx = s_to_idx(config.target_stability_days);
    for d_idx in 0..10 {
        for s_idx in target_s_idx..s_len {
            value[d_idx][s_idx] = 0.0;
        }
    }

    for _iter in 0..config.max_iterations {
        let mut max_diff: f64 = 0.0;

        for d_idx in 0..10_usize {
            let difficulty = d_idx as f64 + 1.0;

            for s_idx in (0..s_len).rev() {
                if value[d_idx][s_idx] == 0.0 {
                    continue; // 已达目标
                }
                let s = stability_list[s_idx];
                let old_v = value[d_idx][s_idx];
                let mut best_v = old_v;
                let mut best_r = optimal_r[d_idx][s_idx];

                for &r in &r_values {
                    let interval = fsrs5_next_interval(s, r, memory_config);
                    let p = fsrs5_recall(interval, s, memory_config).clamp(0.01, 0.99);

                    // Again: 遗忘
                    let s_again = fsrs5_stability_after_lapse(s, difficulty, p, memory_config);
                    let d_again = fsrs5_next_difficulty(difficulty, 1, memory_config);
                    let si_again = s_to_idx(s_again);
                    let di_again = ((d_again.round() as i32).clamp(1, 10) - 1) as usize;
                    let v_again = value[di_again][si_again];

                    // Hard (grade=2)
                    let s_hard = fsrs5_stability_after_recall(s, difficulty, p, 2, memory_config);
                    let d_hard = fsrs5_next_difficulty(difficulty, 2, memory_config);
                    let v_hard = value[((d_hard.round() as i32).clamp(1, 10) - 1) as usize]
                        [s_to_idx(s_hard)];

                    // Good (grade=3)
                    let s_good = fsrs5_stability_after_recall(s, difficulty, p, 3, memory_config);
                    let d_good = fsrs5_next_difficulty(difficulty, 3, memory_config);
                    let v_good = value[((d_good.round() as i32).clamp(1, 10) - 1) as usize]
                        [s_to_idx(s_good)];

                    // Easy (grade=4)
                    let s_easy = fsrs5_stability_after_recall(s, difficulty, p, 4, memory_config);
                    let d_easy = fsrs5_next_difficulty(difficulty, 4, memory_config);
                    let v_easy = value[((d_easy.round() as i32).clamp(1, 10) - 1) as usize]
                        [s_to_idx(s_easy)];

                    let exp_cost = (1.0 - p) * (config.forget_cost + gamma * v_again)
                        + p * prob_hard * (cost_hard + gamma * v_hard)
                        + p * prob_good * (cost_good + gamma * v_good)
                        + p * prob_easy * (cost_easy + gamma * v_easy);

                    if exp_cost < best_v {
                        best_v = exp_cost;
                        best_r = r;
                    }
                }

                max_diff = max_diff.max((old_v - best_v).abs());
                value[d_idx][s_idx] = best_v;
                optimal_r[d_idx][s_idx] = best_r;
            }
        }

        if max_diff < config.convergence_threshold && _iter > 20 {
            break;
        }
    }

    // 从 optimal_r 转换为 optimal_interval（天）
    let tables: Vec<Vec<f64>> = (0..10)
        .map(|d_idx| {
            (0..s_len)
                .map(|s_idx| {
                    let s = stability_list[s_idx];
                    let r = optimal_r[d_idx][s_idx];
                    fsrs5_next_interval(s, r, memory_config).max(1.0)
                })
                .collect()
        })
        .collect();

    PrecomputeResult {
        tables,
        stability_list,
        dual_grid,
    }
}

/// 将策略表导出为 CSV 文件
pub fn export_tables(tables: &[Vec<f64>], dir: &Path, config: &SspConfig) -> Result<(), String> {
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("Failed to create dir {}: {e}", dir.display()))?;

    let base = config.base;
    let min_idx = config.min_index;

    for (d_idx, intervals) in tables.iter().enumerate() {
        let d = d_idx + 1;
        let path = dir.join(format!("ssp_policy_d{d}.csv"));
        let mut content = String::new();
        for (s_idx, &ivl) in intervals.iter().enumerate() {
            let s = base.powi(s_idx as i32 + min_idx);
            content.push_str(&format!("{s_idx},{s:.6},{ivl:.1}\n"));
        }
        std::fs::write(&path, content)
            .map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    }
    Ok(())
}

// --- FSRS-5 公式（纯函数，与 mdm.rs 保持一致）---

fn fsrs5_recall(
    elapsed_days: f64,
    stability: f64,
    config: &crate::amas::config::MemoryModelConfig,
) -> f64 {
    let s = stability.max(0.01);
    let power_law = (1.0 + config.forgetting_curve_factor * elapsed_days / s)
        .powf(config.forgetting_curve_decay);
    let floor = config.forgetting_curve_floor;
    (floor + (1.0 - floor) * power_law).clamp(0.0, 1.0)
}

fn fsrs5_stability_after_recall(
    s: f64,
    d: f64,
    r: f64,
    grade: u32,
    config: &crate::amas::config::MemoryModelConfig,
) -> f64 {
    let w = &config.w;
    let bonus = match grade {
        2 => w[15],
        4 => w[16],
        _ => 1.0,
    };
    let s_inc = (w[8].exp()
        * (11.0 - d)
        * s.max(0.01).powf(-w[9])
        * ((w[10] * (1.0 - r)).exp() - 1.0)
        * bonus)
        .max(0.0);
    (s * (s_inc + 1.0)).max(0.01)
}

fn fsrs5_stability_after_lapse(
    s: f64,
    d: f64,
    r: f64,
    config: &crate::amas::config::MemoryModelConfig,
) -> f64 {
    let w = &config.w;
    (w[11] * d.powf(-w[12]) * ((s + 1.0).powf(w[13]) - 1.0) * (w[14] * (1.0 - r)).exp())
        .clamp(0.01, s.max(0.01))
}

fn stability_to_raw_index(stability: f64, base: f64, min_index: i32) -> usize {
    if stability <= 0.0 {
        return 0;
    }
    let raw = (stability.ln() / base.ln()).round() as i32 - min_index;
    raw.max(0) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::amas::config::{MemoryModelConfig, SspConfig};
    use crate::amas::memory::mdm;

    macro_rules! ssp_debug {
        ($($arg:tt)*) => {
            if std::env::var_os("WORDFORGE_SSP_DEBUG").is_some() {
                eprintln!($($arg)*);
            }
        };
    }

    #[test]
    fn precompute_produces_valid_tables() {
        let ssp_config = SspConfig {
            max_iterations: 50,
            ..Default::default()
        };
        let mem_config = MemoryModelConfig::default();
        let result = precompute(&ssp_config, &mem_config);
        assert_eq!(result.tables.len(), 10);
        let index_len = (ssp_config.max_index - ssp_config.min_index) as usize;
        for t in &result.tables {
            assert_eq!(t.len(), index_len);
            for &ivl in t {
                assert!(ivl >= 1.0, "interval must be >= 1 day, got {ivl}");
            }
        }
    }

    #[test]
    fn policy_lookup_returns_valid_interval() {
        let ssp_config = SspConfig {
            max_iterations: 50,
            ..Default::default()
        };
        let mem_config = MemoryModelConfig::default();
        let result = precompute(&ssp_config, &mem_config);
        let policy = SspPolicy::from_tables(result.tables, &ssp_config);

        let ivl = policy.optimal_interval(1.0, 5.0);
        assert!(
            ivl >= 1.0,
            "interval for S=1, D=5 should be >= 1, got {ivl}"
        );

        // 高 stability 应给出更长间隔
        let ivl_low_s = policy.optimal_interval(5.0, 5.0);
        let ivl_high_s = policy.optimal_interval(100.0, 5.0);
        assert!(
            ivl_high_s >= ivl_low_s,
            "higher stability should give longer interval: S=5→{ivl_low_s}, S=100→{ivl_high_s}"
        );
    }

    #[test]
    fn stability_index_boundary() {
        let config = SspConfig::default();
        let policy = SspPolicy::from_tables(
            vec![vec![1.0; (config.max_index - config.min_index) as usize]; 10],
            &config,
        );
        assert_eq!(policy.optimal_interval(0.0, 5.0), 1.0);
        assert_eq!(policy.optimal_interval(999999.0, 5.0), 1.0);
        assert_eq!(policy.optimal_interval(1.0, 0.0), 1.0);
        assert_eq!(policy.optimal_interval(1.0, 15.0), 1.0);
    }

    #[test]
    fn stability_to_index_clamps_below_min_and_above_max() {
        let cfg = SspConfig {
            max_iterations: 10,
            dual_grid_enabled: false,
            ..Default::default()
        };
        let result = precompute(&cfg, &MemoryModelConfig::default());
        let policy = SspPolicy::from_tables(result.tables, &cfg);
        // stability <= 0 → index 0 → 有效 interval
        let low = policy.optimal_interval(-1.0, 5.0);
        assert!(low >= 1.0);
        // 极高 stability 应饱和到最后一个 bin
        let high = policy.optimal_interval(1e12, 5.0);
        assert!(high >= 1.0);
    }

    #[test]
    fn from_tables_with_bins_uses_partition_point() {
        let bins = vec![1.0, 5.0, 25.0, 100.0];
        let tables: Vec<Vec<f64>> = (0..10).map(|d| vec![(d as f64 + 1.0) * 2.0; 4]).collect();
        let policy = SspPolicy::from_tables_with_bins(tables, bins);
        // S=0 → index 0
        let v0 = policy.optimal_interval(0.0, 1.0);
        assert!(v0 > 0.0);
        // S 在第一个 bin 区间
        let v_mid = policy.optimal_interval(3.0, 5.0);
        assert!(v_mid > 0.0);
        // S 超出最后 bin
        let v_high = policy.optimal_interval(1000.0, 10.0);
        assert!(v_high > 0.0);
    }

    #[test]
    fn dual_grid_precompute_includes_log_and_linear_segments() {
        let ssp_config = SspConfig {
            max_iterations: 30,
            dual_grid_enabled: true,
            dual_grid_threshold_days: 10.0,
            linear_step_days: 5.0,
            ..Default::default()
        };
        let result = precompute(&ssp_config, &MemoryModelConfig::default());
        assert!(result.dual_grid);
        assert_eq!(result.tables.len(), 10);
        assert!(!result.stability_list.is_empty());
        // 列表应至少跨越 threshold + linear_step
        let max_s = result
            .stability_list
            .iter()
            .cloned()
            .fold(0.0_f64, f64::max);
        assert!(max_s > 10.0);
    }

    #[test]
    fn export_tables_creates_one_csv_per_difficulty() {
        let cfg = SspConfig {
            max_iterations: 10,
            ..Default::default()
        };
        let tables: Vec<Vec<f64>> = (0..10)
            .map(|d| vec![d as f64 + 1.0; (cfg.max_index - cfg.min_index) as usize])
            .collect();
        let dir = tempfile::tempdir().expect("tempdir");
        export_tables(&tables, dir.path(), &cfg).expect("export");
        for d in 1..=10 {
            let path = dir.path().join(format!("ssp_policy_d{d}.csv"));
            assert!(path.exists(), "missing csv for d={d}");
            let content = std::fs::read_to_string(&path).unwrap();
            assert!(!content.is_empty());
            // 每行 idx,s,interval
            assert!(content.lines().next().unwrap().split(',').count() >= 3);
        }
    }

    #[test]
    fn load_returns_err_when_csv_missing() {
        let cfg = SspConfig::default();
        let dir = tempfile::tempdir().expect("tempdir");
        let err = match SspPolicy::load(dir.path(), &cfg) {
            Err(e) => e,
            Ok(_) => panic!("expected Err"),
        };
        assert!(err.to_lowercase().contains("failed to read"));
    }

    #[test]
    fn load_skips_short_and_empty_lines() {
        let cfg = SspConfig {
            max_index: 5,
            min_index: 0,
            ..Default::default()
        };
        let dir = tempfile::tempdir().expect("tempdir");
        // 写 10 个 d 文件，包含空行、短行和有效行
        for d in 1..=10 {
            let path = dir.path().join(format!("ssp_policy_d{d}.csv"));
            std::fs::write(
                &path,
                "\n\
                 short,line\n\
                 0,1.0,3.5\n\
                 1,1.05,4.2\n",
            )
            .unwrap();
        }
        let policy = SspPolicy::load(dir.path(), &cfg).expect("load");
        // d=1 → table[0]，idx=0 → 3.5
        assert_eq!(policy.tables[0][0], 3.5);
        assert_eq!(policy.tables[0][1], 4.2);
        // 剩余位置应为 fallback 1.0
        assert_eq!(policy.tables[0][2], 1.0);
    }

    #[test]
    fn load_returns_err_on_invalid_numeric_field() {
        let cfg = SspConfig {
            max_index: 5,
            min_index: 0,
            ..Default::default()
        };
        let dir = tempfile::tempdir().expect("tempdir");
        for d in 1..=10 {
            let path = dir.path().join(format!("ssp_policy_d{d}.csv"));
            std::fs::write(&path, "abc,1.0,3.5\n").unwrap();
        }
        let err = match SspPolicy::load(dir.path(), &cfg) {
            Err(e) => e,
            Ok(_) => panic!("expected Err"),
        };
        assert!(err.to_lowercase().contains("invalid index"));
    }

    #[test]
    fn load_returns_err_on_invalid_interval() {
        let cfg = SspConfig {
            max_index: 5,
            min_index: 0,
            ..Default::default()
        };
        let dir = tempfile::tempdir().expect("tempdir");
        for d in 1..=10 {
            let path = dir.path().join(format!("ssp_policy_d{d}.csv"));
            std::fs::write(&path, "0,1.0,not-a-float\n").unwrap();
        }
        let err = match SspPolicy::load(dir.path(), &cfg) {
            Err(e) => e,
            Ok(_) => panic!("expected Err"),
        };
        assert!(err.to_lowercase().contains("invalid interval"));
    }

    #[test]
    fn load_ignores_out_of_range_index() {
        let cfg = SspConfig {
            max_index: 3,
            min_index: 0,
            ..Default::default()
        };
        let dir = tempfile::tempdir().expect("tempdir");
        for d in 1..=10 {
            let path = dir.path().join(format!("ssp_policy_d{d}.csv"));
            // idx=99 应被忽略
            std::fs::write(&path, "99,1.0,7.7\n0,1.0,3.0\n").unwrap();
        }
        let policy = SspPolicy::load(dir.path(), &cfg).expect("load");
        assert_eq!(policy.tables[0][0], 3.0);
    }

    #[test]
    fn precompute_converges_early_via_threshold() {
        let cfg = SspConfig {
            max_iterations: 1000,       // 高上限
            convergence_threshold: 1e9, // 极易满足
            ..Default::default()
        };
        let result = precompute(&cfg, &MemoryModelConfig::default());
        // convergence break 走过 → 仍能产出有效表
        assert_eq!(result.tables.len(), 10);
    }

    #[test]
    fn dual_grid_build_falls_back_when_threshold_below_smin() {
        // base ^ min_index 极小，threshold 设为 0 → log 区间空，仅线性段
        let cfg = SspConfig {
            dual_grid_enabled: true,
            dual_grid_threshold_days: 0.0,
            linear_step_days: 1.0,
            min_index: 0,
            max_index: 5,
            max_iterations: 5,
            ..Default::default()
        };
        let result = precompute(&cfg, &MemoryModelConfig::default());
        assert!(result.dual_grid);
        assert!(!result.stability_list.is_empty());
    }

    #[test]
    fn stability_to_raw_index_zero_returns_zero() {
        // 直接覆盖 production helper 的 stability<=0 分支
        let idx = stability_to_raw_index(0.0, 1.05, -30);
        assert_eq!(idx, 0);
        let idx2 = stability_to_raw_index(-1.0, 1.05, -30);
        assert_eq!(idx2, 0);
    }

    #[test]
    fn export_tables_errors_when_dir_creation_fails() {
        let tables: Vec<Vec<f64>> = (0..10).map(|_| vec![1.0; 10]).collect();
        let cfg = SspConfig::default();
        // 无法创建（路径在不存在的目录下）
        let result = export_tables(&tables, Path::new("/this/path/does/not/exist/foo"), &cfg);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("Failed to") || msg.contains("dir"));
    }

    #[test]
    fn precompute_with_disabled_dual_grid_uses_uniform_bins() {
        let ssp_config = SspConfig {
            max_iterations: 30,
            dual_grid_enabled: false,
            ..Default::default()
        };
        let result = precompute(&ssp_config, &MemoryModelConfig::default());
        assert!(!result.dual_grid);
        let expected = (ssp_config.max_index - ssp_config.min_index) as usize;
        assert_eq!(result.stability_list.len(), expected);
    }

    #[test]
    fn export_and_reload_roundtrip() {
        let ssp_config = SspConfig {
            max_iterations: 50,
            ..Default::default()
        };
        let mem_config = MemoryModelConfig::default();
        let result = precompute(&ssp_config, &mem_config);

        let dir = std::env::temp_dir().join("ssp_test_roundtrip");
        let _ = std::fs::remove_dir_all(&dir);
        export_tables(&result.tables, &dir, &ssp_config).unwrap();

        let loaded = SspPolicy::load(&dir, &ssp_config).unwrap();
        let original = &result.tables[4][50];
        let loaded_val = loaded.tables[4][50];
        assert!(
            (original - loaded_val).abs() < 0.2,
            "roundtrip mismatch: original={original}, loaded={loaded_val}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[ignore] // 需要大量迭代，手动运行: cargo test --release -- ssp_vs_mdm --ignored --nocapture
    fn ssp_vs_mdm_interval_comparison() {
        let ssp_config = SspConfig {
            max_iterations: 200_000,
            ..Default::default()
        };
        let mem_config = MemoryModelConfig::default();
        let result = precompute(&ssp_config, &mem_config);

        let policy = SspPolicy::from_tables(result.tables, &ssp_config);

        ssp_debug!("\n=== SSP 策略表全览 ===");
        ssp_debug!(
            "{:<6} {:<10} {:<10} {:<10} {:<10} {:<10} {:<10} {:<10} {:<10} {:<10} {:<10}",
            "S(天)",
            "D=1",
            "D=2",
            "D=3",
            "D=4",
            "D=5",
            "D=6",
            "D=7",
            "D=8",
            "D=9",
            "D=10"
        );
        for s in [
            0.3, 0.5, 1.0, 2.0, 3.0, 5.0, 8.0, 12.0, 20.0, 30.0, 50.0, 80.0, 120.0, 200.0, 300.0,
        ] {
            let mut row = format!("{:<6.1}", s);
            for d in 1..=10 {
                row.push_str(&format!(" {:<9.0}", policy.optimal_interval(s, d as f64)));
            }
            ssp_debug!("{row}");
        }

        ssp_debug!("\n=== SSP vs MDM 间隔对比 ===");
        ssp_debug!(
            "{:<6} {:<12} {:<15} {:<15} {:<10}",
            "D",
            "S(days)",
            "MDM(days)",
            "SSP(days)",
            "Δ%"
        );

        for d in [1.0, 3.0, 5.0, 7.0, 10.0] {
            for s in [0.5, 1.0, 3.0, 8.0, 20.0, 50.0, 100.0, 200.0] {
                let mut mdm_state = mdm::MdmState::default();
                mdm_state.stability = s;
                mdm_state.difficulty = d;
                mdm_state.last_review_at = Some(0);

                let mdm_interval_secs = mdm::compute_interval(&mdm_state, 0.9, 1.0, &mem_config);
                let mdm_days = mdm_interval_secs as f64 / 86400.0;

                let ssp_days = policy.optimal_interval(s, d);

                let delta_pct = if mdm_days > 0.01 {
                    (ssp_days - mdm_days) / mdm_days * 100.0
                } else {
                    0.0
                };

                ssp_debug!(
                    "{:<6.0} {:<12.1} {:<15.2} {:<15.2} {:<+10.1}",
                    d,
                    s,
                    mdm_days,
                    ssp_days,
                    delta_pct
                );

                // 两者都应产生有效间隔
                assert!(mdm_days >= 0.0, "MDM interval must be >= 0");
                assert!(ssp_days >= 1.0, "SSP interval must be >= 1 day");
            }
        }
    }

    #[test]
    #[ignore] // 需要大量迭代，手动运行: cargo test -- dual_grid --ignored --nocapture
    fn dual_grid_produces_valid_policy() {
        let ssp_config = SspConfig {
            max_iterations: 1000,
            dual_grid_enabled: true,
            dual_grid_threshold_days: 10.0,
            linear_step_days: 5.0,
            ..Default::default()
        };
        let mem_config = MemoryModelConfig::default();
        let result = precompute(&ssp_config, &mem_config);
        assert!(result.dual_grid);
        assert_eq!(result.tables.len(), 10);

        let policy = SspPolicy::from_tables_with_bins(result.tables, result.stability_list.clone());

        // 基本有效性
        let ivl = policy.optimal_interval(5.0, 5.0);
        assert!(ivl >= 1.0, "dual grid interval must be >= 1, got {ivl}");

        // 单调性：同 D 下，更高 S 应给出更长间隔
        let ivl_low = policy.optimal_interval(1.0, 5.0);
        let ivl_high = policy.optimal_interval(30.0, 5.0);
        assert!(
            ivl_high >= ivl_low,
            "higher stability should give longer interval: S=1→{ivl_low}, S=30→{ivl_high}"
        );

        ssp_debug!(
            "\n双网格 bins 数量: {} (原始均匀: {})",
            result.stability_list.len(),
            (ssp_config.max_index - ssp_config.min_index) as usize
        );
    }

    #[test]
    #[ignore] // cargo test --release -- ssp_parameter_sweep --ignored --nocapture
    fn ssp_parameter_sweep() {
        use crate::amas::config::SspCostParams;

        let mem_config = MemoryModelConfig::default();

        // 采样点
        let sample_points: Vec<(f64, f64)> = vec![
            (1.0, 3.0),
            (1.0, 7.0),
            (5.0, 3.0),
            (5.0, 7.0),
            (15.0, 3.0),
            (15.0, 7.0),
            (30.0, 3.0),
            (30.0, 7.0),
            (60.0, 3.0),
            (60.0, 7.0),
            (100.0, 5.0),
            (150.0, 5.0),
            (200.0, 3.0),
            (200.0, 7.0),
            (300.0, 5.0),
        ];

        // MDM 基线 (R=0.90)
        let mdm_intervals: Vec<f64> = sample_points
            .iter()
            .map(|&(s, d)| {
                let mut state = mdm::MdmState::default();
                state.stability = s;
                state.difficulty = d;
                state.last_review_at = Some(0);
                let secs = mdm::compute_interval(&state, 0.90, 1.0, &mem_config);
                secs as f64 / 86400.0
            })
            .collect();

        // === Phase 1: sweep forget_cost/recall_cost 比值 ===
        let ratios: Vec<f64> = vec![1.5, 2.0, 3.0, 5.0, 8.0, 12.0, 20.0, 30.0];

        ssp_debug!("\n============================================================");
        ssp_debug!("Phase 1: forget/recall 比值扫描 (recall_cost=3, 10k iter)");
        ssp_debug!("============================================================");
        ssp_debug!("{:<8} {:<10}", "比值", "对齐分数");

        let mut best_ratio = 3.0_f64;
        let mut best_score = 0.0_f64;

        for &ratio in &ratios {
            let ssp_config = SspConfig {
                recall_cost: 3.0,
                forget_cost: 3.0 * ratio,
                max_iterations: 10_000,
                ..Default::default()
            };
            let result = precompute(&ssp_config, &mem_config);
            let policy = SspPolicy::from_tables(result.tables, &ssp_config);

            let mut score_sum = 0.0;
            let mut count = 0;
            for (i, &(s, d)) in sample_points.iter().enumerate() {
                let ssp = policy.optimal_interval(s, d);
                let mdm = mdm_intervals[i];
                if mdm > 0.01 && ssp > 0.01 {
                    score_sum += (ssp / mdm).min(mdm / ssp);
                    count += 1;
                }
            }
            let score = if count > 0 {
                score_sum / count as f64
            } else {
                0.0
            };

            ssp_debug!("{:<8.1} {:<10.4}", ratio, score);

            if score > best_score {
                best_score = score;
                best_ratio = ratio;
            }
        }

        ssp_debug!("\n最佳比值: {best_ratio:.1} (对齐分数: {best_score:.4})");

        // === Phase 2: 用最佳比值展示完整策略表 ===
        ssp_debug!("\n============================================================");
        ssp_debug!("Phase 2: 最佳比值 {best_ratio:.1} 的完整策略表 (200k iter)");
        ssp_debug!("============================================================");

        let ssp_config = SspConfig {
            recall_cost: 3.0,
            forget_cost: 3.0 * best_ratio,
            max_iterations: 200_000,
            ..Default::default()
        };
        let result = precompute(&ssp_config, &mem_config);
        let policy = SspPolicy::from_tables(result.tables, &ssp_config);

        ssp_debug!(
            "\n{:<8} {:<8} {:<10} {:<10} {:<10}",
            "S(天)",
            "D",
            "SSP(天)",
            "MDM(天)",
            "SSP/MDM"
        );
        for &(s, d) in &sample_points {
            let ssp = policy.optimal_interval(s, d);
            let mut state = mdm::MdmState::default();
            state.stability = s;
            state.difficulty = d;
            state.last_review_at = Some(0);
            let mdm = mdm::compute_interval(&state, 0.90, 1.0, &mem_config) as f64 / 86400.0;
            let ratio_val = if mdm > 0.01 { ssp / mdm } else { 0.0 };
            ssp_debug!(
                "{:<8.1} {:<8.0} {:<10.1} {:<10.1} {:<10.2}",
                s,
                d,
                ssp,
                mdm,
                ratio_val
            );
        }

        // === Phase 3: 在最佳比值基础上 sweep cost_params ===
        ssp_debug!("\n============================================================");
        ssp_debug!("Phase 3: cost_params 调制扫描");
        ssp_debug!("============================================================");

        let param_combos: Vec<(f64, f64, &str)> = vec![
            (0.0, 0.0, "baseline"),
            (0.5, 0.0, "forget_s=0.5"),
            (1.0, 0.0, "forget_s=1.0"),
            (0.0, 0.3, "forget_d=0.3"),
            (0.0, 0.6, "forget_d=0.6"),
            (0.5, 0.3, "forget_s=0.5,d=0.3"),
            (1.0, 0.5, "forget_s=1.0,d=0.5"),
        ];

        ssp_debug!("{:<25} {:<10}", "参数组合", "对齐分数");
        let mut best_params_score = 0.0_f64;
        let mut best_params_name = "baseline";
        let mut best_fs = 0.0_f64;
        let mut best_fd = 0.0_f64;

        for &(fs, fd, name) in &param_combos {
            let ssp_config = SspConfig {
                recall_cost: 3.0,
                forget_cost: 3.0 * best_ratio,
                max_iterations: 10_000,
                cost_params: SspCostParams {
                    forget_s_coeff: fs,
                    forget_d_coeff: fd,
                    ..Default::default()
                },
                ..Default::default()
            };
            let result = precompute(&ssp_config, &mem_config);
            let policy = SspPolicy::from_tables(result.tables, &ssp_config);

            let mut score_sum = 0.0;
            let mut count = 0;
            for (i, &(s, d)) in sample_points.iter().enumerate() {
                let ssp = policy.optimal_interval(s, d);
                let mdm = mdm_intervals[i];
                if mdm > 0.01 && ssp > 0.01 {
                    score_sum += (ssp / mdm).min(mdm / ssp);
                    count += 1;
                }
            }
            let score = if count > 0 {
                score_sum / count as f64
            } else {
                0.0
            };
            ssp_debug!("{:<25} {:<10.4}", name, score);

            if score > best_params_score {
                best_params_score = score;
                best_params_name = name;
                best_fs = fs;
                best_fd = fd;
            }
        }

        ssp_debug!(
            "\n最佳 cost_params: {} (分数: {best_params_score:.4})",
            best_params_name
        );
        ssp_debug!("\n=== 推荐配置 ===");
        ssp_debug!("recall_cost: 3.0");
        ssp_debug!("forget_cost: {:.1}", 3.0 * best_ratio);
        ssp_debug!("forget_s_coeff: {best_fs:.1}");
        ssp_debug!("forget_d_coeff: {best_fd:.1}");
    }
}
