//! M0-P1 补充：http_request_duration_seconds histogram middleware。
//!
//! 在每个请求完成后记录延迟，按 (method, route, status_class) 分组。
//! 无外部依赖，手写 OpenMetrics histogram exposition。
//!
//! - route：取 axum `MatchedPath` 已匹配的路由模板（如 `/api/words/:id`），未匹配
//!   （404 fallback）统一归 `<other>`，从根上封死高基数标签爆炸（请求方无法注入任意
//!   route 段）。叠加 REGISTRY 条目上限作二次兜底。/metrics 端点本身排除，避免自引用。
//! - status_class：`2xx` / `3xx` / `4xx` / `5xx`（其余归 `other`）。
//! - bucket 边界：0.01 / 0.05 / 0.1 / 0.5 / 2.0 / +Inf（秒）。

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use once_cell::sync::Lazy;
use serde::Serialize;
use tokio::sync::RwLock;

/// Histogram bucket 边界（秒），必须升序。
/// W3-3：细化 0.1→0.5→2.0 之间的塌陷区间（原仅 2 桶覆盖 100ms~2s，p95/p99 线性插值严重失真——
/// 而慢请求恰是 SLO 关注点）。改动桶数=直方图 series 断层，见 docs/runbook/metrics-buckets.md。
const BUCKET_BOUNDS: &[f64] = &[0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5];

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

/// Key: (HTTP method, matched route template, status_class)
type HistogramKey = (String, String, String);
type HistogramRegistry = RwLock<std::collections::HashMap<HistogramKey, HistogramData>>;

static REGISTRY: OnceLock<HistogramRegistry> = OnceLock::new();

/// route 标签由 `MatchedPath` 取，未匹配路由统一归此值，封死高基数爆炸。
const OTHER_ROUTE: &str = "<other>";

/// REGISTRY 条目上限（二次兜底）。route 已被 `MatchedPath` 限定为有限路由表，
/// 此上限防御未来路由注册异常等意外把 key 撑爆；超阈值仅更新已有 key、停止新增。
const REGISTRY_MAX_ENTRIES: usize = 4096;

fn registry() -> &'static HistogramRegistry {
    REGISTRY.get_or_init(|| RwLock::new(std::collections::HashMap::new()))
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

/// 记录一次观测到 registry，满容量时仅更新已有 key、拒绝新增（防 OOM 兜底）。
fn observe_capped(
    map: &mut std::collections::HashMap<HistogramKey, HistogramData>,
    key: HistogramKey,
    latency_secs: f64,
    cap: usize,
) {
    if let Some(hist) = map.get_mut(&key) {
        hist.observe(latency_secs);
    } else if map.len() < cap {
        map.entry(key)
            .or_insert_with(HistogramData::new)
            .observe(latency_secs);
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

    // route 标签取已匹配的路由模板（请求方不可控），未匹配（404 fallback）归 <other>，
    // 从根上封死高基数爆炸。须在 next.run 消费 req 前读取。
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| OTHER_ROUTE.to_string());

    let start = Instant::now();
    let response = next.run(req).await;
    let elapsed = start.elapsed().as_secs_f64();

    if !is_metrics {
        let status_code = response.status().as_u16();
        let status = status_class(status_code).to_string();
        let key = (method, route, status);

        {
            let mut map = registry().write().await;
            observe_capped(&mut map, key, elapsed, REGISTRY_MAX_ENTRIES);
        }

        // M0-P5：滚动窗口聚合（SLO 卡 / 请求延迟图 / axum 服务行 rps 的数据源）
        record_rolling(elapsed, (500..=599).contains(&status_code), bytes_in);
    }

    response
}

