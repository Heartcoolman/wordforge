//! C6:per-patch canary 自动回滚监测 worker（cron 每 5 分钟）。
//!
//! 对每条 active patch_canary 取 live 切片（aggregate_amas_version_slice）与
//! baseline_metrics_json 对比:reward 降幅 > REWARD_DROP_THRESHOLD 或 anomaly 率
//! 升幅 > ANOMALY_RISE_THRESHOLD → 自动回滚（status='rolled_back' + 审计 + SSE）。
//! worker 失败仅 tracing::warn,不抛、不 disable 调度器（沿用 worker 容错惯例）。

use serde::Deserialize;

use crate::state::{AppState, SseEvent};
use crate::store::operations::amas_telemetry::VersionMetricsSlice;

/// reward 平均值降幅阈值（live 比 baseline 低超过此绝对值 → 回滚）。
// TODO(C6): 设为 system_settings 可配。
const REWARD_DROP_THRESHOLD: f64 = 0.05;
/// anomaly 率升幅阈值（live 比 baseline 高超过此绝对值 → 回滚）。
// TODO(C6): 设为 system_settings 可配。
const ANOMALY_RISE_THRESHOLD: f64 = 0.05;
/// 最少样本量:live 切片 event_count 不足时跳过判定（避免早期噪声误回滚）。
const MIN_SAMPLE: u64 = 50;

/// baseline_metrics_json 反序列化目标（灰度起始时 stable 切片快照）。
/// 兼容 VersionMetricsSlice 的 camelCase 序列化字段名。
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Baseline {
    #[serde(default)]
    mean_reward: f64,
    #[serde(default)]
    anomaly_rate: f64,
}

/// 纯判定:给定 baseline 与 live 切片,是否应回滚。样本不足返回 false。
fn should_rollback(baseline: &Baseline, live: &VersionMetricsSlice) -> bool {
    if live.event_count < MIN_SAMPLE {
        return false;
    }
    let reward_drop = baseline.mean_reward - live.mean_reward;
    let anomaly_rise = live.anomaly_rate - baseline.anomaly_rate;
    reward_drop > REWARD_DROP_THRESHOLD || anomaly_rise > ANOMALY_RISE_THRESHOLD
}

pub async fn run(state: &AppState) {
    let store = state.store().clone();
    let canaries = match store.get_active_patch_canaries() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "canary_monitor: 查 active canary 失败");
            return;
        }
    };
    for c in canaries {
        let baseline: Baseline = serde_json::from_str(&c.baseline_metrics_json).unwrap_or_default();
        let live = match store.aggregate_amas_version_slice(&c.version_hash) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(id = c.id, error = %e, "canary_monitor: 聚合切片失败,跳过");
                continue;
            }
        };
        if !should_rollback(&baseline, &live) {
            continue;
        }
        if let Err(e) = store.set_patch_canary_status(c.id, "rolled_back") {
            tracing::warn!(id = c.id, error = %e, "canary_monitor: 自动回滚置状态失败");
            continue;
        }
        tracing::warn!(
            id = c.id,
            version_hash = %c.version_hash,
            baseline_reward = baseline.mean_reward,
            live_reward = live.mean_reward,
            baseline_anomaly = baseline.anomaly_rate,
            live_anomaly = live.anomaly_rate,
            "canary_monitor: patch canary 自动回滚"
        );
        state.broadcast_to_all_sse(SseEvent::Incident {
            error_rate: live.anomaly_rate,
            window_secs: 0,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slice(count: u64, reward: f64, anomaly: f64) -> VersionMetricsSlice {
        VersionMetricsSlice {
            version_hash: "h".into(),
            event_count: count,
            mean_reward: reward,
            anomaly_rate: anomaly,
            ..Default::default()
        }
    }

    #[test]
    fn rolls_back_on_reward_drop() {
        let baseline = Baseline {
            mean_reward: 0.80,
            anomaly_rate: 0.01,
        };
        let live = slice(100, 0.70, 0.01); // 降 0.10 > 0.05
        assert!(should_rollback(&baseline, &live));
    }

    #[test]
    fn rolls_back_on_anomaly_rise() {
        let baseline = Baseline {
            mean_reward: 0.80,
            anomaly_rate: 0.01,
        };
        let live = slice(100, 0.80, 0.10); // 升 0.09 > 0.05
        assert!(should_rollback(&baseline, &live));
    }

    #[test]
    fn keeps_when_within_thresholds() {
        let baseline = Baseline {
            mean_reward: 0.80,
            anomaly_rate: 0.05,
        };
        let live = slice(100, 0.78, 0.06); // 降 0.02、升 0.01 均 < 0.05
        assert!(!should_rollback(&baseline, &live));
    }

    #[test]
    fn skips_when_sample_too_small() {
        let baseline = Baseline {
            mean_reward: 0.80,
            anomaly_rate: 0.01,
        };
        let live = slice(10, 0.0, 1.0); // 即便极端退化,样本 < 50 不判定
        assert!(!should_rollback(&baseline, &live));
    }

    #[test]
    fn baseline_parses_camel_case_slice_json() {
        // baseline_metrics_json 来源是 VersionMetricsSlice 的 camelCase 序列化
        let slice = slice(100, 0.77, 0.03);
        let json = serde_json::to_string(&slice).unwrap();
        let baseline: Baseline = serde_json::from_str(&json).unwrap();
        assert!((baseline.mean_reward - 0.77).abs() < 1e-9);
        assert!((baseline.anomaly_rate - 0.03).abs() < 1e-9);
    }
}
