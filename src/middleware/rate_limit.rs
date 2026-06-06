use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::extract::{ConnectInfo, Request, State};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::net::SocketAddr;
use tokio::sync::{broadcast, Mutex};

use crate::auth::{extract_token_from_headers, verify_jwt};
use crate::response::{AppError, ErrorBody};
use crate::state::AppState;

const NUM_SHARDS: usize = 16;

/// v1.1-P2.3：限流 key 前缀。
/// - `ip:<addr>` 用于匿名/未鉴权请求
/// - `u:<user_id>` 用于已登录请求（按 sub 限流，绕开同 IP 多用户互踩）
const KEY_PREFIX_IP: &str = "ip:";
const KEY_PREFIX_USER: &str = "u:";
/// `a:<admin_id>` 用于 admin 贵操作端点的 per-admin 限流（与 user 命名空间隔离）。
const KEY_PREFIX_ADMIN: &str = "a:";

#[derive(Debug, Clone)]
struct WindowEntry {
    count: u64,
    window_start: Instant,
}

#[derive(Debug)]
struct Shard {
    map: Mutex<HashMap<String, WindowEntry>>,
}

#[derive(Debug)]
pub struct RateLimiter {
    window_secs: u64,
    /// 默认（fallback）配额，当 `check` 不显式传 max_requests 时使用。
    max_requests: u64,
    shards: Vec<Shard>,
}

fn shard_index_for_key(key: &str) -> usize {
    // 简单 FNV-1a，避免引入 hasher 依赖。
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in key.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    (hash as usize) % NUM_SHARDS
}

/// v1.0 兼容入口：旧测试按 IpAddr 推导 shard 时复用。
#[cfg(test)]
fn shard_index(ip: &IpAddr) -> usize {
    shard_index_for_key(&ip_key(*ip))
}

#[derive(Debug, Clone)]
pub struct RateLimitResult {
    pub allowed: bool,
    pub limit: u64,
    pub remaining: u64,
    pub reset_at: u64,
}

impl RateLimiter {
    pub fn new(window_secs: u64, max_requests: u64) -> Self {
        let shards = (0..NUM_SHARDS)
            .map(|_| Shard {
                map: Mutex::new(HashMap::new()),
            })
            .collect();
        Self {
            window_secs,
            max_requests,
            shards,
        }
    }

    /// 兼容旧调用：按 IP 限流，使用构造时的默认 max_requests。
    pub async fn check(&self, ip: IpAddr, max_entries: usize) -> RateLimitResult {
        let key = ip_key(ip);
        self.check_with_max(&key, max_entries, self.max_requests)
            .await
    }

    /// v1.1-P2.3：泛化 key + 显式 max_requests，支持匿名 IP / 已登录 user_id 双轨。
    pub async fn check_with_max(
        &self,
        key: &str,
        max_entries: usize,
        max_requests: u64,
    ) -> RateLimitResult {
        let now = Instant::now();
        let shard = &self.shards[shard_index_for_key(key)];
        let mut map = shard.map.lock().await;
        let per_shard_max = max_entries / NUM_SHARDS + 1;

        if map.len() >= per_shard_max && !map.contains_key(key) {
            map.retain(|_, v| now.duration_since(v.window_start).as_secs() < self.window_secs);
            if map.len() >= per_shard_max {
                return RateLimitResult {
                    allowed: false,
                    limit: max_requests,
                    remaining: 0,
                    reset_at: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                        + self.window_secs,
                };
            }
        }

        let entry = map.entry(key.to_string()).or_insert(WindowEntry {
            count: 0,
            window_start: now,
        });

        if now.duration_since(entry.window_start).as_secs() >= self.window_secs {
            entry.count = 0;
            entry.window_start = now;
        }

        let allowed = entry.count < max_requests;
        if allowed {
            entry.count += 1;
        }

        let remaining = max_requests.saturating_sub(entry.count);
        let elapsed = now.duration_since(entry.window_start).as_secs();
        let reset_after = self.window_secs.saturating_sub(elapsed);
        let reset_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            + reset_after;

        RateLimitResult {
            allowed,
            limit: max_requests,
            remaining,
            reset_at,
        }
    }