/// 获取当前所有 histogram 数据的快照（用于 /metrics 端点输出）。
pub async fn snapshot() -> Vec<(HistogramKey, HistogramData)> {
    let map = registry().read().await;
    map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
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
        let labels_prefix = format!("method=\"{method}\",route=\"{route}\",status=\"{status}\"");
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
// 下采样时序图。minute 层随进程生命周期、重启清零；hour 层经 D3 持久化到
// availability_rollup 表并在启动回灌，故「可用性」hour 窗口（≤30d）跨重启可达；
// effective 窗口仍按 min(window, 自首条记录以来) 如实标注，不伪造历史。
// ═══════════════════════════════════════════════════════════════════

const MINUTE_CAP: usize = 24 * 60; // 24h 的每分钟槽
const HOUR_CAP: usize = 30 * 24; // D3:30d 的每小时槽（持久化到 availability_rollup 跨重启）
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
/// 时钟回拨(NTP step / VM 挂起恢复)时 key 可能 < 队尾 key；此时不在尾部压入更小 key（会破坏
/// aggregate/downsample 依赖的升序不变式）：先找已存在的同 key 槽合并，无则按 key 二分插入到
/// 正确的有序位置（而非误并入尾槽），保持队列升序，并把观测落到真正对应当前时刻的槽。
fn touch_slot(dq: &mut VecDeque<Slot>, key: i64, cap: usize) -> &mut Slot {
    match dq.back().map(|s| s.key) {
        Some(back_key) if back_key == key => dq.back_mut().unwrap(),
        Some(back_key) if key < back_key => {
            // 回拨：找已有同 key 槽合并，避免在尾部压入更小 key 破坏升序。
            if let Some(idx) = dq.iter().rposition(|s| s.key == key) {
                dq.get_mut(idx).unwrap()
            } else {
                // 无同 key 槽：按 key 二分定位插入点，插入新槽保持升序，
                // 而非把回拨观测错误地并入最新（尾）槽。
                let idx = dq.partition_point(|s| s.key < key);
                if idx == 0 && dq.len() >= cap {
                    // key 比保留窗口内所有槽都旧，且已满；新建槽会被立刻淘汰，
                    // 退而合并进当前最旧（front）槽，避免无意义的插入/淘汰。
                    dq.front_mut().unwrap()
                } else {
                    dq.insert(idx, Slot::new(key));
                    // idx>=1 时 front 不是新槽，pop_front 安全淘汰最旧槽。
                    while dq.len() > cap {
                        dq.pop_front();
                    }
                    let pos = dq
                        .iter()
                        .position(|s| s.key == key)
                        .expect("inserted slot must exist");
                    dq.get_mut(pos).unwrap()
                }
            }
        }
        _ => {
            dq.push_back(Slot::new(key));
            while dq.len() > cap {
                dq.pop_front();
            }
            dq.back_mut().unwrap()
        }
    }
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

/// D3：导出当前内存 hour 槽，供 metrics_flush 持久化到 availability_rollup。
/// 返回 (hour_key, count, err5xx, bytes_in, buckets)，按内存顺序（升序）。
pub fn export_hour_rollup() -> Vec<(i64, u64, u64, u64, Vec<u64>)> {
    let store = match ROLLING.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    store
        .hour
        .iter()
        .map(|s| (s.key, s.count, s.err5xx, s.bytes_in, s.buckets.clone()))
        .collect()
}

/// D3：启动回灌持久化的 hour 槽，恢复跨重启的 ≤30d 可用率窗口。
/// 仅追加内存中尚不存在的 hour_key（启动期 deque 通常为空），据最早 key 回填
/// first_record_unix，并保证 deque 按 key 升序（aggregate/downsample 不变式）。
pub fn import_hour_rollup(rows: Vec<(i64, u64, u64, u64, Vec<u64>)>) {
    if rows.is_empty() {
        return;
    }
    let want = BUCKET_BOUNDS.len() + 1;
    let mut store = match ROLLING.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    for (key, count, err5xx, bytes_in, buckets) in rows {
        if store.hour.iter().any(|s| s.key == key) {
            continue;
        }
        // W3-3：桶数变更（直方图边界细化）时，旧持久化行的桶向量与新 schema 长度不符——
        // 简单 resize 会把旧 +Inf 计数滞留在错误 index、污染历史小时桶 p99。检测到长度不符即
        // 丢弃该行直方图（置零），但保留 count/err5xx（可用率比率不依赖桶，不受影响），不伪造历史。
        // （未来若出现「桶数不变但边界变」的改动，需改用显式桶版本标记。）
        let buckets = if buckets.len() == want {
            buckets
        } else {
            vec![0u64; want]
        };
        let mut slot = Slot::new(key);
        slot.count = count;
        slot.err5xx = err5xx;
        slot.bytes_in = bytes_in;
        slot.buckets = buckets;
        store.hour.push_back(slot);
        let first = key * 3600;
        store.first_record_unix = Some(match store.first_record_unix {
            Some(f) => f.min(first),
            None => first,
        });
    }
    while store.hour.len() > HOUR_CAP {
        store.hour.pop_front();
    }
    store.hour.make_contiguous().sort_by_key(|s| s.key);
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

/// 时序图单点：unix 秒 + QPS + P50/P95/P99(ms)。前端负责格式化 HH:MM 标签。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestMetricsPoint {
    pub t: i64,
    pub qps: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
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
    pub p95_ms: f64,
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
    let src = if use_minute {
        &store.minute
    } else {
        &store.hour
    };

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
        p95_ms: percentile_ms(&agg_buckets, total, 0.95),
        p99_ms: percentile_ms(&agg_buckets, total, 0.99),
        error_rate,
        availability_pct: (1.0 - error_rate) * 100.0,
        bandwidth_in_bps: bytes as f64 / effective_secs as f64,
        series,
    }
}

/// 单个端点（method+route）的累积指标行。延迟分位/错误率/计数均为「自进程启动以来」
/// 的累积值（REGISTRY 是 lifetime histogram，无时间窗口），rpm 据 uptime 折算。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointMetricsRow {
    pub method: String,
    pub route: String,
    pub count: u64,
    /// 每分钟请求数 = count / uptime_secs × 60（累积均值，非瞬时）
    pub rpm: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    /// 5xx 占比 0.0–1.0
    pub error_rate: f64,
}

