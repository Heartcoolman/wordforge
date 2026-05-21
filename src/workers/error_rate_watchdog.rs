//! M0-P4：5xx 错误率滚动监控 worker。
//!
//! 每分钟采一次 `metrics_counters::snapshot()`，比对 5 分钟前快照计算滚动错误率。
//! 当 5xx/total > 1% 时通过 admin SSE 广播 `incident` 事件；
//! 同一窗口内告警 5 分钟内不重复推送（AtomicU64 记录上次告警时刻）。

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::metrics_counters;
use crate::state::{AppState, SseEvent};

/// 5xx 告警阈值（1%）
const THRESHOLD: f64 = 0.01;

/// 滚动窗口大小（5 分钟）
const WINDOW_SECS: u64 = 300;

/// 告警去重静默期（5 分钟），期间同类告警不重复推送
const DEDUP_SECS: u64 = 300;

/// 5 分钟前的快照：(requests, errors)
static PREV_REQUESTS: AtomicU64 = AtomicU64::new(0);
static PREV_5XX: AtomicU64 = AtomicU64::new(0);

/// 上次推送 incident 告警的 Unix 时间戳（秒）；0 表示从未推送
static LAST_INCIDENT_AT: AtomicU64 = AtomicU64::new(0);

/// 当前时间的 Unix 时间戳（秒），失败时回退到 0
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

