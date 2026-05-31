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

// ─────────── m027:设备页对齐 clients.html 设计新增 5 端点 ───────────

#[tokio::test]
async fn it_admin_clients_paginated_with_search_and_platform_filter() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    // 注入 5 设备 (3 web / 1 ios / 1 android)
    for (id, plt) in [
        ("dev-web-a", "web"),
        ("dev-web-b", "web"),
        ("dev-web-c", "web"),
        ("dev-ios-a", "ios"),
        ("dev-and-a", "android"),
    ] {
        app.state
            .store()
            .upsert_client_device(id, plt, "u-1")
            .expect("upsert");
    }

    // 默认分页
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/clients/paginated?page=1&perPage=10",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["total"], 5);

    // 平台过滤 web → 3 行
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/clients/paginated?platform=web&page=1&perPage=10",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (_, _, body) = response_json(resp).await;
    assert_eq!(body["data"]["total"], 3);

    // 搜索 'dev-ios' 命中 1 行
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/clients/paginated?q=dev-ios",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (_, _, body) = response_json(resp).await;
    assert_eq!(body["data"]["total"], 1);
}

#[tokio::test]
async fn it_admin_clients_distribution_returns_platforms_versions_policies() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    for (id, plt, ver) in [
        ("d1", "web", Some("v0.7.3")),
        ("d2", "web", Some("v0.7.3")),
        ("d3", "web", Some("v0.7.2")),
        ("d4", "ios", Some("v0.7.3")),
        ("d5", "android", None),
    ] {
        app.state
            .store()
            .upsert_client_device_with_version(id, plt, "u-1", ver)
            .expect("upsert");
    }

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/clients/distribution",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);

    let platforms = body["data"]["platforms"].as_array().unwrap();
    assert_eq!(platforms.len(), 3);
    let web_total = platforms
        .iter()
        .find(|p| p["platform"] == "web")
        .unwrap()["total"]
        .as_i64()
        .unwrap();
    assert_eq!(web_total, 3);

    let versions = body["data"]["versions"].as_array().unwrap();
    assert!(versions.iter().any(|v| v["platform"] == "web" && v["version"] == "v0.7.3"));
    assert!(versions.iter().any(|v| v["platform"] == "android" && v["version"] == "unknown"));

    // 三个 seed 平台必出现在 policies
    let policies = body["data"]["policies"].as_array().unwrap();
    let names: Vec<&str> = policies.iter().filter_map(|p| p["platform"].as_str()).collect();
    assert!(names.contains(&"web"));
    assert!(names.contains(&"ios"));
    assert!(names.contains(&"android"));
}

#[tokio::test]
async fn it_admin_clients_upgrade_policy_put_then_get_persists() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        Method::PUT,
        "/api/admin/clients/upgrade-policy/web",
        Some(serde_json::json!({
            "minVersion": "v0.6.5",
            "suggestedVersion": "v0.7.0",
            "grayscalePct": 20,
            "pwaSilentUpdate": true,
        })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/clients/upgrade-policy",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (_, _, body) = response_json(resp).await;
    let web = body["data"]["policies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["platform"] == "web")
        .unwrap()
        .clone();
    assert_eq!(web["minVersion"], "v0.6.5");
    assert_eq!(web["suggestedVersion"], "v0.7.0");
    assert_eq!(web["grayscalePct"], 20);
    assert_eq!(web["pwaSilentUpdate"], true);
}

#[tokio::test]
async fn it_admin_clients_upgrade_policy_rejects_invalid_platform() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/admin/clients/upgrade-policy/desktop",
        Some(serde_json::json!({
            "minVersion": "v0.5.0",
        })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn it_admin_clients_upgrade_policy_rejects_invalid_grayscale_pct() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/admin/clients/upgrade-policy/web",
        Some(serde_json::json!({
            "minVersion": "v0.5.0",
            "grayscalePct": 150,
        })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn it_admin_clients_broadcast_upgrade_returns_matched_count() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    // 2 老版本 + 1 新版本设备(只有老版本计入 matched)
    for (id, ver) in [
        ("dev-old-1", Some("v0.5.0")),
        ("dev-old-2", Some("v0.6.0")),
        ("dev-new", Some("v0.7.3")),
    ] {
        app.state
            .store()
            .upsert_client_device_with_version(id, "web", "u-1", ver)
            .expect("upsert");
    }

    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/clients/broadcast-upgrade/web",
        Some(serde_json::json!({
            "belowVersion": "v0.7.0",
            "latestVersion": "v0.7.3",
            "message": "强制升级",
        })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["matched"], 2);
    // pushedConnections 是 0(无活跃 SSE 连接)
    assert_eq!(body["data"]["pushedConnections"], 0);
}

/// 回归:learning_sessions 无 completed_at/words_count 列，SELECT 误用会致端点恒 500。
/// 断言端点 200 且 completed 会话正确映射出 completedAt/wordsCount/correctCount。
#[tokio::test]
async fn it_admin_user_sessions_maps_real_columns() {
    use learning_backend::store::operations::learning_sessions::{LearningSession, SessionStatus};

    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let now = chrono::Utc::now();
    app.state
        .store()
        .create_learning_session(&LearningSession {
            id: "sess-1".into(),
            user_id: "u-sessions".into(),
            status: SessionStatus::Completed,
            target_mastery_count: 10,
            total_questions: 12,
            actual_mastery_count: 8,
            context_shifts: 0,
            created_at: now,
            updated_at: now,
            summary: None,
            correct_count: 9,
            total_count: 12,
        })
        .expect("seed learning session");

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/users/u-sessions/sessions",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    let sessions = body["data"]["sessions"].as_array().expect("sessions array");
    let s = sessions.iter().find(|s| s["id"] == "sess-1").expect("sess-1");
    assert_eq!(s["wordsCount"], 12);
    assert_eq!(s["correctCount"], 9);
    assert!(s["completedAt"].is_string(), "completed 会话应有 completedAt: {s}");
}