    pub async fn cleanup(&self) {
        let now = Instant::now();
        for shard in &self.shards {
            let mut map = shard.map.lock().await;
            map.retain(|_, value| {
                now.duration_since(value.window_start).as_secs() <= self.window_secs * 2
            });
        }
    }
}

/// 由 IpAddr 构造限流 key：IPv6 先按 /64 前缀聚合，避免 /64 内随机段绕过。
fn ip_key(ip: IpAddr) -> String {
    let normalized = normalize_ip_for_rate_limit(ip);
    format!("{KEY_PREFIX_IP}{normalized}")
}

fn user_key(user_id: &str) -> String {
    format!("{KEY_PREFIX_USER}{user_id}")
}

fn admin_key(admin_id: &str) -> String {
    format!("{KEY_PREFIX_ADMIN}{admin_id}")
}

fn normalize_ip_for_rate_limit(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(_) => ip,
        IpAddr::V6(v6) => {
            let segments = v6.segments();
            let normalized = std::net::Ipv6Addr::new(
                segments[0],
                segments[1],
                segments[2],
                segments[3],
                0,
                0,
                0,
                0,
            );
            IpAddr::V6(normalized)
        }
    }
}

#[derive(Debug)]
pub struct RateLimitState {
    pub limiter: RateLimiter,
}

impl RateLimitState {
    pub fn new(window_secs: u64, max_requests: u64) -> Self {
        Self {
            limiter: RateLimiter::new(window_secs, max_requests),
        }
    }
}

pub async fn rate_limit_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let raw_path = req.uri().path().to_string();
    let path = normalize_api_path(&raw_path);

    if !path.starts_with("/api/") {
        return Ok(next.run(req).await);
    }

    // 管理员后台原则上不走全局限流；其认证子树（/api/admin/auth/*）由
    // auth_rate_limit_middleware 单独按 IP 限流（见 routes/mod.rs）。但 admin
    // 业务路由里存在贵操作端点（尤其 AMAS 的 approve-all/advisor-run 等会产生
    // 500 倍写放大），须对这类写放大端点叠加 per-admin 限流，廉价只读端点仍放行。
    if path.starts_with("/api/admin/") {
        if path.starts_with("/api/admin/auth/") || !is_expensive_admin_endpoint(req.method(), &path)
        {
            return Ok(next.run(req).await);
        }

        let connect_ip = req
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0.ip());
        let ip = extract_client_ip(req.headers(), state.config().trust_proxy, connect_ip);
        let max_entries = state.config().limits.rate_limit_max_entries;
        let cfg = state.config();
        // 按 admin JWT sub 限流（隔离同 IP 多 admin），验签失败/缺失回退按 IP。
        // 配额复用 authenticated 档：贵写操作天然低频，够用且不误伤正常运维。
        let key = match resolve_admin_from_request(req.headers(), &cfg.admin_jwt_secret) {
            Some(admin_id) => admin_key(&admin_id),
            None => ip_key(ip),
        };
        let max_requests = cfg.rate_limit.authenticated_max_requests();

        let result = state
            .rate_limit()
            .limiter
            .check_with_max(&key, max_entries, max_requests)
            .await;

        if !result.allowed {
            let mut response = (
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                Json(ErrorBody {
                    success: false,
                    code: "RATE_LIMITED".to_string(),
                    message: "请求过于频繁".to_string(),
                    trace_id: None,
                }),
            )
                .into_response();
            apply_rate_limit_headers(&mut response, &result);
            if let Ok(v) = cfg.rate_limit.window_secs.to_string().parse() {
                response.headers_mut().insert("retry-after", v);
            }
            return Ok(response);
        }

        let mut response = next.run(req).await;
        apply_rate_limit_headers(&mut response, &result);
        return Ok(response);
    }

    let connect_ip = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip());
    let ip = extract_client_ip(req.headers(), state.config().trust_proxy, connect_ip);
    let max_entries = state.config().limits.rate_limit_max_entries;

    // v1.1-P2.3：双轨限流——根据 Authorization 是否解析出有效用户 JWT
    // 选择"已登录 user_id 限流"或"匿名 IP 限流"；JWT 验签失败 / 缺失皆按匿名处理。
    let cfg = state.config();
    let (key, max_requests) = match resolve_user_from_request(req.headers(), &cfg.jwt_secret) {
        Some(user_id) => (
            user_key(&user_id),
            cfg.rate_limit.authenticated_max_requests(),
        ),
        None => (ip_key(ip), cfg.rate_limit.anonymous_max_requests()),
    };

    let result = state
        .rate_limit()
        .limiter
        .check_with_max(&key, max_entries, max_requests)
        .await;

    if !result.allowed {
        let mut response = (
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            Json(ErrorBody {
                success: false,
                code: "RATE_LIMITED".to_string(),
                message: "请求过于频繁".to_string(),
                trace_id: None,
            }),
        )
            .into_response();

        apply_rate_limit_headers(&mut response, &result);
        if let Ok(v) = state.config().rate_limit.window_secs.to_string().parse() {
            response.headers_mut().insert("retry-after", v);
        }
        return Ok(response);
    }

    let mut response = next.run(req).await;
    apply_rate_limit_headers(&mut response, &result);
    Ok(response)
}