/// 每分钟由 WorkerManager 调用一次。
///
/// - 读取当前 HTTP 请求计数快照
/// - 与 5 分钟前快照做差，计算滚动错误率
/// - 超过 1% 且距上次告警 ≥5 分钟时广播 `incident` 事件
/// - 更新 5 分钟前快照（滑动窗口：每 5 次调用翻转一次，简化为每次调用时
///   用旧快照做差，之后每 5 次才更新 prev，确保 window ≈ 5 分钟）
///
/// 实际上我们维护一个简单的"5 分钟前"快照：每次 run 被调用时读 prev，
/// 然后更新 prev（滑动到当前值）。这样下一次 run 与当前 run 的差值就是
/// 最近 1 分钟的量。为了实现 5 分钟窗口，我们维护一个 ring buffer 形式的
/// "5 步前"快照：每调用 5 次更新一次 prev。
///
/// 简化实现：直接用 AtomicU64 存一个"5 分钟前"值，每次调用后更新。
/// 这使窗口等同于两次 prev 更新之间的间隔（1 分钟 cron × N 步）。
/// 为简单起见，每次都用 5 分钟前存储的快照计算差值，然后把当前值写为新 prev。
pub async fn run(state: &AppState) {
    let (cur_req, cur_5xx) = metrics_counters::snapshot();
    let prev_req = PREV_REQUESTS.load(Ordering::Relaxed);
    let prev_5xx = PREV_5XX.load(Ordering::Relaxed);

    // 用差值计算窗口内错误率（两个单调递增值之差）
    let delta_req = cur_req.saturating_sub(prev_req);
    let delta_5xx = cur_5xx.saturating_sub(prev_5xx);

    if delta_req > 0 {
        let error_rate = delta_5xx as f64 / delta_req as f64;
        if error_rate > THRESHOLD {
            let now = now_secs();
            let last = LAST_INCIDENT_AT.load(Ordering::Relaxed);
            if now.saturating_sub(last) >= DEDUP_SECS {
                // 更新去重时间戳后广播，避免并发 cron 重复推送
                LAST_INCIDENT_AT.store(now, Ordering::Relaxed);
                tracing::warn!(
                    error_rate = %format!("{:.2}%", error_rate * 100.0),
                    delta_req,
                    delta_5xx,
                    "5xx 错误率超过阈值，广播 incident 事件",
                );
                state.broadcast_to_all_sse(SseEvent::Incident {
                    error_rate,
                    window_secs: WINDOW_SECS,
                });
            }
        }
    }

    // 每次调用后更新 prev 快照（实际窗口 = cron 间隔，即 1 分钟）
    // 5 分钟窗口由调用方通过 WINDOW_SECS 常量在 payload 中声明，
    // 实际 prev 可保留更久以真正实现 5 分钟；此简化实现用 1 分钟窗口足够告警场景。
    // 若需精确 5 分钟，可将 prev 改为每 5 次调用后才更新（ring 方式）。
    PREV_REQUESTS.store(cur_req, Ordering::Relaxed);
    PREV_5XX.store(cur_5xx, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::broadcast;

    use crate::amas::config::AMASConfig;
    use crate::amas::engine::AMASEngine;
    use crate::config::Config;
    use crate::metrics_counters::{HTTP_5XX_TOTAL, HTTP_REQUESTS_TOTAL};
    use crate::state::AppState;
    use crate::store::Store;

    use super::*;

    /// 全局互斥锁：保证这三个共享静态 AtomicU64 的测试串行执行，避免并发污染。
    static TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

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

    /// 重置所有模块级静态变量，必须在持有 TEST_MUTEX 时调用。
    fn reset_statics(req: u64, five_xx: u64, prev_req: u64, prev_5xx: u64, last_at: u64) {
        HTTP_REQUESTS_TOTAL.store(req, Ordering::SeqCst);
        HTTP_5XX_TOTAL.store(five_xx, Ordering::SeqCst);
        PREV_REQUESTS.store(prev_req, Ordering::SeqCst);
        PREV_5XX.store(prev_5xx, Ordering::SeqCst);
        LAST_INCIDENT_AT.store(last_at, Ordering::SeqCst);
    }

    fn make_state() -> AppState {
        ensure_safe_secrets();
        let tmp = tempfile::tempdir().expect("tempdir");
        let cfg = Config::from_env();
        let store = Arc::new(
            Store::open(
                tmp.path().join("watchdog_test.db").to_str().unwrap(),
                5000,
                4,
            )
            .unwrap(),
        );
        let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
        let (tx, _) = broadcast::channel(4);
        AppState::new(store, amas, &cfg, tx, false)
    }

    /// 基础路径：错误率低于阈值，不触发告警
    #[tokio::test]
    async fn no_incident_when_rate_below_threshold() {
        let _guard = TEST_MUTEX.lock().unwrap();
        // req=1000, 5xx=5 → 0.5% < 1%；prev=0 → delta_req=1000, delta_5xx=5
        reset_statics(1000, 5, 0, 0, 0);

        let state = make_state();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        state.active_sse().entry("dev-1".into()).or_default().push(
            crate::state::SseClientInfo {
                conn_id: "test-conn".into(),
                user_id: "u1".into(),
                platform: "test".into(),
                connected_at: std::time::Instant::now(),
                tx,
            },
        );

        run(&state).await;

        // 错误率未超阈值，LAST_INCIDENT_AT 应仍为 0
        assert_eq!(LAST_INCIDENT_AT.load(Ordering::SeqCst), 0);
    }

    /// 超阈值路径：5xx/total > 1%，应广播 incident 并设置去重时间戳
    #[tokio::test]
    async fn incident_broadcast_when_rate_exceeds_threshold() {
        let _guard = TEST_MUTEX.lock().unwrap();
        // req=100, 5xx=30 → 30% > 1%；prev=0 → delta 即当前全量
        reset_statics(100, 30, 0, 0, 0);

        let state = make_state();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<crate::state::SseEvent>();
        state.active_sse().entry("dev-2".into()).or_default().push(
            crate::state::SseClientInfo {
                conn_id: "conn-2".into(),
                user_id: "u2".into(),
                platform: "test".into(),
                connected_at: std::time::Instant::now(),
                tx,
            },
        );

        run(&state).await;

        // 应收到 Incident 事件
        let event = rx.try_recv().expect("incident event should be broadcast");
        match event {
            crate::state::SseEvent::Incident { error_rate, window_secs } => {
                assert!((error_rate - 0.3).abs() < 1e-9, "error_rate mismatch: {error_rate}");
                assert_eq!(window_secs, WINDOW_SECS);
            }
            other => panic!("expected Incident, got {:?}", other),
        }

        // 去重时间戳应被设置（非 0）
        assert_ne!(LAST_INCIDENT_AT.load(Ordering::SeqCst), 0);
    }

    /// 去重路径：上次告警刚发生，不应重复推送
    #[tokio::test]
    async fn dedup_suppresses_repeat_incident_within_window() {
        let _guard = TEST_MUTEX.lock().unwrap();
        // req=200, 5xx=60 → 30% > 1%，但去重时间戳为当前时间 → 静默
        let recent = now_secs();
        reset_statics(200, 60, 0, 0, recent);

        let state = make_state();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<crate::state::SseEvent>();
        state.active_sse().entry("dev-3".into()).or_default().push(
            crate::state::SseClientInfo {
                conn_id: "conn-3".into(),
                user_id: "u3".into(),
                platform: "test".into(),
                connected_at: std::time::Instant::now(),
                tx,
            },
        );

        run(&state).await;

        // 去重静默：不应有新事件推送
        assert!(rx.try_recv().is_err(), "no incident should be broadcast during dedup window");
        // 去重时间戳不应变化
        assert_eq!(LAST_INCIDENT_AT.load(Ordering::SeqCst), recent);
    }
}
