//! C6:per-patch canary 自动回滚监测 worker（cron 每 5 分钟）。
//!
//! 对每条 active patch_canary 取 live 切片（aggregate_amas_version_slice）与
//! baseline_metrics_json 对比:reward 降幅 > reward_drop_threshold 或 anomaly 率
//! 升幅 > anomaly_rise_threshold → 自动回滚（status='rolled_back' + 审计 + SSE）。
//! 两阈值 E3 起改 system_settings 运行时可配（默认 0.05），免改代码重发版。
//! worker 失败仅 tracing::warn,不抛、不 disable 调度器（沿用 worker 容错惯例）。

use serde::Deserialize;

use crate::state::{AppState, SseEvent};
use crate::store::operations::amas_telemetry::VersionMetricsSlice;

/// 最少样本量:live 切片 event_count 不足时跳过判定（避免早期噪声误回滚）。
const MIN_SAMPLE: u64 = 50;

/// baseline_metrics_json 反序列化目标（灰度起始时 stable 切片快照）。
/// 兼容 VersionMetricsSlice 的 camelCase 序列化字段名。
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Baseline {
    /// baseline 切片自身样本量。退化 baseline(解析失败 default / 零样本 stable 切片)下
    /// event_count 不足,reward 轴对照不可信,须跳过以免漏报回滚。
    #[serde(default)]
    event_count: u64,
    #[serde(default)]
    mean_reward: f64,
    #[serde(default)]
    anomaly_rate: f64,
}

/// 纯判定:给定 baseline、live 切片与两阈值,是否应回滚。样本不足返回 false。
fn should_rollback(
    baseline: &Baseline,
    live: &VersionMetricsSlice,
    reward_drop_threshold: f64,
    anomaly_rise_threshold: f64,
) -> bool {
    if live.event_count < MIN_SAMPLE {
        return false;
    }
    // anomaly 轴:不依赖 baseline 样本量。baseline.anomaly_rate 退化为 0 时 anomaly_rise=live,
    // 仍能正确捕获异常率飙升,故始终生效(也是退化 baseline 下的兜底回滚轴)。
    let anomaly_rise = live.anomaly_rate - baseline.anomaly_rate;
    if anomaly_rise > anomaly_rise_threshold {
        return true;
    }
    // reward 轴:reward_drop = baseline - live,仅在 baseline 样本充足时可信。退化 baseline
    //（解析失败 default 或零样本 stable 切片,event_count < MIN_SAMPLE 且 mean_reward 记 0）下,
    // reward_drop 对任意正 live reward 恒为负 → reward 轴漏报回滚。故 baseline 样本不足时跳过
    // reward 轴,避免把"应回滚"误判为"通过"。
    if baseline.event_count < MIN_SAMPLE {
        return false;
    }
    let reward_drop = baseline.mean_reward - live.mean_reward;
    reward_drop > reward_drop_threshold
}

