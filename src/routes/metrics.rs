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
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} counter");
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
    let amas_snapshot = state.amas().metrics_registry().snapshot();

    for (algo, snap) in &amas_snapshot {
        let label = format!("algorithm=\"{algo}\"");
        counter(
            &mut out,
            "amas_process_event_calls",
            "AMAS 算法处理事件的累计调用次数",
            &label,
            snap.call_count as f64,
        );
        counter(
            &mut out,
            "amas_process_event_errors",
            "AMAS 算法处理事件时发生错误的累计次数",
            &label,
            snap.error_count as f64,
        );
        counter(
            &mut out,
            "amas_process_event_latency_us",
            "AMAS 算法处理事件的累计延迟（微秒），用于计算平均延迟",
            &label,
            snap.total_latency_us as f64,
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
            .run_store_task("metrics.worker_last_run", |store| store.list_worker_last_run())
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
