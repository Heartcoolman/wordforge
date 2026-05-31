//! M0-P1 补充：http_request_duration_seconds histogram middleware。
//!
//! 在每个请求完成后记录延迟，按 (method, route, status_class) 分组。
//! 无外部依赖，手写 OpenMetrics histogram exposition。
//!
//! - route：取 URI path 的第一段（如 `/api/words/123` → `/api/words`），防止
//!   高基数标签爆炸。/metrics 端点本身排除，避免自引用。
//! - status_class：`2xx` / `3xx` / `4xx` / `5xx`（其余归 `other`）。
//! - bucket 边界：0.01 / 0.05 / 0.1 / 0.5 / 2.0 / +Inf（秒）。

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use once_cell::sync::Lazy;
use serde::Serialize;
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

    // 入站请求体字节：用 Content-Length 头估算（HTTP 入站口径，非网卡总量）。
    let bytes_in = req
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    // 排除 /metrics 避免自引用（admin 频繁 scrape 会影响自身 histogram）
    let is_metrics = path == "/metrics";

    let start = Instant::now();
    let response = next.run(req).await;
    let elapsed = start.elapsed().as_secs_f64();

    if !is_metrics {
        let status_code = response.status().as_u16();
        let route = route_prefix(&path);
        let status = status_class(status_code).to_string();
        let key = (method, route, status);

        {
            let mut map = registry().write().await;
            map.entry(key).or_insert_with(HistogramData::new).observe(elapsed);
        }

        // M0-P5：滚动窗口聚合（SLO 卡 / 请求延迟图 / axum 服务行 rps 的数据源）
        record_rolling(elapsed, (500..=599).contains(&status_code), bytes_in);
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

// ═══════════════════════════════════════════════════════════════════
// 滚动窗口聚合器（M0-P5）
//
// 在累积直方图之外，额外维护「每分钟」与「每小时」两级环形缓冲，
// 供 admin 监控页计算窗口内 P50/P99、QPS、错误率、可用性、入站带宽 +
// 下采样时序图。计数随进程生命周期，重启清零 —— 因此「可用性」按实际
// 可得窗口（min(window, 自首条记录以来)）如实标注，不伪造 30 天历史。
// ═══════════════════════════════════════════════════════════════════

const MINUTE_CAP: usize = 24 * 60; // 24h 的每分钟槽
const HOUR_CAP: usize = 7 * 24; // 7d 的每小时槽
const MINUTE_SOURCE_MAX_SECS: u64 = 24 * 3600; // ≤24h 用分钟槽，更长用小时槽

/// 单个时间槽（分钟或小时粒度）的累积量。
#[derive(Clone)]
struct Slot {
    /// 槽键：分钟槽 = unix/60，小时槽 = unix/3600
    key: i64,
    count: u64,
    err5xx: u64,
    bytes_in: u64,
    /// 累积桶（含 +Inf），长度 = BUCKET_BOUNDS.len() + 1
    buckets: Vec<u64>,
}

impl Slot {
    fn new(key: i64) -> Self {
        Self {
            key,
            count: 0,
            err5xx: 0,
            bytes_in: 0,
            buckets: vec![0; BUCKET_BOUNDS.len() + 1],
        }
    }

    fn observe(&mut self, latency_secs: f64, is_5xx: bool, bytes_in: u64) {
        self.count += 1;
        if is_5xx {
            self.err5xx += 1;
        }
        self.bytes_in += bytes_in;
        for (i, &bound) in BUCKET_BOUNDS.iter().enumerate() {
            if latency_secs <= bound {
                self.buckets[i] += 1;
            }
        }
        *self.buckets.last_mut().unwrap() += 1;
    }
}

struct RollingStore {
    minute: VecDeque<Slot>,
    hour: VecDeque<Slot>,
    first_record_unix: Option<i64>,
}

static ROLLING: Lazy<Mutex<RollingStore>> = Lazy::new(|| {
    Mutex::new(RollingStore {
        minute: VecDeque::new(),
        hour: VecDeque::new(),
        first_record_unix: None,
    })
});

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 取/建匹配 key 的尾部槽，并保持容量上限。
fn touch_slot(dq: &mut VecDeque<Slot>, key: i64, cap: usize) -> &mut Slot {
    if dq.back().map(|s| s.key) != Some(key) {
        dq.push_back(Slot::new(key));
        while dq.len() > cap {
            dq.pop_front();
        }
    }
    dq.back_mut().unwrap()
}

fn record_rolling(latency_secs: f64, is_5xx: bool, bytes_in: u64) {
    let now = now_unix();
    let mut store = match ROLLING.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    if store.first_record_unix.is_none() {
        store.first_record_unix = Some(now);
    }
    touch_slot(&mut store.minute, now / 60, MINUTE_CAP).observe(latency_secs, is_5xx, bytes_in);
    touch_slot(&mut store.hour, now / 3600, HOUR_CAP).observe(latency_secs, is_5xx, bytes_in);
}

/// 从累积桶估算分位延迟（毫秒），桶内线性插值。
fn percentile_ms(buckets: &[u64], count: u64, p: f64) -> f64 {
    if count == 0 {
        return 0.0;
    }
    let target = count as f64 * p;
    let mut prev_cum = 0.0f64;
    let mut prev_bound = 0.0f64;
    for (i, &cum) in buckets.iter().enumerate() {
        let cum = cum as f64;
        if cum >= target {
            let bound = if i < BUCKET_BOUNDS.len() {
                BUCKET_BOUNDS[i]
            } else {
                // +Inf 桶：用最大有限边界 ×1.5 作上界估计
                BUCKET_BOUNDS[BUCKET_BOUNDS.len() - 1] * 1.5
            };
            let in_bucket = cum - prev_cum;
            let frac = if in_bucket > 0.0 {
                ((target - prev_cum) / in_bucket).clamp(0.0, 1.0)
            } else {
                0.0
            };
            return (prev_bound + (bound - prev_bound) * frac) * 1000.0;
        }
        prev_cum = cum;
        if i < BUCKET_BOUNDS.len() {
            prev_bound = BUCKET_BOUNDS[i];
        }
    }
    BUCKET_BOUNDS[BUCKET_BOUNDS.len() - 1] * 1000.0
}

/// 时序图单点：unix 秒 + QPS + P99(ms)。前端负责格式化 HH:MM 标签。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestMetricsPoint {
    pub t: i64,
    pub qps: f64,
    pub p99_ms: f64,
}