pub async fn run(state: &AppState) {
    let store = state.store().clone();
    // E3:两阈值改 system_settings 运行时可配,读失败回落默认 0.05。
    let (reward_drop_threshold, anomaly_rise_threshold) = match store.get_system_settings() {
        Ok(s) => (
            s.canary_reward_drop_threshold,
            s.canary_anomaly_rise_threshold,
        ),
        Err(e) => {
            tracing::warn!(error = %e, "canary_monitor: 读 system_settings 阈值失败,回落默认 0.05");
            (0.05, 0.05)
        }
    };
    let canaries = match store.get_active_patch_canaries() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "canary_monitor: 查 active canary 失败");
            return;
        }
    };
    for c in canaries {
        // 解析失败不再静默 default:warn 使"reward 轴本周期失效"可观测,且 default 的
        // event_count=0 会让 should_rollback 自动跳过 reward 轴(anomaly 轴仍兜底)。
        let baseline: Baseline = match serde_json::from_str(&c.baseline_metrics_json) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(
                    id = c.id,
                    error = %e,
                    "canary_monitor: baseline_metrics_json 解析失败,reward 轴本周期跳过(仅 anomaly 轴生效)"
                );
                Baseline::default()
            }
        };
        let live = match store.aggregate_amas_version_slice(&c.version_hash) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(id = c.id, error = %e, "canary_monitor: 聚合切片失败,跳过");
                continue;
            }
        };
        if !should_rollback(
            &baseline,
            &live,
            reward_drop_threshold,
            anomaly_rise_threshold,
        ) {
            continue;
        }
        if let Err(e) = store.rollback_patch_canary_and_release_suggestion(c.id) {
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
        // W2-1:自动回滚是最高优先级运营事件 → 写 admin 告警收件箱(持久可追溯,补 D1 覆盖盲区)。
        // kind 拼 version_hash:system_alerts 按 (source,kind) dedup,静态 kind 会让同周期多 patch
        // 回滚合并成一行丢明细;version_hash 入 kind 保证每个被回滚 patch 独立成行。SSE 并存不互替
        //（Incident 瞬态刷新即丢,收件箱才持久）。本 worker 已在 async 上下文直调阻塞 store 方法,
        // 此处直调即与既有惯例一致,无需 spawn_blocking。
        if let Err(e) = store.record_system_alert(
            "canary_monitor",
            &format!("auto_rollback:{}", c.version_hash),
            "warning",
            "AMAS patch canary 自动回滚",
            &format!(
                "patch {} 触发自动回滚:baseline reward {:.4}→live {:.4}, baseline anomaly {:.4}→live {:.4}",
                c.version_hash,
                baseline.mean_reward,
                live.mean_reward,
                baseline.anomaly_rate,
                live.anomaly_rate
            ),
        ) {
            tracing::warn!(id = c.id, error = %e, "canary_monitor: 写告警收件箱失败");
        }
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

    fn baseline(reward: f64, anomaly: f64) -> Baseline {
        Baseline {
            event_count: 100, // 样本充足:reward 轴生效
            mean_reward: reward,
            anomaly_rate: anomaly,
        }
    }

    #[test]
    fn rolls_back_on_reward_drop() {
        let baseline = baseline(0.80, 0.01);
        let live = slice(100, 0.70, 0.01); // 降 0.10 > 0.05
        assert!(should_rollback(&baseline, &live, 0.05, 0.05));
    }

    #[test]
    fn rolls_back_on_anomaly_rise() {
        let baseline = baseline(0.80, 0.01);
        let live = slice(100, 0.80, 0.10); // 升 0.09 > 0.05
        assert!(should_rollback(&baseline, &live, 0.05, 0.05));
    }

    #[test]
    fn keeps_when_within_thresholds() {
        let baseline = baseline(0.80, 0.05);
        let live = slice(100, 0.78, 0.06); // 降 0.02、升 0.01 均 < 0.05
        assert!(!should_rollback(&baseline, &live, 0.05, 0.05));
    }

    #[test]
    fn skips_when_sample_too_small() {
        let baseline = baseline(0.80, 0.01);
        let live = slice(10, 0.0, 1.0); // live 样本 < 50 不判定
        assert!(!should_rollback(&baseline, &live, 0.05, 0.05));
    }

    #[test]
    fn skips_reward_axis_on_degraded_baseline() {
        // 退化 baseline(event_count=0,如解析失败 default 或零样本 stable 切片):reward 轴跳过,
        // 不因 baseline.mean_reward=0 把"应回滚"漏判。live reward 偏低但 anomaly 正常 → 不回滚。
        let baseline = Baseline::default(); // event_count=0, mean_reward=0, anomaly_rate=0
        let live = slice(100, 0.70, 0.02);
        assert!(!should_rollback(&baseline, &live, 0.05, 0.05));
    }

    #[test]
    fn degraded_baseline_still_rolls_back_on_anomaly() {
        // 退化 baseline 下 anomaly 轴仍兜底:live 异常率飙升仍触发回滚。
        let baseline = Baseline::default();
        let live = slice(100, 0.70, 0.20); // anomaly 0.20 - 0 = 0.20 > 0.05
        assert!(should_rollback(&baseline, &live, 0.05, 0.05));
    }

    #[test]
    fn baseline_parses_camel_case_slice_json() {
        // baseline_metrics_json 来源是 VersionMetricsSlice 的 camelCase 序列化
        let slice = slice(100, 0.77, 0.03);
        let json = serde_json::to_string(&slice).unwrap();
        let baseline: Baseline = serde_json::from_str(&json).unwrap();
        assert_eq!(baseline.event_count, 100); // eventCount 随切片一并解析,供 reward 轴守卫用
        assert!((baseline.mean_reward - 0.77).abs() < 1e-9);
        assert!((baseline.anomaly_rate - 0.03).abs() < 1e-9);
    }
}
