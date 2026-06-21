/// /metrics 端点 — Prometheus OpenMetrics 文本格式（M0-P1）
///
/// 端点路径：/metrics
/// 鉴权：同 /api/admin/* 鉴权链（AdminAuthUser extractor）
/// 无依赖新 crate，手写 exposition 格式；RFC 4.2 规定每个 metric 写 # HELP / # TYPE。
use std::fmt::Write as _;
use std::sync::atomic::Ordering;

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;

use crate::auth::AdminAuthUser;
use crate::routes::realtime::SSE_CONNECTION_COUNT;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(metrics_handler))
}

/// 追加一条 gauge metric（含 HELP + TYPE）
fn gauge(out: &mut String, name: &str, help: &str, labels: &str, value: f64) {
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} gauge");
    if labels.is_empty() {
        let _ = writeln!(out, "{name} {value}");
    } else {
        let _ = writeln!(out, "{name}{{{labels}}} {value}");
    }
}

/// 追加一条 counter metric（含 HELP + TYPE）
fn counter(out: &mut String, name: &str, help: &str, labels: &str, value: f64) {
    counter_header(out, name, help);
    counter_line(out, name, labels, value);
}

/// 仅写 counter 的 HELP + TYPE 头（每个 metric family 只能出现一次）
fn counter_header(out: &mut String, name: &str, help: &str) {
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} counter");
}

/// 仅写 counter 的数据行（不含 HELP/TYPE），用于带 label 的多行族
fn counter_line(out: &mut String, name: &str, labels: &str, value: f64) {
    if labels.is_empty() {
        let _ = writeln!(out, "{name}_total {value}");
    } else {
        let _ = writeln!(out, "{name}_total{{{labels}}} {value}");
    }
}

