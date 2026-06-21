//! M0-P4：5xx 错误率滚动监控 worker。
//!
//! 每分钟采一次 `metrics_counters::snapshot()`，比对 5 分钟前快照计算滚动错误率。
//! 当 5xx/total > 1% 时通过 admin SSE 广播 `incident` 事件；
//! 同一窗口内告警 5 分钟内不重复推送（AtomicU64 记录上次告警时刻）。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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

/// 上次翻转 PREV 快照的 Unix 时间戳（秒）；用于上报 delta 实际覆盖的窗口长度。
/// PREV 每 WINDOW_STEPS 次 tick 才翻转，翻转后第 1 个 tick 的 delta 仅覆盖约 60s，
/// 到第 5 个 tick 才接近 300s，故 incident 上报的 window_secs 必须用实际经过时长，
/// 不能恒为 WINDOW_SECS，否则会把 1 分钟样本误标为 5 分钟滚动率。
static PREV_FLIP_AT: AtomicU64 = AtomicU64::new(0);

/// 冷启动标志：首次 run() 时 PREV 尚未被任何真实快照填充（仍为初始 0），
/// 此时 delta 实为"自进程启动以来累计"而非声明的 5 分钟滚动窗口，会在低流量启动期
/// 偶发 5xx 时误报 incident。首次仅种子化 PREV 后直接返回，第二次起才做阈值判定。
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// cron tick 计数：每 5 次（=5 分钟，cron 间隔 1 分钟）才翻转一次 PREV，
/// 使 delta 真正覆盖 WINDOW_SECS=300s 窗口，而非每分钟翻转得到的 1 分钟窗口。
static TICK_COUNT: AtomicU64 = AtomicU64::new(0);

/// PREV 翻转周期（步数）：WINDOW_SECS / cron 间隔(60s) = 5。
const WINDOW_STEPS: u64 = WINDOW_SECS / 60;

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
/// - 每 `WINDOW_STEPS`（5）次 tick 才翻转一次 PREV 快照，使 delta 真正覆盖
///   `WINDOW_SECS`=300s 窗口（cron 间隔 1 分钟 × 5 步），与 payload 声明的
///   window_secs 一致。
pub async fn run(state: &AppState) {
    let (cur_req, cur_5xx) = metrics_counters::snapshot();

    // 冷启动种子化：首次 run() 时 PREV 仍是初始 0，delta 会等于"自启动累计"而非 5 分钟窗口，
    // 低流量启动期偶发 5xx 易误报。首次仅把当前值写入 PREV 后直接返回，不做阈值判定。
    if !INITIALIZED.swap(true, Ordering::Relaxed) {
        PREV_REQUESTS.store(cur_req, Ordering::Relaxed);
        PREV_5XX.store(cur_5xx, Ordering::Relaxed);
        PREV_FLIP_AT.store(now_secs(), Ordering::Relaxed);
        return;
    }

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
                // delta 实际覆盖的窗口长度 = 距上次 PREV 翻转的经过秒数（翻转后第 1 tick≈60s，
                // 第 5 tick≈300s）；封顶到 WINDOW_SECS 并对 flip_at 未种子化（0）的情况兜底。
                let flip_at = PREV_FLIP_AT.load(Ordering::Relaxed);
                let elapsed = if flip_at == 0 {
                    WINDOW_SECS
                } else {
                    now.saturating_sub(flip_at).min(WINDOW_SECS)
                };
                let window_secs = elapsed.max(1);
                tracing::warn!(
                    error_rate = %format!("{:.2}%", error_rate * 100.0),
                    delta_req,
                    delta_5xx,
                    window_secs,
                    "5xx 错误率超过阈值，广播 incident 事件",
                );
                state.broadcast_to_all_sse(SseEvent::Incident {
                    error_rate,
                    window_secs,
                });
            }
        }
    }

    // 仅每 WINDOW_STEPS（5）次 tick 才翻转 PREV，使 delta 真正覆盖 5 分钟窗口，
    // 与 payload 中声明的 window_secs:300 一致（cron 间隔 1 分钟 × 5 步）。
    let tick = TICK_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    if tick % WINDOW_STEPS == 0 {
        PREV_REQUESTS.store(cur_req, Ordering::Relaxed);
        PREV_5XX.store(cur_5xx, Ordering::Relaxed);
        // 记录本次翻转时刻，使下一窗口内 incident 上报的 window_secs 反映真实经过时长。
        PREV_FLIP_AT.store(now_secs(), Ordering::Relaxed);
    }
}

