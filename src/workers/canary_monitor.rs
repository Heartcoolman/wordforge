//! C6:per-patch canary 自动回滚监测 worker（cron 每 5 分钟）。
//!
//! 对每条 active patch_canary 取 live 切片（aggregate_amas_version_slice）与
//! baseline_metrics_json 对比:reward 降幅 > reward_drop_threshold 或 anomaly 率
//! 升幅 > anomaly_rise_threshold → 自动回滚（status='rolled_back' + 审计 + SSE）。
//! 两阈值 E3 起改 system_settings 运行时可配（默认 0.05），免改代码重发版。
//! worker 失败仅 tracing::warn,不抛、不 disable 调度器（沿用 worker 容错惯例）。

// 判定逻辑与类型统一复用 crate::amas::monitoring（promote-canary 守卫同源）。
use crate::amas::monitoring::{should_rollback, CanaryBaseline};
use crate::state::{AppState, SseEvent};

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
        let baseline: CanaryBaseline = match serde_json::from_str(&c.baseline_metrics_json) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(
                    id = c.id,
                    error = %e,
                    "canary_monitor: baseline_metrics_json 解析失败,reward 轴本周期跳过(仅 anomaly 轴生效)"
                );
                CanaryBaseline::default()
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
        // window_secs = live 切片实际覆盖的时间跨度（首末事件间隔），语义与
        // error_rate_watchdog 的「anomaly 率在多长窗口上算得」一致；切片时间戳缺失时
        // 回退 canary 已运行时长（started_at → now），绝不再上报语义不符的 0。
        let window_secs = match (live.first_event_at, live.last_event_at) {
            (Some(first), Some(last)) => (last - first).num_seconds().max(1) as u64,
            _ => chrono::DateTime::parse_from_rfc3339(&c.started_at)
                .map(|started| {
                    (chrono::Utc::now() - started.with_timezone(&chrono::Utc))
                        .num_seconds()
                        .max(1) as u64
                })
                .unwrap_or(1),
        };
        state.broadcast_to_all_sse(SseEvent::Incident {
            error_rate: live.anomaly_rate,
            window_secs,
        });
    }
}

#[cfg(test)]
mod tests {
    // 判定逻辑（reward/anomaly 双轴、样本守卫、退化 baseline）的测试在
    // crate::amas::monitoring；此处仅守 worker 的解析契约。
    use super::*;
    use crate::store::operations::amas_telemetry::VersionMetricsSlice;

    #[test]
    fn baseline_parses_camel_case_slice_json() {
        // baseline_metrics_json 来源是 VersionMetricsSlice 的 camelCase 序列化
        let slice = VersionMetricsSlice {
            version_hash: "h".into(),
            event_count: 100,
            mean_reward: 0.77,
            anomaly_rate: 0.03,
            ..Default::default()
        };
        let json = serde_json::to_string(&slice).unwrap();
        let baseline: CanaryBaseline = serde_json::from_str(&json).unwrap();
        assert_eq!(baseline.event_count, 100); // eventCount 随切片一并解析,供 reward 轴守卫用
        assert!((baseline.mean_reward - 0.77).abs() < 1e-9);
        assert!((baseline.anomaly_rate - 0.03).abs() < 1e-9);
    }
}
