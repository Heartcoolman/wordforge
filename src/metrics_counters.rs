/// 全局 HTTP 请求计数器（M0-P1 + M0-P4）
///
/// 由 request_id_middleware 在每个请求完成后递增。
/// error_rate_watchdog 读取这些计数计算 5xx 错误率，
/// /metrics 端点也暴露这些值。
///
/// 设计：两个 AtomicU64（单调递增，永不归零），watchdog 用两个时间点的差值
/// 计算滚动 5 分钟的 5xx 率，无需复杂的 ring buffer。
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

/// 所有进入 request_id_middleware 的请求总数（不含健康检查）
pub static HTTP_REQUESTS_TOTAL: AtomicU64 = AtomicU64::new(0);

/// 所有产生 5xx 响应的请求数
pub static HTTP_5XX_TOTAL: AtomicU64 = AtomicU64::new(0);

/// 当前在途（正在处理中）的请求数。饱和度信号：CPU 钉死但 5xx=0 时，
/// in-flight 堆积是"过载但无错误"最直接的证据。由 request_id_middleware
/// 用 RAII 守卫维护（进入 +1，返回/panic 时 -1）。
pub static HTTP_INFLIGHT: AtomicI64 = AtomicI64::new(0);

/// in-flight RAII 守卫：构造 +1，Drop -1。保证 early-return / panic 也能正确回收。
pub struct InflightGuard(());

impl InflightGuard {
    #[inline]
    pub fn enter() -> Self {
        HTTP_INFLIGHT.fetch_add(1, Ordering::Relaxed);
        InflightGuard(())
    }
}

impl Drop for InflightGuard {
    #[inline]
    fn drop(&mut self) {
        HTTP_INFLIGHT.fetch_sub(1, Ordering::Relaxed);
    }
}

/// 当前在途请求数（钳到 ≥0）
#[inline]
pub fn inflight() -> i64 {
    HTTP_INFLIGHT.load(Ordering::Relaxed).max(0)
}

/// 记录一次请求（在 request_id_middleware 末尾调用）
#[inline]
pub fn record_request(is_5xx: bool) {
    HTTP_REQUESTS_TOTAL.fetch_add(1, Ordering::Relaxed);
    if is_5xx {
        HTTP_5XX_TOTAL.fetch_add(1, Ordering::Relaxed);
    }
}

/// 读取当前快照（requests, errors_5xx）
#[inline]
pub fn snapshot() -> (u64, u64) {
    (
        HTTP_REQUESTS_TOTAL.load(Ordering::Relaxed),
        HTTP_5XX_TOTAL.load(Ordering::Relaxed),
    )
}