/// 仅靠 JWT 验签提取 user_id：不查 DB / 不查 session，性能 µs 级。
/// - token 缺失、解析失败、token_type 非 "user" → None（按匿名处理）
/// - 已登录会话由后续 AuthUser extractor 在 handler 层再做完整校验
fn resolve_user_from_request(headers: &HeaderMap, jwt_secret: &str) -> Option<String> {
    let token = extract_token_from_headers(headers).ok()?;
    let claims = verify_jwt(&token, jwt_secret).ok()?;
    if claims.token_type != "user" {
        return None;
    }
    Some(claims.sub)
}

/// 仅靠 admin JWT 验签提取 admin_id：token 缺失/验签失败/token_type 非 "admin" → None。
/// admin token 用独立的 `admin_jwt_secret` 签发，故此处不能复用 user 的 jwt_secret。
fn resolve_admin_from_request(headers: &HeaderMap, admin_jwt_secret: &str) -> Option<String> {
    let token = extract_token_from_headers(headers).ok()?;
    let claims = verify_jwt(&token, admin_jwt_secret).ok()?;
    if claims.token_type != "admin" {
        return None;
    }
    Some(claims.sub)
}

/// 判定 admin 端点是否属于「贵/写放大」类，需叠加限流。
/// 命中条件（满足其一）：
///   - 任意写方法（POST/PUT/PATCH/DELETE）落在 `/api/admin/amas/`：AMAS 配置写均会
///     写 amas_config.toml + 插 amas_config_versions，approve-all 更是 500 倍放大；
///   - AMAS 重聚合只读端点（metrics/* 等）：每请求做一次 KPI/切片聚合，可被高频压垮。
/// 廉价 admin 只读端点（看板高频 GET）不命中，避免误限导致后台不可用。
fn is_expensive_admin_endpoint(method: &axum::http::Method, path: &str) -> bool {
    if !path.starts_with("/api/admin/amas/") {
        return false;
    }
    let is_write = matches!(
        *method,
        axum::http::Method::POST
            | axum::http::Method::PUT
            | axum::http::Method::PATCH
            | axum::http::Method::DELETE
    );
    if is_write {
        return true;
    }
    // 重聚合只读端点：metrics_*、user-state 分布/聚类、anomalies feed 等。
    path.starts_with("/api/admin/amas/metrics")
        || path.starts_with("/api/admin/amas/user-state/")
        || path.starts_with("/api/admin/amas/anomalies")
        || path.starts_with("/api/admin/amas/config/diff-impact")
}

fn normalize_api_path(raw_path: &str) -> String {
    if raw_path.starts_with("/api/") {
        raw_path.to_string()
    } else {
        format!("/api{raw_path}")
    }
}

fn apply_rate_limit_headers(response: &mut Response, result: &RateLimitResult) {
    if let Ok(v) = result.limit.to_string().parse() {
        response.headers_mut().insert("ratelimit-limit", v);
    }
    if let Ok(v) = result.remaining.to_string().parse() {
        response.headers_mut().insert("ratelimit-remaining", v);
    }
    if let Ok(v) = result.reset_at.to_string().parse() {
        response.headers_mut().insert("ratelimit-reset", v);
    }
}

