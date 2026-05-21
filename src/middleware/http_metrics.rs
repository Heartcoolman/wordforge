//! M0-P1 补充：http_request_duration_seconds histogram middleware。
//!
//! 在每个请求完成后记录延迟，按 (method, route, status_class) 分组。
//! 无外部依赖，手写 OpenMetrics histogram exposition。
//!
//! - route：取 URI path 的第一段（如 `/api/words/123` → `/api/words`），防止
//!   高基数标签爆炸。/metrics 端点本身排除，避免自引用。
//! - status_class：`2xx` / `3xx` / `4xx` / `5xx`（其余归 `other`）。
//! - bucket 边界：0.01 / 0.05 / 0.1 / 0.5 / 2.0 / +Inf（秒）。

use std::sync::OnceLock;
use std::time::Instant;

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use tokio::sync::RwLock;

/// Histogram bucket 边界（秒），必须升序
const BUCKET_BOUNDS: &[f64] = &[0.01, 0.05, 0.1, 0.5, 2.0];

/// 单个 (method, route, status_class) 的累积数据
#[derive(Default, Clone)]
pub struct HistogramData {
    /// 每个 bucket 的累积计数（含 +Inf）；长度 = BUCKET_BOUNDS.len() + 1
    pub buckets: Vec<u64>,
    /// 观测次数总计
    pub count: u64,
    /// 所有延迟值的总和（秒）
    pub sum: f64,
}

impl HistogramData {
    fn new() -> Self {
        Self {
            buckets: vec![0; BUCKET_BOUNDS.len() + 1],
            count: 0,
            sum: 0.0,
        }
    }

    fn observe(&mut self, latency_secs: f64) {
        self.count += 1;
        self.sum += latency_secs;
        for (i, &bound) in BUCKET_BOUNDS.iter().enumerate() {
            if latency_secs <= bound {
                self.buckets[i] += 1;
            }
        }
        // +Inf bucket 永远 +1
        *self.buckets.last_mut().unwrap() += 1;
    }
}

/// Key: (HTTP method, route_prefix, status_class)
type HistogramKey = (String, String, String);
type HistogramRegistry = RwLock<std::collections::HashMap<HistogramKey, HistogramData>>;

static REGISTRY: OnceLock<HistogramRegistry> = OnceLock::new();

fn registry() -> &'static HistogramRegistry {
    REGISTRY.get_or_init(|| RwLock::new(std::collections::HashMap::new()))
}

/// 取 URI path 第二段（/api/words/123 → "/api/words"）作为 route label。
/// 直接用完整路径会导致标签基数爆炸（每个 ID 都是新标签），
/// 用前两段可以覆盖绝大多数有意义的路由分组。
fn route_prefix(path: &str) -> String {
    let parts: Vec<&str> = path.splitn(4, '/').collect();
    // parts[0] = ""（leading slash 前）, parts[1] = "api", parts[2] = "words", ...
    match parts.len() {
        0 | 1 => "/".to_string(),
        2 => format!("/{}", parts[1]),
        _ => format!("/{}/{}", parts[1], parts[2]),
    }
}

fn status_class(status: u16) -> &'static str {
    match status {
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        500..=599 => "5xx",
        _ => "other",
    }
}

/// Axum middleware：在每个请求完成后记录延迟到 histogram。
/// /metrics 端点本身会被跳过。
pub async fn record_http_metrics(req: Request, next: Next) -> Response {
    let method = req.method().as_str().to_string();
    let path = req.uri().path().to_string();

    // 排除 /metrics 避免自引用（admin 频繁 scrape 会影响自身 histogram）
    let is_metrics = path == "/metrics";

    let start = Instant::now();
    let response = next.run(req).await;
    let elapsed = start.elapsed().as_secs_f64();

    if !is_metrics {
        let route = route_prefix(&path);
        let status = status_class(response.status().as_u16()).to_string();
        let key = (method, route, status);

        let mut map = registry().write().await;
        map.entry(key).or_insert_with(HistogramData::new).observe(elapsed);
    }

    response
}