pub async fn metrics_handler(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let mut out = String::with_capacity(2048);

    // --- SSE 连接数 ---
    gauge(
        &mut out,
        "sse_active_connections",
        "当前活跃 SSE 长连接数（raw socket 级别）",
        "",
        SSE_CONNECTION_COUNT.load(Ordering::Relaxed) as f64,
    );

    // --- SSE 活跃设备数 ---
    gauge(
        &mut out,
        "sse_active_devices",
        "当前持有 SSE 连接的唯一设备数",
        "",
        state.active_sse().len() as f64,
    );

    // --- 数据库文件大小 ---
    let db_bytes = state
        .run_store_task("metrics.db_size_bytes", |store| store.db_size_bytes())
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or(0);
    gauge(
        &mut out,
        "db_size_bytes",
        "SQLite 数据库文件字节数（page_count × page_size）",
        "",
        db_bytes as f64,
    );

    // --- 服务器已运行秒数 ---
    gauge(
        &mut out,
        "uptime_seconds",
        "进程已运行的秒数（自启动起）",
        "",
        state.uptime_secs() as f64,
    );

    // --- AMAS 算法指标（每个 algorithm 3 个 metric） ---
    // amas_process_event_call_count_total{algorithm}
    // amas_process_event_error_count_total{algorithm}
    // amas_process_event_latency_us_total{algorithm}  — 累计总微秒数（用于计算平均值）
    //
    // Prometheus 计数器必须单调累增。registry 每 5 分钟被 metrics_flush reset(swap 0),
    // 直接读 registry 会让 `_total` 锯齿归零→违反计数器契约且 rate()/increase() 误判 reset。
    // 故以已落库的全历史累计为基准,叠加当前未 flush 的内存增量,得到单调累计值。
    //
    // 读取顺序必须「内存在前、DB 在后」：metrics_flush 是「先 reset 内存、再落库」两阶段。
    // 若先读 DB(=P) 再读内存,可能恰好落在 flush「内存已归零、DB 尚未写入 P+D」的窗口,读到
    // P+0=P,而上一次抓取报的是 P+D → `_total` 倒退,Prometheus 误判 counter reset。先读内存、
    // 再读 DB:并发 flush 只会让同一份 D 既计入内存快照又计入已落库 DB(短暂双计、单调不减),
    // 永不少计,保证计数器单调。
    let amas_snapshot = state.amas().metrics_registry().snapshot();
    let persisted_totals = state
        .run_store_task("metrics.cumulative_amas", |store| {
            store.cumulative_metrics_totals()
        })
        .await
        .ok()
        .and_then(Result::ok)
        .unwrap_or_default();

    // 合并落库累计 + 内存增量(union of algorithm keys)。
    let mut algos: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    algos.extend(persisted_totals.keys().cloned());
    algos.extend(amas_snapshot.keys().cloned());

    // HELP/TYPE 每个 metric family 只能出现一次：在 per-algo 循环之前统一写头，
    // 循环内仅写带 label 的数据行，避免重复 HELP/TYPE 被严格 scraper 拒绝整次抓取。
    counter_header(
        &mut out,
        "amas_process_event_calls",
        "AMAS 算法处理事件的累计调用次数",
    );
    counter_header(
        &mut out,
        "amas_process_event_errors",
        "AMAS 算法处理事件时发生错误的累计次数",
    );
    counter_header(
        &mut out,
        "amas_process_event_latency_us",
        "AMAS 算法处理事件的累计延迟（微秒），用于计算平均延迟",
    );
    for algo in &algos {
        let (p_calls, p_errors, p_latency) =
            persisted_totals.get(algo).copied().unwrap_or((0, 0, 0));
        let (d_calls, d_errors, d_latency) = amas_snapshot
            .get(algo)
            .map(|s| (s.call_count, s.error_count, s.total_latency_us))
            .unwrap_or((0, 0, 0));
        let label = format!("algorithm=\"{algo}\"");
        counter_line(
            &mut out,
            "amas_process_event_calls",
            &label,
            p_calls.saturating_add(d_calls) as f64,
        );
        counter_line(
            &mut out,
            "amas_process_event_errors",
            &label,
            p_errors.saturating_add(d_errors) as f64,
        );
        counter_line(
            &mut out,
            "amas_process_event_latency_us",
            &label,
            p_latency.saturating_add(d_latency) as f64,
        );
    }

    // --- M0-P4：HTTP 请求计数（供 error_rate_watchdog 和外部监控消费）---
    let (total_req, total_5xx) = crate::metrics_counters::snapshot();
    counter(
        &mut out,
        "http_requests",
        "进入 request_id_middleware 的 HTTP 请求总数（不含健康检查）",
        "",
        total_req as f64,
    );
    counter(
        &mut out,
        "http_5xx",
        "产生 5xx 响应的 HTTP 请求总数",
        "",
        total_5xx as f64,
    );
    gauge(
        &mut out,
        "http_inflight_requests",
        "当前在途（正在处理中）的 HTTP 请求数——过载饱和度信号",
        "",
        crate::metrics_counters::inflight() as f64,
    );

    // --- 是否处于维护模式 ---
    gauge(
        &mut out,
        "maintenance_mode_active",
        "维护模式是否启用（1=是，0=否）",
        "",
        if state.is_maintenance() { 1.0 } else { 0.0 },
    );

    // --- M0-P1 补充：http_request_duration_seconds histogram ---
    crate::middleware::http_metrics::write_histogram_exposition(&mut out).await;

    // --- M1-A5：worker_last_run_seconds（读 DB 真实数据，替换原 stub）---
    {
        let _ = writeln!(&mut out, "# HELP worker_last_run_seconds 每个 worker 上次完成时的 Unix 时间戳（秒），0 表示尚未运行");
        let _ = writeln!(&mut out, "# TYPE worker_last_run_seconds gauge");
        let rows = state
            .run_store_task("metrics.worker_last_run", |store| {
                store.list_worker_last_run()
            })
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();
        for row in &rows {
            let _ = writeln!(
                &mut out,
                "worker_last_run_seconds{{name=\"{}\"}} {}",
                row.worker_name, row.last_run_at
            );
        }
    }

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        out,
    )
}