pub fn extract_client_ip(
    headers: &HeaderMap,
    trust_proxy: bool,
    connect_ip: Option<IpAddr>,
) -> IpAddr {
    if trust_proxy {
        // 优先取 nginx 设置的 X-Real-IP（=$remote_addr，客户端不可伪造）。
        if let Some(real_ip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
            if let Ok(ip) = real_ip.parse::<IpAddr>() {
                return ip;
            }
        }
        // 回退到 XFF：已知 1 层可信代理下取最右(最后)段，即代理追加的真实
        // remote_addr；取最左段会被客户端注入的伪造首值绕过。
        if let Some(forwarded) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            if let Some(last) = forwarded.split(',').next_back() {
                if let Ok(ip) = last.trim().parse() {
                    return ip;
                }
            }
        }
    }

    connect_ip.unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
}

pub async fn rate_limit_cleanup_loop(
    limiter: Arc<RateLimitState>,
    cleanup_interval_secs: u64,
    mut shutdown_rx: broadcast::Receiver<()>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(cleanup_interval_secs));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                limiter.limiter.cleanup().await;
            }
            _ = shutdown_rx.recv() => break,
        }
    }
}

pub async fn auth_rate_limit_cleanup_loop(
    limiter: Arc<AuthRateLimitState>,
    cleanup_interval_secs: u64,
    mut shutdown_rx: broadcast::Receiver<()>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(cleanup_interval_secs));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                limiter.limiter.cleanup().await;
            }
            _ = shutdown_rx.recv() => break,
        }
    }
}

#[derive(Debug)]
pub struct AuthRateLimitState {
    pub limiter: RateLimiter,
}

impl AuthRateLimitState {
    pub fn new(window_secs: u64, max_requests: u64) -> Self {
        Self {
            limiter: RateLimiter::new(window_secs, max_requests),
        }
    }
}

