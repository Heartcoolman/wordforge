/// 全局 HTTP 请求计数器（M0-P1 + M0-P4）
///
/// 由 request_id_middleware 在每个请求完成后递增。
/// error_rate_watchdog 读取这些计数计算 5xx 错误率，
/// /metrics 端点也暴露这些值。
///
/// 设计：两个 AtomicU64（单调递增，永不归零），watchdog 用两个时间点的差值
/// 计算滚动 5 分钟的 5xx 率，无需复杂的 ring buffer。
use std::sync::atomic::{AtomicU64, Ordering};

/// 所有进入 request_id_middleware 的请求总数（不含健康检查）
pub static HTTP_REQUESTS_TOTAL: AtomicU64 = AtomicU64::new(0);

/// 所有产生 5xx 响应的请求数
pub static HTTP_5XX_TOTAL: AtomicU64 = AtomicU64::new(0);

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