/// 获取当前所有 histogram 数据的快照（用于 /metrics 端点输出）。
pub async fn snapshot() -> Vec<(HistogramKey, HistogramData)> {
    let map = registry().read().await;
    map.iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// 以 OpenMetrics text 格式输出 `http_request_duration_seconds` histogram。
pub async fn write_histogram_exposition(out: &mut String) {
    use std::fmt::Write as _;

    let data = snapshot().await;

    let _ = writeln!(out, "# HELP http_request_duration_seconds HTTP 请求延迟直方图（秒），按路由、方法、响应状态分类");
    let _ = writeln!(out, "# TYPE http_request_duration_seconds histogram");

    if data.is_empty() {
        // 无实际数据时输出全零占位行，保证 metric 名称可被 scraper 发现
        for &bound in BUCKET_BOUNDS {
            let _ = writeln!(
                out,
                "http_request_duration_seconds_bucket{{le=\"{bound}\"}} 0"
            );
        }
        let _ = writeln!(out, "http_request_duration_seconds_bucket{{le=\"+Inf\"}} 0");
        let _ = writeln!(out, "http_request_duration_seconds_count 0");
        let _ = writeln!(out, "http_request_duration_seconds_sum 0");
        return;
    }

    for ((method, route, status), hist) in &data {
        let labels_prefix = format!(
            "method=\"{method}\",route=\"{route}\",status=\"{status}\""
        );
        // 输出有限 bucket
        for (i, &bound) in BUCKET_BOUNDS.iter().enumerate() {
            let _ = writeln!(
                out,
                "http_request_duration_seconds_bucket{{{labels_prefix},le=\"{bound}\"}} {}",
                hist.buckets[i]
            );
        }
        // +Inf bucket
        let _ = writeln!(
            out,
            "http_request_duration_seconds_bucket{{{labels_prefix},le=\"+Inf\"}} {}",
            hist.buckets.last().copied().unwrap_or(0)
        );
        let _ = writeln!(
            out,
            "http_request_duration_seconds_count{{{labels_prefix}}} {}",
            hist.count
        );
        let _ = writeln!(
            out,
            "http_request_duration_seconds_sum{{{labels_prefix}}} {}",
            hist.sum
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_prefix_extracts_two_segments() {
        assert_eq!(route_prefix("/api/words/123"), "/api/words");
        assert_eq!(route_prefix("/api/users"), "/api/users");
        assert_eq!(route_prefix("/health"), "/health");
        assert_eq!(route_prefix("/"), "/");
        assert_eq!(route_prefix("/metrics"), "/metrics");
    }

    #[test]
    fn status_class_maps_correctly() {
        assert_eq!(status_class(200), "2xx");
        assert_eq!(status_class(201), "2xx");
        assert_eq!(status_class(400), "4xx");
        assert_eq!(status_class(401), "4xx");
        assert_eq!(status_class(500), "5xx");
        assert_eq!(status_class(302), "3xx");
        assert_eq!(status_class(100), "other");
    }

    #[test]
    fn histogram_data_observe_increments_buckets() {
        let mut h = HistogramData::new();
        // 0.03 秒：落在 le=0.05 及以上所有 bucket
        h.observe(0.03);
        assert_eq!(h.count, 1);
        assert!((h.sum - 0.03).abs() < 1e-10);
        // 0.01 bucket 未命中（0.03 > 0.01）
        assert_eq!(h.buckets[0], 0);
        // 0.05 bucket 命中
        assert_eq!(h.buckets[1], 1);
        // +Inf
        assert_eq!(*h.buckets.last().unwrap(), 1);
    }

    #[tokio::test]
    async fn write_histogram_exposition_empty_still_outputs_help_type() {
        // 强制清空 registry（通过写入然后清除）
        {
            let mut map = registry().write().await;
            map.clear();
        }
        let mut out = String::new();
        write_histogram_exposition(&mut out).await;
        assert!(out.contains("# HELP http_request_duration_seconds"));
        assert!(out.contains("# TYPE http_request_duration_seconds histogram"));
    }
}