pub async fn auth_rate_limit_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let connect_ip = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip());
    let ip = extract_client_ip(req.headers(), state.config().trust_proxy, connect_ip);
    let max_entries = state.config().limits.rate_limit_max_entries;
    // 认证端点（登录/注册）天然是匿名前置场景，永远按 IP 限流。
    let result = state.auth_rate_limit().limiter.check(ip, max_entries).await;

    if !result.allowed {
        let mut response = (
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            Json(ErrorBody {
                success: false,
                code: "AUTH_RATE_LIMITED".to_string(),
                message: "认证尝试次数过多，请稍后再试".to_string(),
                trace_id: None,
            }),
        )
            .into_response();

        apply_rate_limit_headers(&mut response, &result);
        if let Ok(v) = state
            .config()
            .auth_rate_limit
            .window_secs
            .to_string()
            .parse()
        {
            response.headers_mut().insert("retry-after", v);
        }
        return Ok(response);
    }

    let mut response = next.run(req).await;
    apply_rate_limit_headers(&mut response, &result);
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn within_limit_is_allowed() {
        let limiter = RateLimiter::new(60, 2);
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        assert!(limiter.check(ip, 100_000).await.allowed);
        assert!(limiter.check(ip, 100_000).await.allowed);
        assert!(!limiter.check(ip, 100_000).await.allowed);
    }

    #[test]
    fn extract_ip_fallbacks() {
        let headers = HeaderMap::new();
        let ip = extract_client_ip(&headers, false, None);
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::LOCALHOST));
    }

    #[test]
    fn extract_ip_uses_connect_ip() {
        let headers = HeaderMap::new();
        let connect = Some(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)));
        let ip = extract_client_ip(&headers, false, connect);
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)));
    }

    #[test]
    fn untrusted_proxy_ignores_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "1.2.3.4".parse().unwrap());
        let ip = extract_client_ip(&headers, false, None);
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::LOCALHOST));
    }

    #[test]
    fn trusted_proxy_reads_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "1.2.3.4".parse().unwrap());
        let ip = extract_client_ip(&headers, true, None);
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)));
    }

    #[test]
    fn path_normalization_works() {
        assert_eq!(normalize_api_path("/api/users/me"), "/api/users/me");
        assert_eq!(normalize_api_path("/users/me"), "/api/users/me");
    }

    #[test]
    fn ipv6_aggregated_by_64_prefix() {
        let ip1: IpAddr = "2001:db8:85a3::1".parse().unwrap();
        let ip2: IpAddr = "2001:db8:85a3::ffff".parse().unwrap();
        let norm1 = normalize_ip_for_rate_limit(ip1);
        let norm2 = normalize_ip_for_rate_limit(ip2);
        assert_eq!(norm1, norm2);
    }

    #[test]
    fn expensive_admin_endpoint_classification() {
        use axum::http::Method;
        // 写方法落在 amas/ → 贵端点。
        assert!(is_expensive_admin_endpoint(
            &Method::POST,
            "/api/admin/amas/suggestions/approve-all"
        ));
        assert!(is_expensive_admin_endpoint(
            &Method::PUT,
            "/api/admin/amas/config"
        ));
        // amas 重聚合只读端点 → 贵端点。
        assert!(is_expensive_admin_endpoint(
            &Method::GET,
            "/api/admin/amas/metrics/kpi"
        ));
        assert!(is_expensive_admin_endpoint(
            &Method::GET,
            "/api/admin/amas/user-state/distribution"
        ));
        // amas 廉价只读（如 /config GET、/state）不命中。
        assert!(!is_expensive_admin_endpoint(
            &Method::GET,
            "/api/admin/amas/state"
        ));
        // 非 amas 的 admin 端点不命中。
        assert!(!is_expensive_admin_endpoint(&Method::POST, "/api/admin/x"));
    }

    #[test]
    fn ipv4_not_aggregated() {
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        assert_eq!(normalize_ip_for_rate_limit(ip), ip);
    }

    use crate::amas::config::AMASConfig;
    use crate::amas::engine::AMASEngine;
    use crate::config::Config;
    use crate::store::Store;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use axum::Router;
    use std::sync::Arc as StdArc;
    use tokio::sync::broadcast;
    use tower::ServiceExt;

    fn ensure_secrets() {
        let secret = "test_secret_that_is_at_least_32_characters_long_ok";
        std::env::set_var("JWT_SECRET", secret);
        std::env::set_var("ADMIN_JWT_SECRET", secret);
        std::env::set_var("REFRESH_JWT_SECRET", secret);
    }

    async fn build_state(api_window: u64, api_max: u64) -> (AppState, tempfile::TempDir) {
        ensure_secrets();
        let mut cfg = Config::from_env();
        cfg.rate_limit.window_secs = api_window;
        cfg.rate_limit.max_requests = api_max;
        // 让 anonymous/authenticated fallback 到 api_max，保留旧用例语义。
        cfg.rate_limit.anonymous_max_requests = 0;
        cfg.rate_limit.authenticated_max_requests = 0;
        cfg.auth_rate_limit.window_secs = api_window;
        cfg.auth_rate_limit.max_requests = api_max;
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = StdArc::new(
            Store::open(tmp.path().join("rate_mw.db").to_str().unwrap(), 5000, 4).unwrap(),
        );
        store.run_migrations().unwrap();
        let amas = StdArc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
        let (tx, _) = broadcast::channel(4);
        let state = AppState::new(store, amas, &cfg, tx, false);
        (state, tmp)
    }

    fn build_router(state: AppState) -> Router {
        Router::new()
            .route("/api/users/me", get(|| async { "ok" }))
            .route("/api/admin/x", get(|| async { "admin-ok" }))
            .route(
                "/api/admin/amas/suggestions/approve-all",
                axum::routing::post(|| async { "approved" }),
            )
            .route("/non-api", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                rate_limit_middleware,
            ))
            .with_state(state)
    }

    fn build_auth_router(state: AppState) -> Router {
        Router::new()
            .route("/api/auth/login", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                auth_rate_limit_middleware,
            ))
            .with_state(state)
    }

    #[tokio::test]
    async fn limiter_check_allows_then_denies_after_max() {
        let limiter = RateLimiter::new(60, 3);
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        for _ in 0..3 {
            assert!(limiter.check(ip, 100_000).await.allowed);
        }
        let denied = limiter.check(ip, 100_000).await;
        assert!(!denied.allowed);
        assert_eq!(denied.remaining, 0);
        assert_eq!(denied.limit, 3);
    }

    #[tokio::test]
    async fn limiter_resets_after_window_elapsed() {
        let limiter = RateLimiter::new(1, 1);
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        assert!(limiter.check(ip, 100_000).await.allowed);
        assert!(!limiter.check(ip, 100_000).await.allowed);
        // 等待 window 过期
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        assert!(limiter.check(ip, 100_000).await.allowed);
    }

    #[tokio::test]
    async fn limiter_max_entries_evicts_and_rejects_new_ip() {
        // 极小容量；NUM_SHARDS=16，每 shard 容量 = max_entries/16 + 1
        let limiter = RateLimiter::new(60, 1);
        // 让某 shard 满
        let ip0 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        // 选择落在同一 shard 的 IPs（通过 shard_index 推导）
        let target_shard = shard_index(&ip0);
        let mut fills: Vec<IpAddr> = Vec::new();
        let mut octet = 0u8;
        while fills.len() < 2 {
            let cand = IpAddr::V4(Ipv4Addr::new(10, 0, 0, octet));
            if shard_index(&cand) == target_shard {
                fills.push(cand);
            }
            if octet == 255 {
                break;
            }
            octet += 1;
        }
        // 用 max_entries=1 → per_shard_max=1+1=2；先填满 2 个 IP
        for ip in &fills {
            limiter.check(*ip, 1).await;
        }
        // 第三个新 IP（同 shard）应被拒
        let mut octet2 = octet;
        let mut new_ip = None;
        for o in 0..=255_u16 {
            let candidate = IpAddr::V4(Ipv4Addr::new(10, 0, 0, o as u8));
            if shard_index(&candidate) == target_shard && !fills.contains(&candidate) {
                new_ip = Some(candidate);
                break;
            }
            octet2 = o as u8;
        }
        let _ = octet2;
        if let Some(ip) = new_ip {
            let result = limiter.check(ip, 1).await;
            assert!(!result.allowed, "expected rejection when shard is full");
        }
    }

    #[tokio::test]
    async fn limiter_cleanup_removes_old_entries() {
        let limiter = RateLimiter::new(1, 5);
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        limiter.check(ip, 100_000).await;
        // 等到 entries 过期（>2*window）
        tokio::time::sleep(std::time::Duration::from_millis(2100)).await;
        limiter.cleanup().await;
        // cleanup 后再次 check 应是新窗口
        let result = limiter.check(ip, 100_000).await;
        assert_eq!(result.remaining, 4);
    }

    #[tokio::test]
    async fn ipv6_shard_index_uses_segments() {
        let v6: IpAddr = "2001:db8::1".parse().unwrap();
        let _ = shard_index(&v6); // 仅验证不 panic
    }

    #[tokio::test]
    async fn extract_client_ip_xff_takes_rightmost_hop_when_trusted() {
        // 1 层可信代理：取最右段（代理追加的真实 remote_addr），
        // 防客户端注入伪造首值绕过限流。
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "5.5.5.5, 9.9.9.9".parse().unwrap());
        let ip = extract_client_ip(&h, true, None);
        assert_eq!(ip, "9.9.9.9".parse::<IpAddr>().unwrap());
    }

    #[tokio::test]
    async fn extract_client_ip_falls_back_to_real_ip_when_xff_invalid() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "garbage".parse().unwrap());
        h.insert("x-real-ip", "8.8.8.8".parse().unwrap());
        let ip = extract_client_ip(&h, true, Some("1.1.1.1".parse().unwrap()));
        assert_eq!(ip, "8.8.8.8".parse::<IpAddr>().unwrap());
    }

    #[tokio::test]
    async fn middleware_normalizes_non_api_path_into_api_prefix() {
        // normalize_api_path 把 /non-api → /api/non-api，因此仍走限流。
        // 验证：max=10 时连发 5 次都能通过（说明限流被实际应用，但容量充足）。
        let (state, _tmp) = build_state(60, 10).await;
        let app = build_router(state);
        for _ in 0..5 {
            let req = Request::builder()
                .uri("/non-api")
                .body(Body::empty())
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn middleware_admin_path_bypasses_check() {
        let (state, _tmp) = build_state(60, 1).await;
        let app = build_router(state);
        for _ in 0..5 {
            let req = Request::builder()
                .uri("/api/admin/x")
                .body(Body::empty())
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn middleware_throttles_expensive_amas_admin_endpoint() {
        // approve-all 是写放大贵端点：max=1 时第 2 次应被 429（不再无限放行）。
        let (state, _tmp) = build_state(60, 1).await;
        let app = build_router(state);
        let mk = || {
            Request::builder()
                .method("POST")
                .uri("/api/admin/amas/suggestions/approve-all")
                .body(Body::empty())
                .unwrap()
        };
        let resp1 = app.clone().oneshot(mk()).await.unwrap();
        assert_eq!(resp1.status(), StatusCode::OK);
        let resp2 = app.oneshot(mk()).await.unwrap();
        assert_eq!(resp2.status(), StatusCode::TOO_MANY_REQUESTS);
        let body = axum::body::to_bytes(resp2.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "RATE_LIMITED");
    }

    #[tokio::test]
    async fn middleware_cheap_admin_endpoint_still_bypasses() {
        // 廉价 admin 只读端点（非 amas 贵端点）仍不限流：max=1 也能连发 5 次。
        let (state, _tmp) = build_state(60, 1).await;
        let app = build_router(state);
        for _ in 0..5 {
            let req = Request::builder()
                .uri("/api/admin/x")
                .body(Body::empty())
                .unwrap();
            let resp = app.clone().oneshot(req).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn middleware_returns_429_with_headers_when_over_limit() {
        let (state, _tmp) = build_state(60, 1).await;
        let app = build_router(state);
        // 第一次：通过
        let req1 = Request::builder()
            .uri("/api/users/me")
            .body(Body::empty())
            .unwrap();
        let resp1 = app.clone().oneshot(req1).await.unwrap();
        assert_eq!(resp1.status(), StatusCode::OK);
        assert!(resp1.headers().get("ratelimit-limit").is_some());

        // 第二次：超限
        let req2 = Request::builder()
            .uri("/api/users/me")
            .body(Body::empty())
            .unwrap();
        let resp2 = app.oneshot(req2).await.unwrap();
        assert_eq!(resp2.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(resp2.headers().get("retry-after").is_some());
        let body = axum::body::to_bytes(resp2.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "RATE_LIMITED");
    }

    #[tokio::test]
    async fn auth_middleware_returns_auth_rate_limited_on_excess() {
        let (state, _tmp) = build_state(60, 1).await;
        let app = build_auth_router(state);
        let req1 = Request::builder()
            .uri("/api/auth/login")
            .body(Body::empty())
            .unwrap();
        let resp1 = app.clone().oneshot(req1).await.unwrap();
        assert_eq!(resp1.status(), StatusCode::OK);

        let req2 = Request::builder()
            .uri("/api/auth/login")
            .body(Body::empty())
            .unwrap();
        let resp2 = app.oneshot(req2).await.unwrap();
        assert_eq!(resp2.status(), StatusCode::TOO_MANY_REQUESTS);
        let body = axum::body::to_bytes(resp2.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "AUTH_RATE_LIMITED");
    }

    #[tokio::test]
    async fn rate_limit_cleanup_loop_terminates_on_shutdown() {
        let limiter = StdArc::new(RateLimitState::new(60, 5));
        let (tx, rx) = broadcast::channel(1);
        let handle = tokio::spawn(rate_limit_cleanup_loop(limiter, 60, rx));
        // 立即关闭
        let _ = tx.send(());
        tokio::time::timeout(std::time::Duration::from_secs(2), handle)
            .await
            .expect("cleanup loop should exit on shutdown")
            .expect("join ok");
    }

    #[tokio::test]
    async fn auth_rate_limit_cleanup_loop_terminates_on_shutdown() {
        let limiter = StdArc::new(AuthRateLimitState::new(60, 5));
        let (tx, rx) = broadcast::channel(1);
        let handle = tokio::spawn(auth_rate_limit_cleanup_loop(limiter, 60, rx));
        let _ = tx.send(());
        tokio::time::timeout(std::time::Duration::from_secs(2), handle)
            .await
            .expect("auth cleanup loop should exit on shutdown")
            .expect("join ok");
    }

    #[test]
    fn rate_limit_state_constructors_work() {
        let s = RateLimitState::new(60, 100);
        assert_eq!(s.limiter.window_secs, 60);
        let a = AuthRateLimitState::new(30, 10);
        assert_eq!(a.limiter.window_secs, 30);
    }
}