#[cfg(test)]
// 测试用 std::sync::Mutex 串行化访问 LAST_INCIDENT_AT 等全局静态计数器。
// 持锁穿越 await 在 #[tokio::test] 默认单线程 runtime 下不会死锁；改用
// tokio::sync::Mutex 会引入 #[tokio::test(flavor="multi_thread")] 需求与
// 不必要的 async overhead，得不偿失。
#[allow(clippy::await_holding_lock)]
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
        // flip_at=0：未种子化兜底，使 incident 上报 window_secs 回退为 WINDOW_SECS，
        // 既有断言（window_secs == WINDOW_SECS）继续成立。
        PREV_FLIP_AT.store(0, Ordering::SeqCst);
        TICK_COUNT.store(0, Ordering::SeqCst);
        LAST_INCIDENT_AT.store(last_at, Ordering::SeqCst);
        // 既有用例语义是 PREV 已被填充（非冷启动），故默认置 INITIALIZED=true，
        // 让被调用的 run() 直接做 delta/阈值判定。冷启动专项用例单独清此标志。
        INITIALIZED.store(true, Ordering::SeqCst);
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
        let (tx, _rx) = tokio::sync::mpsc::channel(crate::state::SSE_CONN_CHANNEL_CAP);
        state
            .active_sse()
            .entry("dev-1".into())
            .or_default()
            .push(crate::state::SseClientInfo {
                conn_id: "test-conn".into(),
                user_id: "u1".into(),
                platform: "test".into(),
                connected_at: std::time::Instant::now(),
                tx,
            });

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
        let (tx, mut rx) = tokio::sync::mpsc::channel::<crate::state::SseEvent>(crate::state::SSE_CONN_CHANNEL_CAP);
        state
            .active_sse()
            .entry("dev-2".into())
            .or_default()
            .push(crate::state::SseClientInfo {
                conn_id: "conn-2".into(),
                user_id: "u2".into(),
                platform: "test".into(),
                connected_at: std::time::Instant::now(),
                tx,
            });

        run(&state).await;

        // 应收到 Incident 事件
        let event = rx.try_recv().expect("incident event should be broadcast");
        match event {
            crate::state::SseEvent::Incident {
                error_rate,
                window_secs,
            } => {
                assert!(
                    (error_rate - 0.3).abs() < 1e-9,
                    "error_rate mismatch: {error_rate}"
                );
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
        let (tx, mut rx) = tokio::sync::mpsc::channel::<crate::state::SseEvent>(crate::state::SSE_CONN_CHANNEL_CAP);
        state
            .active_sse()
            .entry("dev-3".into())
            .or_default()
            .push(crate::state::SseClientInfo {
                conn_id: "conn-3".into(),
                user_id: "u3".into(),
                platform: "test".into(),
                connected_at: std::time::Instant::now(),
                tx,
            });

        run(&state).await;

        // 去重静默：不应有新事件推送
        assert!(
            rx.try_recv().is_err(),
            "no incident should be broadcast during dedup window"
        );
        // 去重时间戳不应变化
        assert_eq!(LAST_INCIDENT_AT.load(Ordering::SeqCst), recent);
    }

    /// 冷启动路径：首次 run() 即便累计错误率超阈值也只种子化 PREV、不告警，
    /// 避免低流量启动期偶发 5xx 引发的 boot-time 误报。
    #[tokio::test]
    async fn cold_start_seeds_prev_without_incident() {
        let _guard = TEST_MUTEX.lock().unwrap();
        // req=100, 5xx=30 → 30% > 1%，但冷启动应被抑制
        reset_statics(100, 30, 0, 0, 0);
        // 模拟真实冷启动：PREV 未被任何真实快照填充
        INITIALIZED.store(false, Ordering::SeqCst);

        let state = make_state();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<crate::state::SseEvent>(
            crate::state::SSE_CONN_CHANNEL_CAP,
        );
        state
            .active_sse()
            .entry("dev-cold".into())
            .or_default()
            .push(crate::state::SseClientInfo {
                conn_id: "conn-cold".into(),
                user_id: "u-cold".into(),
                platform: "test".into(),
                connected_at: std::time::Instant::now(),
                tx,
            });

        run(&state).await;

        // 首次 run 不应广播任何事件
        assert!(
            rx.try_recv().is_err(),
            "cold start must not broadcast incident"
        );
        // 去重时间戳应保持 0（未告警）
        assert_eq!(LAST_INCIDENT_AT.load(Ordering::SeqCst), 0);
        // PREV 已被种子化为当前值
        assert_eq!(PREV_REQUESTS.load(Ordering::SeqCst), 100);
        assert_eq!(PREV_5XX.load(Ordering::SeqCst), 30);
        // INITIALIZED 已翻转
        assert!(INITIALIZED.load(Ordering::SeqCst));
    }
}