/// 窗口聚合结果：SLO 卡 + 延迟图 + axum 服务行 rps 的统一数据源。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestMetricsAggregate {
    /// 请求窗口（秒）
    pub window_secs: u64,
    /// 实际可得窗口（秒）= min(window, 自首条记录以来)；可用性据此如实标注
    pub effective_secs: u64,
    pub total_requests: u64,
    pub total_5xx: u64,
    pub qps_avg: f64,
    pub p50_ms: f64,
    pub p99_ms: f64,
    /// 窗口错误率 0.0–1.0
    pub error_rate: f64,
    /// 窗口可用性百分比 = (1 - error_rate) × 100
    pub availability_pct: f64,
    /// 入站带宽（字节/秒，Content-Length 口径）
    pub bandwidth_in_bps: f64,
    pub series: Vec<RequestMetricsPoint>,
}

/// 聚合给定窗口的请求指标。window_secs ≤ 24h 用分钟槽，更长用小时槽。
pub fn aggregate(window_secs: u64) -> RequestMetricsAggregate {
    let now = now_unix();
    let cutoff = now - window_secs as i64;
    let use_minute = window_secs <= MINUTE_SOURCE_MAX_SECS;
    let gran: i64 = if use_minute { 60 } else { 3600 };

    let store = match ROLLING.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let src = if use_minute { &store.minute } else { &store.hour };

    // 窗口内的槽（按时间升序）
    let slots: Vec<&Slot> = src.iter().filter(|s| s.key * gran >= cutoff).collect();

    let mut total = 0u64;
    let mut err = 0u64;
    let mut bytes = 0u64;
    let mut agg_buckets = vec![0u64; BUCKET_BOUNDS.len() + 1];
    for s in &slots {
        total += s.count;
        err += s.err5xx;
        bytes += s.bytes_in;
        for (i, b) in s.buckets.iter().enumerate() {
            agg_buckets[i] += b;
        }
    }

    let effective_secs = match store.first_record_unix {
        Some(first) => (window_secs as i64).min((now - first).max(1)).max(1) as u64,
        None => window_secs.max(1),
    };
    let error_rate = if total > 0 {
        err as f64 / total as f64
    } else {
        0.0
    };

    // 下采样到 ≤60 点
    const TARGET: usize = 60;
    let series = downsample(&slots, gran, TARGET);

    RequestMetricsAggregate {
        window_secs,
        effective_secs,
        total_requests: total,
        total_5xx: err,
        qps_avg: total as f64 / effective_secs as f64,
        p50_ms: percentile_ms(&agg_buckets, total, 0.50),
        p99_ms: percentile_ms(&agg_buckets, total, 0.99),
        error_rate,
        availability_pct: (1.0 - error_rate) * 100.0,
        bandwidth_in_bps: bytes as f64 / effective_secs as f64,
        series,
    }
}

