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

/// 限流命中（返回 429）累计计数，按命中类型拆分（单调递增，永不归零）。
/// 由 rate_limit_middleware / auth_rate_limit_middleware 在各拒绝路径递增，
/// 供 /metrics 或运维面观察各类限流压力来源。不落库。
pub static RATE_LIMIT_HIT_USER: AtomicU64 = AtomicU64::new(0);
pub static RATE_LIMIT_HIT_ANON: AtomicU64 = AtomicU64::new(0);
pub static RATE_LIMIT_HIT_ADMIN: AtomicU64 = AtomicU64::new(0);
pub static RATE_LIMIT_HIT_AUTH_BRUTEFORCE: AtomicU64 = AtomicU64::new(0);

/// 限流命中类型。各拒绝路径据此选择对应计数器。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateLimitKind {
    /// 已登录用户配额拒绝
    User,
    /// 匿名（按 IP）配额拒绝
    Anon,
    /// admin 贵操作端点配额拒绝
    Admin,
    /// 认证端点（登录/注册）暴破限流拒绝
    AuthBruteforce,
}

/// 记录一次限流命中（在各 429 拒绝路径调用）
#[inline]
pub fn incr_rate_limit_hit(kind: RateLimitKind) {
    let counter = match kind {
        RateLimitKind::User => &RATE_LIMIT_HIT_USER,
        RateLimitKind::Anon => &RATE_LIMIT_HIT_ANON,
        RateLimitKind::Admin => &RATE_LIMIT_HIT_ADMIN,
        RateLimitKind::AuthBruteforce => &RATE_LIMIT_HIT_AUTH_BRUTEFORCE,
    };
    counter.fetch_add(1, Ordering::Relaxed);
}

/// 读取限流命中快照：(user, anon, admin, auth_bruteforce)
#[inline]
pub fn rate_limit_hit_snapshot() -> (u64, u64, u64, u64) {
    (
        RATE_LIMIT_HIT_USER.load(Ordering::Relaxed),
        RATE_LIMIT_HIT_ANON.load(Ordering::Relaxed),
        RATE_LIMIT_HIT_ADMIN.load(Ordering::Relaxed),
        RATE_LIMIT_HIT_AUTH_BRUTEFORCE.load(Ordering::Relaxed),
    )
}

/// strict-mode 观测：声明 `x-device-platform: web` 但缺失/非法 `sec-fetch-mode` 的请求
/// 累计数（仅观测不拒绝——该信号曾被用作硬拒条件，误伤不发送 Sec-Fetch-* 的老浏览器；
/// 现降级为计数，供评估真实误伤率与伪造平台头的异常模式）。单调递增，不落库。
pub static STRICT_WEB_CLAIM_WITHOUT_FETCH_METADATA: AtomicU64 = AtomicU64::new(0);

/// 记录一次「声明 web 但缺/非法 sec-fetch-mode」观测命中。
#[inline]
pub fn incr_strict_web_claim_without_fetch_metadata() {
    STRICT_WEB_CLAIM_WITHOUT_FETCH_METADATA.fetch_add(1, Ordering::Relaxed);
}

/// 读取「声明 web 但缺/非法 sec-fetch-mode」累计计数。
#[inline]
pub fn strict_web_claim_without_fetch_metadata() -> u64 {
    STRICT_WEB_CLAIM_WITHOUT_FETCH_METADATA.load(Ordering::Relaxed)
}
