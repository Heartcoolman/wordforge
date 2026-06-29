//! BA3b：分阶段管线计时 + 单记录瀑布 trace（进程内，无 DB）。
//!
//! 两套独立的进程内聚合，镜像 middleware/http_metrics.rs 的 RollingStore 机制：
//!
//! 1) StageRollingStore —— 按 [`Stage`] 分桶的滚动窗口（每分钟槽），每槽维护累积
//!    直方图（分位）+ 真实 max + 计数 + 错误数。stage_observe() 在各阶段边界调用，
//!    aggregate(window) 产出 per-stage {count,rpm,p50,p95,p99,max,errorRate}。
//!    重启清零（实时管线视图可接受）。
//!
//! 2) 单记录瀑布 trace —— 一个有界环形缓冲（CAP=50），存最近 N 条记录摄取的
//!    分步 span（{stage,startOffsetMs,durMs}）。records 摄取 handler 逐步 Instant 计时后 push。
//!
//! 直方图边界用毫秒域（覆盖典型阶段延迟 0.5ms..5s），与 http_metrics 的秒域边界不同。

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use once_cell::sync::Lazy;
use serde::Serialize;

/// 管线阶段（按请求流转顺序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    Gateway,
    Auth,
    RateLimit,
    Amas,
    Sqlite,
    Outbox,
    Sse,
}

impl Stage {
    /// 输出用稳定标识（camelCase 化的阶段名）。
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Gateway => "gateway",
            Stage::Auth => "auth",
            Stage::RateLimit => "rateLimit",
            Stage::Amas => "amas",
            Stage::Sqlite => "sqlite",
            Stage::Outbox => "outbox",
            Stage::Sse => "sse",
        }
    }

    /// 管线顺序（仅含已插桩阶段；输出按此顺序）。
    const ORDER: [Stage; 7] = [
        Stage::Gateway,
        Stage::Auth,
        Stage::RateLimit,
        Stage::Amas,
        Stage::Sqlite,
        Stage::Outbox,
        Stage::Sse,
    ];

    fn index(self) -> usize {
        match self {
            Stage::Gateway => 0,
            Stage::Auth => 1,
            Stage::RateLimit => 2,
            Stage::Amas => 3,
            Stage::Sqlite => 4,
            Stage::Outbox => 5,
            Stage::Sse => 6,
        }
    }
}

/// 毫秒域直方图边界（升序，不含 +Inf）。
const BUCKET_BOUNDS_MS: &[f64] = &[
    0.5, 1.0, 2.5, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0,
];
const MINUTE_CAP: usize = 24 * 60; // 24h 每分钟槽

/// 单分钟槽：某阶段一分钟内的累积量。
#[derive(Clone)]
struct Slot {
    key: i64, // unix / 60
    count: u64,
    err: u64,
    max_ms: f64,
    /// 累积桶（含 +Inf），长度 = BUCKET_BOUNDS_MS.len() + 1。
    buckets: Vec<u64>,
}

impl Slot {
    fn new(key: i64) -> Self {
        Self {
            key,
            count: 0,
            err: 0,
            max_ms: 0.0,
            buckets: vec![0; BUCKET_BOUNDS_MS.len() + 1],
        }
    }

    fn observe(&mut self, dur_ms: f64, is_error: bool) {
        self.count += 1;
        if is_error {
            self.err += 1;
        }
        if dur_ms > self.max_ms {
            self.max_ms = dur_ms;
        }
        for (i, &bound) in BUCKET_BOUNDS_MS.iter().enumerate() {
            if dur_ms <= bound {
                self.buckets[i] += 1;
            }
        }
        *self.buckets.last_mut().unwrap() += 1;
    }
}

/// 每阶段一个分钟槽环形缓冲。
struct StageRollingStore {
    minute: [VecDeque<Slot>; 7],
}

static STAGES: Lazy<Mutex<StageRollingStore>> = Lazy::new(|| {
    Mutex::new(StageRollingStore {
        minute: Default::default(),
    })
});

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn touch_slot(dq: &mut VecDeque<Slot>, key: i64) -> &mut Slot {
    if dq.back().map(|s| s.key) != Some(key) {
        dq.push_back(Slot::new(key));
        while dq.len() > MINUTE_CAP {
            dq.pop_front();
        }
    }
    dq.back_mut().unwrap()
}

/// 记录一次阶段观测。dur_ms = 该阶段耗时（毫秒）；is_error = 该阶段是否出错。
/// 仅持锁 + push，无 await，热路径开销可忽略。
pub fn stage_observe(stage: Stage, dur_ms: f64, is_error: bool) {
    let key = now_unix() / 60;
    let mut store = match STAGES.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    touch_slot(&mut store.minute[stage.index()], key).observe(dur_ms, is_error);
}

/// 从累积桶估算分位（毫秒），桶内线性插值。
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
            let bound = if i < BUCKET_BOUNDS_MS.len() {
                BUCKET_BOUNDS_MS[i]
            } else {
                BUCKET_BOUNDS_MS[BUCKET_BOUNDS_MS.len() - 1] * 1.5
            };
            let in_bucket = cum - prev_cum;
            let frac = if in_bucket > 0.0 {
                ((target - prev_cum) / in_bucket).clamp(0.0, 1.0)
            } else {
                0.0
            };
            return prev_bound + (bound - prev_bound) * frac;
        }
        prev_cum = cum;
        if i < BUCKET_BOUNDS_MS.len() {
            prev_bound = BUCKET_BOUNDS_MS[i];
        }
    }
    prev_bound
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageAgg {
    pub stage: String,
    pub count: u64,
    pub rpm: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
    pub error_rate: f64,
}