/// 将窗口内的槽分组下采样为 ≤target 个时序点。
fn downsample(slots: &[&Slot], gran: i64, target: usize) -> Vec<RequestMetricsPoint> {
    if slots.is_empty() {
        return Vec::new();
    }
    let group = slots.len().div_ceil(target).max(1);
    let mut out = Vec::new();
    let mut i = 0;
    while i < slots.len() {
        let chunk = &slots[i..(i + group).min(slots.len())];
        let mut count = 0u64;
        let mut buckets = vec![0u64; BUCKET_BOUNDS.len() + 1];
        for s in chunk {
            count += s.count;
            for (j, b) in s.buckets.iter().enumerate() {
                buckets[j] += b;
            }
        }
        let span_secs = (chunk.len() as i64 * gran).max(1) as f64;
        let last_key = chunk.last().unwrap().key;
        out.push(RequestMetricsPoint {
            t: last_key * gran,
            qps: count as f64 / span_secs,
            p99_ms: percentile_ms(&buckets, count, 0.99),
        });
        i += group;
    }
    out
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

    #[test]
    fn percentile_ms_interpolates_within_bucket() {
        // 100 个观测，桶边界 [0.01,0.05,0.1,0.5,2.0]+Inf
        // 累积桶：50 个 ≤0.01，全部 ≤0.05
        let buckets = vec![50, 100, 100, 100, 100, 100];
        // P50 落在第 0 桶上界附近（target=50，cum[0]=50 命中）→ ≤10ms
        let p50 = percentile_ms(&buckets, 100, 0.50);
        assert!(p50 > 0.0 && p50 <= 10.0, "p50={p50}");
        // P99 落在 (0.01,0.05] 桶内 → 10..=50ms
        let p99 = percentile_ms(&buckets, 100, 0.99);
        assert!(p99 > 10.0 && p99 <= 50.0, "p99={p99}");
        // 空数据返回 0
        assert_eq!(percentile_ms(&[0, 0, 0, 0, 0, 0], 0, 0.99), 0.0);
    }

    #[test]
    fn rolling_record_and_aggregate_roundtrip() {
        // 记录若干请求后，1h 窗口应能聚合出非零计数
        for _ in 0..10 {
            record_rolling(0.02, false, 128);
        }
        record_rolling(0.02, true, 64); // 一条 5xx
        let agg = aggregate(3600);
        assert!(agg.total_requests >= 11, "total={}", agg.total_requests);
        assert!(agg.total_5xx >= 1);
        assert!(agg.availability_pct < 100.0);
        assert!(!agg.series.is_empty());
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
