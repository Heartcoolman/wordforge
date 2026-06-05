/// M0-P1 集成测试：/metrics 端点
///
/// 验收：
/// - admin bearer → 200，Content-Type: text/plain，含 ≥ 10 个有意义的 metric
/// - 无凭证 → 401
/// - 普通用户 token → 401
mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token, setup_admin_and_get_token};
use common::http::{request, response_json};

/// 从 Prometheus 文本响应中解析出所有出现的 metric 名（# TYPE 行）
fn parse_metric_names(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            line.strip_prefix("# TYPE ")
                .map(|rest| rest.split_whitespace().next().unwrap_or("").to_string())
        })
        .collect()
}

#[tokio::test]
async fn metrics_endpoint_requires_admin_auth() {
    let app = spawn_test_server().await;

    // 无凭证 → 401
    let resp = request(&app.app, Method::GET, "/metrics", None, &[]).await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn metrics_endpoint_rejects_user_token() {
    let app = spawn_test_server().await;
    let user_token = login_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        Method::GET,
        "/metrics",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "普通用户 token 应被拒绝");
}

#[tokio::test]
async fn metrics_endpoint_returns_prometheus_text_with_at_least_10_metrics() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        Method::GET,
        "/metrics",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;

    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read body");
    let body = String::from_utf8_lossy(&bytes).to_string();

    assert_eq!(status, StatusCode::OK, "body: {body}");

    // Content-Type 必须是 text/plain
    let ct = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ct.contains("text/plain"),
        "Content-Type 应为 text/plain，实际：{ct}"
    );

    // 解析 metric 名列表
    let names = parse_metric_names(&body);
    assert!(
        names.len() >= 10,
        "应导出 ≥ 10 个 metric，实际 {}：\n{body}",
        names.len()
    );

    // 必须含这些核心 metric
    let required = [
        "sse_active_connections",
        "sse_active_devices",
        "db_size_bytes",
        "uptime_seconds",
        "maintenance_mode_active",
    ];
    for name in &required {
        assert!(
            names.iter().any(|n| n == name),
            "缺少必要 metric：{name}\n已有：{names:?}"
        );
    }

    // 每个 AMAS 算法都应有 call/error/latency 三条（共 18 条 metric 行）
    let amas_algos = ["heuristic", "ige", "swd", "ensemble", "mdm", "mastery"];
    let amas_kinds = [
        "amas_process_event_calls",
        "amas_process_event_errors",
        "amas_process_event_latency_us",
    ];
    for algo in &amas_algos {
        for kind in &amas_kinds {
            let line = format!("{kind}_total{{algorithm=\"{algo}\"}}");
            assert!(body.contains(&line), "缺少行：{line}\n实际响应：\n{body}");
        }
    }

    // M0-P1 补充验收：histogram + worker_last_run_seconds 必须存在
    assert!(
        body.contains("http_request_duration_seconds_bucket"),
        "缺少 http_request_duration_seconds_bucket\n实际响应：\n{body}"
    );
    assert!(
        body.contains("http_request_duration_seconds_count"),
        "缺少 http_request_duration_seconds_count\n实际响应：\n{body}"
    );
    assert!(
        body.contains("http_request_duration_seconds_sum"),
        "缺少 http_request_duration_seconds_sum\n实际响应：\n{body}"
    );
    assert!(
        body.contains("worker_last_run_seconds"),
        "缺少 worker_last_run_seconds\n实际响应：\n{body}"
    );
}

#[tokio::test]
async fn metrics_endpoint_has_help_and_type_for_each_metric() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        Method::GET,
        "/metrics",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read body");
    let body = String::from_utf8_lossy(&bytes).to_string();

    // 每个 metric 名出现在 # HELP 行中必须也出现在 # TYPE 行中
    let help_names: Vec<String> = body
        .lines()
        .filter_map(|l| {
            l.trim()
                .strip_prefix("# HELP ")
                .map(|r| r.split_whitespace().next().unwrap_or("").to_string())
        })
        .collect();
    let type_names = parse_metric_names(&body);

    assert_eq!(
        help_names.len(),
        type_names.len(),
        "# HELP 条数应与 # TYPE 条数相等"
    );
}