/// 聚合给定窗口内各阶段指标。仅返回窗口内有观测的阶段（无数据的阶段省略），
/// 按管线顺序排列。rpm = count / window_minutes。
pub fn aggregate(window_secs: u64) -> Vec<StageAgg> {
    let now = now_unix();
    let cutoff = now - window_secs as i64;
    let window_mins = (window_secs as f64 / 60.0).max(1.0 / 60.0);
    let store = match STAGES.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };

    let mut out = Vec::new();
    for stage in Stage::ORDER {
        let dq = &store.minute[stage.index()];
        let slots: Vec<&Slot> = dq.iter().filter(|s| s.key * 60 >= cutoff).collect();
        if slots.is_empty() {
            continue;
        }
        let mut total = 0u64;
        let mut err = 0u64;
        let mut max_ms = 0.0f64;
        let mut agg = vec![0u64; BUCKET_BOUNDS_MS.len() + 1];
        for s in &slots {
            total += s.count;
            err += s.err;
            if s.max_ms > max_ms {
                max_ms = s.max_ms;
            }
            for (i, b) in s.buckets.iter().enumerate() {
                agg[i] += b;
            }
        }
        if total == 0 {
            continue;
        }
        out.push(StageAgg {
            stage: stage.as_str().to_string(),
            count: total,
            rpm: total as f64 / window_mins,
            p50_ms: percentile_ms(&agg, total, 0.50),
            p95_ms: percentile_ms(&agg, total, 0.95),
            p99_ms: percentile_ms(&agg, total, 0.99),
            max_ms,
            error_rate: err as f64 / total as f64,
        });
    }
    out
}

// ─────────────── 单记录瀑布 trace ───────────────

const TRACE_CAP: usize = 50;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Span {
    pub stage: String,
    pub start_offset_ms: f64,
    pub dur_ms: f64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Trace {
    pub record_id: String,
    pub ts_ms: i64,
    pub total_ms: f64,
    pub spans: Vec<Span>,
}

/// 单记录摄取的分步计时构建器。start() 锚定 t0，mark() 在每个子步骤结束时调用，
/// 自动按相邻 mark 的时间差填 span（startOffset 相对 t0，dur = 与上个 mark 的间隔）。
pub struct TraceBuilder {
    record_id: String,
    t0: std::time::Instant,
    last: std::time::Instant,
    spans: Vec<Span>,
}

impl TraceBuilder {
    pub fn start(record_id: &str) -> Self {
        let now = std::time::Instant::now();
        Self {
            record_id: record_id.to_string(),
            t0: now,
            last: now,
            spans: Vec::new(),
        }
    }

    /// 标记一个子步骤完成：span 名 = stage，dur = 距上次 mark 的耗时。
    pub fn mark(&mut self, stage: &str) {
        let now = std::time::Instant::now();
        let start_offset_ms = self.last.duration_since(self.t0).as_secs_f64() * 1000.0;
        let dur_ms = now.duration_since(self.last).as_secs_f64() * 1000.0;
        self.spans.push(Span {
            stage: stage.to_string(),
            start_offset_ms,
            dur_ms,
        });
        self.last = now;
    }

    /// 完成并写入环形缓冲。total = 自 t0 起的总耗时。
    pub fn finish(self) {
        let total_ms = self.last.duration_since(self.t0).as_secs_f64() * 1000.0;
        let trace = Trace {
            record_id: self.record_id,
            ts_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
            total_ms,
            spans: self.spans,
        };
        push_trace(trace);
    }
}

static TRACES: Lazy<Mutex<VecDeque<Trace>>> =
    Lazy::new(|| Mutex::new(VecDeque::with_capacity(TRACE_CAP)));

fn push_trace(trace: Trace) {
    if let Ok(mut ring) = TRACES.lock() {
        if ring.len() >= TRACE_CAP {
            ring.pop_front();
        }
        ring.push_back(trace);
    }
}

/// 最近一条 trace（None = 尚无）。
pub fn latest_trace() -> Option<Trace> {
    let ring = match TRACES.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    ring.back().cloned()
}

/// 最近 N 条 trace（最新在前），limit 截断。供 admin /traces 端点。
/// 环形缓冲容量 TRACE_CAP，实际返回数 = min(limit, 现有条数)。
pub fn snapshot_traces(limit: usize) -> Vec<Trace> {
    let ring = match TRACES.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    ring.iter().rev().take(limit).cloned().collect()
}

/// 按 record_id 查 trace（最近优先）。
pub fn trace_by_record(record_id: &str) -> Option<Trace> {
    let ring = match TRACES.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    ring.iter().rev().find(|t| t.record_id == record_id).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_aggregate_basic() {
        stage_observe(Stage::Sqlite, 3.0, false);
        stage_observe(Stage::Sqlite, 7.0, true);
        let aggs = aggregate(3600);
        let sqlite = aggs.iter().find(|a| a.stage == "sqlite").unwrap();
        assert_eq!(sqlite.count, 2);
        assert!((sqlite.error_rate - 0.5).abs() < 1e-9);
        assert!(sqlite.max_ms >= 7.0);
        assert!(sqlite.p99_ms > 0.0);
    }

    #[test]
    fn trace_builder_records_spans_and_total() {
        let mut tb = TraceBuilder::start("rec-xyz");
        tb.mark("validate");
        tb.mark("amas");
        tb.finish();
        let t = trace_by_record("rec-xyz").expect("trace present");
        assert_eq!(t.record_id, "rec-xyz");
        assert_eq!(t.spans.len(), 2);
        assert_eq!(t.spans[0].stage, "validate");
        assert!(t.total_ms >= 0.0);
        // latest_trace 也应可见
        assert!(latest_trace().is_some());
    }
}
