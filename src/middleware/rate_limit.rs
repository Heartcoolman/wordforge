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

use crate::response::{AppError, ErrorBody};
use crate::state::AppState;

#[derive(Debug, Clone)]
struct WindowEntry {
    count: u64,
    window_start: Instant,
}

#[derive(Debug, Clone)]
pub struct RateLimiter {
    window_secs: u64,
    max_requests: u64,
    entries: Arc<Mutex<HashMap<IpAddr, WindowEntry>>>,
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
        Self {
            window_secs,
            max_requests,
            entries: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn check(&self, ip: IpAddr, max_entries: usize) -> RateLimitResult {
        // IPv6 按 /64 前缀聚合限流，防止攻击者利用大量 IPv6 地址绕过限流
        let key = normalize_ip_for_rate_limit(ip);
        let now = Instant::now();
        let mut map = self.entries.lock().await;

        if map.len() >= max_entries && !map.contains_key(&key) {
            // 清理过期条目
            map.retain(|_, v| now.duration_since(v.window_start).as_secs() < self.window_secs);
            if map.len() >= max_entries {
                return RateLimitResult {
                    allowed: false,
                    limit: self.max_requests,
                    remaining: 0,
                    reset_at: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                        + self.window_secs,
                };
            }
        }

        let entry = map.entry(key).or_insert(WindowEntry {
            count: 0,
            window_start: now,
        });

        if now.duration_since(entry.window_start).as_secs() >= self.window_secs {
            entry.count = 0;
            entry.window_start = now;
        }

        let allowed = entry.count < self.max_requests;
        if allowed {
            entry.count += 1;
        }

        let remaining = self.max_requests.saturating_sub(entry.count);
        let elapsed = now.duration_since(entry.window_start).as_secs();
        let reset_after = self.window_secs.saturating_sub(elapsed);
        let reset_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            + reset_after;

        RateLimitResult {
            allowed,
            limit: self.max_requests,
            remaining,
            reset_at,
        }
    }

    pub async fn cleanup(&self) {
        let now = Instant::now();
        let mut map = self.entries.lock().await;
        map.retain(|_, value| {
            now.duration_since(value.window_start).as_secs() <= self.window_secs * 2
        });
    }
}

/// IPv6 地址按 /64 前缀聚合，防止同一子网内的大量地址绕过限流。
/// IPv4 地址保持不变。
fn normalize_ip_for_rate_limit(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(_) => ip,
        IpAddr::V6(v6) => {
            let segments = v6.segments();
            // 保留前 64 位（前 4 个 segment），后 64 位清零
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

#[derive(Debug, Clone)]
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

    let connect_ip = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip());
    let ip = extract_client_ip(req.headers(), state.config().trust_proxy, connect_ip);
    let max_entries = state.config().limits.rate_limit_max_entries;
    let result = state.rate_limit().limiter.check(ip, max_entries).await;

    if !result.allowed {
        let mut response = (
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            Json(ErrorBody {
                success: false,
                code: "RATE_LIMITED".to_string(),
                message: "Too many requests".to_string(),
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
        if let Some(forwarded) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            if let Some(first) = forwarded.split(',').next() {
                if let Ok(ip) = first.trim().parse() {
                    return ip;
                }
            }
        }
        if let Some(real_ip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
            if let Ok(ip) = real_ip.parse::<IpAddr>() {
                return ip;
            }
        }
    }

    // 使用 TCP 连接的真实 IP
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

/// 认证端点专用速率限制器：每 IP 每分钟 10 次
#[derive(Debug, Clone)]
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
    let result = state.auth_rate_limit().limiter.check(ip, max_entries).await;

    if !result.allowed {
        let mut response = (
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            Json(ErrorBody {
                success: false,
                code: "AUTH_RATE_LIMITED".to_string(),
                message: "Too many authentication attempts. Please try again later.".to_string(),
                trace_id: None,
            }),
        )
            .into_response();

        apply_rate_limit_headers(&mut response, &result);
        if let Ok(v) = state.config().auth_rate_limit.window_secs.to_string().parse() {
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
    fn ipv4_not_aggregated() {
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        assert_eq!(normalize_ip_for_rate_limit(ip), ip);
    }
}
