use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::extract::{Request, State};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
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

    pub async fn check(&self, ip: IpAddr) -> RateLimitResult {
        let now = Instant::now();
        let mut map = self.entries.lock().await;

        let entry = map.entry(ip).or_insert(WindowEntry {
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

    let ip = extract_client_ip(req.headers(), state.config().trust_proxy);
    let result = state.rate_limit().limiter.check(ip).await;

    if !result.allowed {
        let mut response = (
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            Json(ErrorBody {
                success: false,
                error: "Too many requests".to_string(),
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

pub fn extract_client_ip(headers: &HeaderMap, trust_proxy: bool) -> IpAddr {
    if trust_proxy {
        if let Some(forwarded) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            if let Some(first) = forwarded.split(',').next() {
                if let Ok(ip) = first.trim().parse() {
                    return ip;
                }
            }
        }
    }

    headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<IpAddr>().ok())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
}

pub async fn rate_limit_cleanup_loop(
    limiter: Arc<RateLimitState>,
    mut shutdown_rx: broadcast::Receiver<()>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(300));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                limiter.limiter.cleanup().await;
            }
            _ = shutdown_rx.recv() => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn within_limit_is_allowed() {
        let limiter = RateLimiter::new(60, 2);
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        assert!(limiter.check(ip).await.allowed);
        assert!(limiter.check(ip).await.allowed);
        assert!(!limiter.check(ip).await.allowed);
    }

    #[test]
    fn extract_ip_fallbacks() {
        let headers = HeaderMap::new();
        let ip = extract_client_ip(&headers, false);
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::LOCALHOST));
    }

    #[test]
    fn path_normalization_works() {
        assert_eq!(normalize_api_path("/api/users/me"), "/api/users/me");
        assert_eq!(normalize_api_path("/users/me"), "/api/users/me");
    }
}