/// 端点流量表数据源：把 REGISTRY 的 (method, route, status_class) 直方图按
/// (method, route) 合并 status 类，算 p50/p95/p99 + 5xx 错误率 + rpm。
/// 累积自进程启动（lifetime histogram，无窗口语义），按 count 降序。
pub async fn endpoints_snapshot(uptime_secs: u64) -> Vec<EndpointMetricsRow> {
    use std::collections::HashMap;

    let data = snapshot().await;
    // (method, route) -> (聚合桶, count, err5xx)
    let mut grouped: HashMap<(String, String), (Vec<u64>, u64, u64)> = HashMap::new();
    for ((method, route, status), hist) in &data {
        let entry = grouped
            .entry((method.clone(), route.clone()))
            .or_insert_with(|| (vec![0u64; BUCKET_BOUNDS.len() + 1], 0, 0));
        for (i, b) in hist.buckets.iter().enumerate() {
            entry.0[i] += b;
        }
        entry.1 += hist.count;
        if status == "5xx" {
            entry.2 += hist.count;
        }
    }

    let secs = uptime_secs.max(1) as f64;
    let mut rows: Vec<EndpointMetricsRow> = grouped
        .into_iter()
        .map(|((method, route), (buckets, count, err5xx))| EndpointMetricsRow {
            method,
            route,
            count,
            rpm: count as f64 / secs * 60.0,
            p50_ms: percentile_ms(&buckets, count, 0.50),
            p95_ms: percentile_ms(&buckets, count, 0.95),
            p99_ms: percentile_ms(&buckets, count, 0.99),
            error_rate: if count > 0 {
                err5xx as f64 / count as f64
            } else {
                0.0
            },
        })
        .collect();
    rows.sort_by(|a, b| b.count.cmp(&a.count));
    rows
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
            p50_ms: percentile_ms(&buckets, count, 0.50),
            p95_ms: percentile_ms(&buckets, count, 0.95),
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
    fn observe_capped_rejects_new_keys_at_capacity_but_updates_existing() {
        let mut map: std::collections::HashMap<HistogramKey, HistogramData> =
            std::collections::HashMap::new();
        let existing = ("GET".into(), "/api/words/:id".into(), "2xx".into());
        observe_capped(&mut map, existing.clone(), 0.01, 2);
        observe_capped(
            &mut map,
            ("GET".into(), OTHER_ROUTE.into(), "4xx".into()),
            0.01,
            2,
        );
        assert_eq!(map.len(), 2, "未达上限应正常新增");
        // 已满（cap=2）：新 key 被拒，map 不增长
        observe_capped(
            &mut map,
            ("POST".into(), "/api/records".into(), "2xx".into()),
            0.01,
            2,
        );
        assert_eq!(map.len(), 2, "满容量后新 key 应被拒绝，防 OOM");
        // 已存在 key 仍可累加
        observe_capped(&mut map, existing.clone(), 0.02, 2);
        assert_eq!(map.get(&existing).unwrap().count, 2, "已有 key 应继续累加");
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
        // W3-3 细化后桶边界 [0.01,0.025,0.05,0.1,0.25,0.5,1.0,2.5]+Inf
        let mut h = HistogramData::new();
        // 0.03 秒：> 0.025，故 le=0.025 未命中，落在 le=0.05 及以上所有 bucket
        h.observe(0.03);
        assert_eq!(h.count, 1);
        assert!((h.sum - 0.03).abs() < 1e-10);
        // le=0.01 (idx0) 未命中
        assert_eq!(h.buckets[0], 0);
        // le=0.025 (idx1) 未命中（0.03 > 0.025）
        assert_eq!(h.buckets[1], 0);
        // le=0.05 (idx2) 命中
        assert_eq!(h.buckets[2], 1);
        // +Inf
        assert_eq!(*h.buckets.last().unwrap(), 1);
    }

    #[test]
    fn percentile_ms_interpolates_within_bucket() {
        // 100 个观测，W3-3 桶边界 [0.01,0.025,0.05,0.1,0.25,0.5,1.0,2.5]+Inf（9 桶）。
        // 累积桶：50 个 ≤0.01，全部 ≤0.025。
        let buckets = vec![50, 100, 100, 100, 100, 100, 100, 100, 100];
        // P50 落在第 0 桶上界（target=50，cum[0]=50 命中）→ ≤10ms
        let p50 = percentile_ms(&buckets, 100, 0.50);
        assert!(p50 > 0.0 && p50 <= 10.0, "p50={p50}");
        // P99 落在 (0.01,0.025] 桶内 → 10..=25ms
        let p99 = percentile_ms(&buckets, 100, 0.99);
        assert!(p99 > 10.0 && p99 <= 25.0, "p99={p99}");
        // 空数据返回 0
        assert_eq!(percentile_ms(&[0; 9], 0, 0.99), 0.0);
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

    #[test]
    fn import_resets_mismatched_length_buckets() {
        // W3-3：旧 schema（6 桶）持久化行在桶数变更后 import，应被重置为 want 长度全零，
        // 而非 resize 错位污染 p99；count/err5xx 保留（可用率不受影响）。
        let want = BUCKET_BOUNDS.len() + 1;
        // 用远未来 unique hour key 避免与其它测试的全局 ROLLING 状态碰撞。
        let key = 9_000_000_i64 + (want as i64);
        // 旧 6 桶：+Inf 在 index 5 = 100（全部观测）。
        let old_buckets = vec![10u64, 20, 30, 40, 50, 100];
        import_hour_rollup(vec![(key, 100, 3, 4096, old_buckets)]);
        let exported = export_hour_rollup();
        let row = exported
            .into_iter()
            .find(|(k, ..)| *k == key)
            .expect("imported row present");
        assert_eq!(row.1, 100, "count 应保留");
        assert_eq!(row.2, 3, "err5xx 应保留(可用率不受桶变更影响)");
        assert_eq!(row.4.len(), want, "桶向量应为新 schema 长度");
        assert!(
            row.4.iter().all(|&b| b == 0),
            "桶数不符的旧行直方图应被重置为零，而非 resize 错位"
        );
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
