//! /api/admin/clients 路由集成测试 —— 在 store 中预置 client_devices + telemetry，
//! 触发 list_clients 的非空路径、ban/unban 成功路径、get_telemetry 成功路径。

mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, setup_admin_and_get_token};
use common::http::{request, response_json};

use learning_backend::store::operations::telemetry::TelemetrySummaryInput;

fn empty_summary() -> TelemetrySummaryInput {
    TelemetrySummaryInput {
        cpu_cores: None,
        memory_gb: None,
        screen_width: None,
        screen_height: None,
        pixel_ratio: None,
        os_name: None,
        browser_name: None,
        browser_version: None,
        timezone: None,
        language: None,
        touch_support: None,
        online_status: None,
        session_duration_secs: 0,
        actions_per_min: 0.0,
        error_count: 0,
        avg_response_time_ms: 0.0,
        current_route: None,
        click_count: None,
        click_targets_json: None,
        scroll_depth_pct: None,
        visibility_changes: None,
        route_changes: None,
        feature_usage_json: "{}".to_string(),
    }
}

#[tokio::test]
async fn it_admin_clients_list_includes_recently_active() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    // 直接在 store 注入设备
    app.state
        .store()
        .upsert_client_device("dev-recent", "ios", "user-x")
        .expect("upsert device");

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/clients",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    let recent = body["data"]["recentlyActive"].as_array().unwrap();
    assert!(
        recent.iter().any(|e| e["deviceId"] == "dev-recent"),
        "recentlyActive should include dev-recent: {recent:?}"
    );
}

#[tokio::test]
async fn it_admin_clients_ban_unban_existing_device() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;
    app.state
        .store()
        .upsert_client_device("dev-ban", "android", "user-y")
        .expect("upsert device");

    // ban
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/clients/dev-ban/ban",
        Some(serde_json::json!({"reason": "spam"})),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["data"]["banned"], true);
    assert_eq!(body["data"]["deviceId"], "dev-ban");

    // 验证 store 中的 is_banned
    assert!(app.state.store().is_device_banned("dev-ban").unwrap());

    // unban
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/clients/dev-ban/unban",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["data"]["banned"], false);
    assert!(!app.state.store().is_device_banned("dev-ban").unwrap());
}

#[tokio::test]
async fn it_admin_clients_ban_handles_various_reason_bodies() {
    // 表驱动：3 种 reason 输入形态都应返回 200 OK 并标记 banned
    // - 缺 body (reason=None)
    // - 超长 reason 触发 chars().take(500) 截断
    // - 空字符串视为 None
    let long_reason: String = "x".repeat(2000);
    let cases: Vec<(&str, Option<serde_json::Value>, &str)> = vec![
        ("dev-nobody", None, "missing body"),
        ("dev-long", Some(serde_json::json!({ "reason": long_reason })), "long reason truncated"),
        ("dev-empty", Some(serde_json::json!({ "reason": "" })), "empty reason"),
    ];
    for (device_id, body, label) in cases {
        let app = spawn_test_server().await;
        let admin_token = setup_admin_and_get_token(&app.app).await;
        app.state
            .store()
            .upsert_client_device(device_id, "web", "user-x")
            .expect("upsert");
        let resp = request(
            &app.app,
            Method::POST,
            &format!("/api/admin/clients/{device_id}/ban"),
            body,
            &[("authorization", auth_header(&admin_token))],
        )
        .await;
        let (status, _, body) = response_json(resp).await;
        assert_eq!(status, StatusCode::OK, "case={label} body={body}");
        assert_eq!(body["data"]["banned"], true, "case={label}");
    }
}

#[tokio::test]
async fn it_admin_clients_get_telemetry_for_existing_device() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    app.state
        .store()
        .upsert_client_device("dev-tel", "ios", "user-tel")
        .expect("upsert");

    // 注入一条 telemetry summary
    let summary = empty_summary();
    app.state
        .store()
        .insert_telemetry_and_summary(
            "tele-1",
            "dev-tel",
            "user-tel",
            "heartbeat",
            None,
            "{}",
            "2026-05-18T00:00:00Z",
            &summary,
        )
        .expect("insert telemetry");

    // 默认 limit=50, offset=0
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/telemetry/dev-tel",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert!(body["data"]["total"].as_u64().unwrap_or(0) >= 1);

    // 带 limit/offset 查询
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/telemetry/dev-tel?limit=10&offset=0",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // limit 越过上限 200 会被 clamp
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/telemetry/dev-tel?limit=5000",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
}
