//! M1-A5：调度器健康 watchdog。
//!
//! 每分钟由 WorkerManager 调用一次（cron "0 * * * * *"）。
//! 读取 `worker_last_run` 表，对比各 worker 注册的 cron 间隔，
//! 若某 worker 连续 3 个调度周期未上报完成记录，广播 `WorkerMissed` SSE 事件。
//!
//! 去重：同一 worker 5 分钟内不重复推送。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::state::{AppState, SseEvent};
use crate::store::Store;

/// 3 周期未上报则告警
const MISS_THRESHOLD: u32 = 3;
/// 同一 worker 告警去重静默期（5 分钟）
const DEDUP_SECS: u64 = 300;

/// 每个 worker 上次告警时间戳（秒），用于去重。
static LAST_ALERT: Mutex<Option<HashMap<String, u64>>> = Mutex::new(None);

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

/// 每个调度 worker 的预期最大静默秒数（即 cron 间隔 × MISS_THRESHOLD）。
/// 键为 worker_name（与 WorkerName::as_str() 对应），值为阈值秒数。
fn expected_max_silence_secs() -> HashMap<&'static str, u64> {
    [
        // 间隔 1 小时的 worker：3 × 3600 = 10800 秒
        ("session_cleanup",          3 * 3600),
        ("password_reset_cleanup",   3 * 3600),
        ("log_export",               3 * 3600),
        // 间隔 5 分钟
        ("delayed_reward",           3 * 300),
        ("cache_cleanup",            3 * 600),  // 10 min
        ("metrics_flush",            3 * 300),
        ("llm_advisor",              3 * 1200), // 20 min
        // 间隔 1 小时（update checker）
        ("update_checker",           3 * 3600),
        // 间隔 1 分钟（watchdog 系列）
        ("error_rate_watchdog",      3 * 60),
        // 每日 / 每周 worker：宽限期 3 × 周期，周期按天计
        ("algorithm_optimization",   3 * 86400),
        ("daily_aggregation",        3 * 86400),
        ("forgetting_alert",         3 * 86400),
        ("health_analysis",          3 * 7 * 86400),  // 每周
        ("confusion_pair_cache",     3 * 7 * 86400),
        ("weekly_report",            3 * 7 * 86400),
        // 每月
        ("monitoring_retention",     3 * 30 * 86400),
    ]
    .into_iter()
    .collect()
}

