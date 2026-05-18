//! /api/telemetry 路由集成测试 —— 触发 submit_telemetry 全分支：
//!   - 鉴权与缺少 X-Device-Id
//!   - on_demand 缺少 requestId
//!   - payload 负数 sessionDurationSecs / errorCount / actionsPerMin / avgResponseTimeMs
//!   - extract_summary 各种字段缺失 vs 完整 device/behavior
//!   - 成功路径：写 store + 更新 heartbeat

mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token};
use common::http::{request, response_json};

fn dev_header(device_id: &str) -> (&'static str, String) {
    ("x-device-id", device_id.to_string())
}

#[tokio::test]
async fn it_telemetry_missing_auth_returns_401() {
    let app = spawn_test_server().await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/telemetry",
        Some(serde_json::json!({
            "eventType": "heartbeat",
            "clientTs": "2026-05-18T00:00:00Z",
            "payload": {}
        })),
        &[dev_header("dev-1")],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn it_telemetry_missing_device_id_returns_400() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/telemetry",
        Some(serde_json::json!({
            "eventType": "heartbeat",
            "clientTs": "2026-05-18T00:00:00Z",
            "payload": {}
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "MISSING_DEVICE_ID");
}

#[tokio::test]
async fn it_telemetry_on_demand_requires_request_id() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/telemetry",
        Some(serde_json::json!({
            "eventType": "on_demand",
            "clientTs": "2026-05-18T00:00:00Z",
            "payload": {}
        })),
        &[("authorization", auth_header(&token)), dev_header("d1")],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_TELEMETRY");
}

#[tokio::test]
async fn it_telemetry_negative_numeric_payloads_return_422() {
    // 表驱动：4 个负数字段（sessionDurationSecs / errorCount / actionsPerMin / avgResponseTimeMs）
    // 都应被 INVALID_PAYLOAD 拒绝。
    let cases: &[(&str, serde_json::Value)] = &[
        ("sessionDurationSecs", serde_json::json!({"sessionDurationSecs": -1})),
        ("errorCount", serde_json::json!({"errorCount": -3})),
        ("actionsPerMin", serde_json::json!({"actionsPerMin": -0.5})),
        ("avgResponseTimeMs", serde_json::json!({"avgResponseTimeMs": -2.0})),
    ];
    for (field, payload) in cases {
        let app = spawn_test_server().await;
        let token = login_and_get_token(&app.app).await;
        let resp = request(
            &app.app,
            Method::POST,
            "/api/telemetry",
            Some(serde_json::json!({
                "eventType": "heartbeat",
                "clientTs": "2026-05-18T00:00:00Z",
                "payload": payload
            })),
            &[("authorization", auth_header(&token)), dev_header("d1")],
        )
        .await;
        let (status, _, body) = response_json(resp).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "field={field}");
        assert_eq!(body["code"], "INVALID_PAYLOAD", "field={field}");
        // sessionDurationSecs 在错误信息里被点名；其他字段当前实现合并为通用提示
        if *field == "sessionDurationSecs" {
            assert!(body["message"].as_str().unwrap().contains(field));
        }
    }
}

#[tokio::test]
async fn it_telemetry_minimal_heartbeat_success() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/telemetry",
        Some(serde_json::json!({
            "eventType": "heartbeat",
            "clientTs": "2026-05-18T00:00:00Z",
            "payload": {}
        })),
        &[("authorization", auth_header(&token)), dev_header("dev-mini")],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert!(body["data"]["id"].is_string());
}

#[tokio::test]
async fn it_telemetry_full_payload_extracts_summary_fields() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    let payload = serde_json::json!({
        "device": {
            "cpuCores": 8,
            "memoryGb": 16.0,
            "screenWidth": 1920,
            "screenHeight": 1080,
            "pixelRatio": 2.0,
            "osName": "macOS",
            "browserName": "Chrome",
            "browserVersion": "120",
            "timezone": "Asia/Shanghai",
            "language": "zh-CN",
            "touchSupport": false,
            "onlineStatus": true,
        },
        "behavior": {
            "currentRoute": "/learn",
            "clickCount": 12,
            "clickTargets": ["btn-a", "btn-b"],
            "scrollDepthPct": 88.5,
            "visibilityChanges": 2,
            "routeChanges": 3,
        },
        "sessionDurationSecs": 600,
        "actionsPerMin": 5.5,
        "errorCount": 0,
        "avgResponseTimeMs": 150.0,
        "featureUsage": {"flashcard": 10, "quiz": 5}
    });
    let resp = request(
        &app.app,
        Method::POST,
        "/api/telemetry",
        Some(serde_json::json!({
            "eventType": "page_view",
            "clientTs": "2026-05-18T00:00:00Z",
            "payload": payload
        })),
        &[("authorization", auth_header(&token)), dev_header("dev-full")],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
}

#[tokio::test]
async fn it_telemetry_on_demand_with_request_id_succeeds() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/telemetry",
        Some(serde_json::json!({
            "eventType": "on_demand",
            "requestId": "req-abc",
            "clientTs": "2026-05-18T00:00:00Z",
            "payload": {"sessionDurationSecs": 100}
        })),
        &[("authorization", auth_header(&token)), dev_header("dev-od")],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
}