/// 每分钟由 WorkerManager 调用一次。
pub async fn run(store: &Store, state: &AppState) {
    let thresholds = expected_max_silence_secs();
    let now = now_secs();

    let rows = match store.list_worker_last_run() {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error=%e, "scheduler_health_watchdog: 读取 worker_last_run 失败");
            return;
        }
    };

    // 建立 worker_name -> last_run_at 映射（仅包含有记录的 worker）
    let run_map: HashMap<&str, u64> = rows
        .iter()
        .map(|r| (r.worker_name.as_str(), r.last_run_at as u64))
        .collect();

    let mut dedup = LAST_ALERT.lock().unwrap_or_else(|e| e.into_inner());
    let dedup_map = dedup.get_or_insert_with(HashMap::new);

    for (worker_name, max_silence) in &thresholds {
        // 若 DB 中没有该 worker 的记录，说明它从未运行（或未启用），不触发告警
        let last_run = match run_map.get(worker_name).copied() {
            Some(ts) if ts > 0 => ts,
            _ => continue,
        };
        let silence = now.saturating_sub(last_run);

        // 尚未超过阈值则跳过
        if silence <= *max_silence {
            continue;
        }

        // 去重检测
        let last_alert = dedup_map.get(*worker_name).copied().unwrap_or(0);
        if now.saturating_sub(last_alert) < DEDUP_SECS {
            continue;
        }

        // 计算错过的周期数（近似）
        let cron_interval_secs = max_silence / MISS_THRESHOLD as u64;
        let miss_count = if cron_interval_secs > 0 {
            (silence / cron_interval_secs).min(100) as u32
        } else {
            MISS_THRESHOLD
        };

        tracing::warn!(
            worker = worker_name,
            silence_secs = silence,
            miss_count,
            "调度器健康 watchdog：worker 连续未上报，广播 WorkerMissed 事件"
        );

        dedup_map.insert(worker_name.to_string(), now);
        state.broadcast_to_all_sse(SseEvent::WorkerMissed {
            worker_name: worker_name.to_string(),
            miss_count,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::broadcast;

    use crate::amas::config::AMASConfig;
    use crate::amas::engine::AMASEngine;
    use crate::config::Config;
    use crate::state::AppState;
    use crate::store::Store;
    use super::run;

    fn ensure_safe_secrets() {
        let secret = "test_secret_that_is_at_least_32_characters_long_ok";
        for key in &["JWT_SECRET", "ADMIN_JWT_SECRET", "REFRESH_JWT_SECRET"] {
            if std::env::var(key)
                .map(|v| v.is_empty() || v.contains("change_me") || v.len() < 32)
                .unwrap_or(true)
            {
                std::env::set_var(key, secret);
            }
        }
    }

    fn make_store() -> (Store, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let s = Store::open(tmp.path().join("hw.db").to_str().unwrap(), 5000, 1).unwrap();
        crate::store::migrate::run(&s).unwrap();
        (s, tmp)
    }

    fn make_state(store: Arc<Store>) -> AppState {
        ensure_safe_secrets();
        let cfg = Config::from_env();
        let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
        let (tx, _) = broadcast::channel(4);
        AppState::new(store, amas, &cfg, tx, false)
    }

    /// 无记录时不广播（worker 从未运行，但 delay_reward 5 分钟窗口尚未超过）
    #[tokio::test]
    async fn no_alert_when_no_runs_and_within_window() {
        let (store, _tmp) = make_store();
        let state = make_state(Arc::new(store.clone()));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        state.active_sse().entry("d1".into()).or_default().push(
            crate::state::SseClientInfo {
                conn_id: "c1".into(),
                user_id: "u1".into(),
                platform: "test".into(),
                connected_at: std::time::Instant::now(),
                tx,
            },
        );
        // 清理去重状态
        *super::LAST_ALERT.lock().unwrap() = None;
        // 插入一条刚运行的记录（now - 10s）
        let now = super::now_secs();
        store
            .upsert_worker_last_run("delayed_reward", (now - 10) as i64, 5, "success", None)
            .unwrap();
        run(&store, &state).await;
        assert!(rx.try_recv().is_err(), "不应在正常范围内触发告警");
    }

    /// 长时间未运行应触发 WorkerMissed 事件
    #[tokio::test]
    async fn alert_fires_when_overdue() {
        let (store, _tmp) = make_store();
        let state = make_state(Arc::new(store.clone()));
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<crate::state::SseEvent>();
        state.active_sse().entry("d2".into()).or_default().push(
            crate::state::SseClientInfo {
                conn_id: "c2".into(),
                user_id: "u2".into(),
                platform: "test".into(),
                connected_at: std::time::Instant::now(),
                tx,
            },
        );
        *super::LAST_ALERT.lock().unwrap() = None;
        // error_rate_watchdog 阈值 3 × 60 = 180s；写一条 400s 前的记录
        let now = super::now_secs();
        store
            .upsert_worker_last_run(
                "error_rate_watchdog",
                (now - 400) as i64,
                1,
                "success",
                None,
            )
            .unwrap();
        run(&store, &state).await;
        let event = rx.try_recv().expect("应收到 WorkerMissed 事件");
        match event {
            crate::state::SseEvent::WorkerMissed { worker_name, miss_count } => {
                assert_eq!(worker_name, "error_rate_watchdog");
                assert!(miss_count >= 3, "miss_count={miss_count}");
            }
            other => panic!("expected WorkerMissed, got {:?}", other),
        }
    }
}
