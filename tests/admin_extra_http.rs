//! admin/* 路由补充：auth、broadcast、clients、monitoring、settings、updates
//! 关注未授权、参数校验、缺少 updater 等错误路径。

mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token, setup_admin_and_get_token};
use common::http::{request, response_json};

// ────────────────────── admin/auth ──────────────────────

#[tokio::test]
async fn it_admin_auth_status_then_setup_then_login() {
    let app = spawn_test_server().await;

    // status 未初始化
    let resp = request(&app.app, Method::GET, "/api/admin/auth/status", None, &[]).await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["initialized"], false);

    // setup invalid email
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/setup",
        Some(serde_json::json!({"email": "not-an-email", "password": "Passw0rd!Long"})),
        &[],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "ADMIN_INVALID_EMAIL");

    // setup weak password
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/setup",
        Some(serde_json::json!({"email": "a@b.com", "password": "weak"})),
        &[],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "ADMIN_WEAK_PASSWORD");

    // setup success
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/setup",
        Some(serde_json::json!({"email": "admin@test.com", "password": "AdminPassw0rd!"})),
        &[],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CREATED);
    let token = body["data"]["token"].as_str().expect("token").to_string();

    // status 已初始化
    let resp = request(&app.app, Method::GET, "/api/admin/auth/status", None, &[]).await;
    let (_, _, body) = response_json(resp).await;
    assert_eq!(body["data"]["initialized"], true);

    // setup 再次 -> 409
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/setup",
        Some(serde_json::json!({"email": "admin2@test.com", "password": "AdminPassw0rd!"})),
        &[],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "ADMIN_ALREADY_EXISTS");

    // login wrong password -> 401
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/login",
        Some(serde_json::json!({"email": "admin@test.com", "password": "WrongPassw0rd!"})),
        &[],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // login non-existent -> 401
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/login",
        Some(serde_json::json!({"email": "nobody@test.com", "password": "AdminPassw0rd!"})),
        &[],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // login success
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/login",
        Some(serde_json::json!({"email": "admin@test.com", "password": "AdminPassw0rd!"})),
        &[],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let login_token = body["data"]["token"].as_str().unwrap().to_string();

    // verify with token
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/auth/verify",
        None,
        &[("authorization", auth_header(&login_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["email"], "admin@test.com");

    // logout
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/logout",
        None,
        &[("authorization", auth_header(&login_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["loggedOut"], true);

    // 用初始 setup token logout（仍有效）
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/logout",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn it_admin_auth_unauthorized_paths() {
    let app = spawn_test_server().await;
    for path in [
        "/api/admin/auth/verify",
    ] {
        let resp = request(&app.app, Method::GET, path, None, &[]).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "path {path}");
    }
    let resp = request(&app.app, Method::POST, "/api/admin/auth/logout", None, &[]).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ────────────────────── admin/broadcast ──────────────────────

#[tokio::test]
async fn it_admin_broadcast_validation_and_persist() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    // empty title -> 400
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/broadcast",
        Some(serde_json::json!({"title": "", "message": "hi"})),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_TITLE");

    // empty message -> 400
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/broadcast",
        Some(serde_json::json!({"title": "T", "message": ""})),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_MESSAGE");

    // title too long -> 400
    let long_title = "a".repeat(201);
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/broadcast",
        Some(serde_json::json!({"title": long_title, "message": "hi"})),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // message too long
    let long_msg = "x".repeat(10_001);
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/broadcast",
        Some(serde_json::json!({"title": "T", "message": long_msg})),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // happy path：注册一个用户接收
    let _user_token = login_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/broadcast",
        Some(serde_json::json!({"title": "Hello", "message": "broadcast"})),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["sent"].as_u64().unwrap_or(0) >= 1);
    assert!(body["data"]["broadcastId"].is_string());
}

#[tokio::test]
async fn it_admin_broadcast_update_uses_state() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/broadcast-update",
        Some(serde_json::json!({"version": "1.2.3", "message": "update available"})),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["broadcasted"], true);

    // both fields None 仍 OK
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/broadcast-update",
        Some(serde_json::json!({})),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn it_admin_broadcast_unauthorized() {
    let app = spawn_test_server().await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/broadcast",
        Some(serde_json::json!({"title": "T", "message": "M"})),
        &[],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ────────────────────── admin/clients ──────────────────────

#[tokio::test]
async fn it_admin_clients_list_empty() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/clients",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["sseLive"].as_array().unwrap().is_empty());
    assert!(body["data"]["recentlyActive"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn it_admin_clients_ban_unban_non_existent_returns_404() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    // ban missing device
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/clients/missing-device/ban",
        Some(serde_json::json!({"reason": "test"})),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // ban without body (Option<JsonBody>=None) -> 404
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/clients/missing-device/ban",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // unban missing device
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/clients/missing-device/unban",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // request-telemetry for offline device -> 422
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/clients/missing-device/request-telemetry",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "DEVICE_OFFLINE");
}

#[tokio::test]
async fn it_admin_clients_telemetry_get_for_missing_device_returns_404() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/telemetry/missing-device?limit=10&offset=0",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn it_admin_clients_unauthorized() {
    let app = spawn_test_server().await;
    let resp = request(&app.app, Method::GET, "/api/admin/clients", None, &[]).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ────────────────────── admin/monitoring ──────────────────────

#[tokio::test]
async fn it_admin_monitoring_health_database_check_update() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    // system_health
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/monitoring/health",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(["healthy", "degraded", "down"].contains(&body["data"]["status"].as_str().unwrap()));
    assert!(body["data"]["uptimeSecs"].is_number());

    // database_stats
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/monitoring/database",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["tableCount"].is_number());

    // check_update: 默认 api_url 为空，走 fallback
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/monitoring/check-update",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["hasUpdate"], false);

    // 再次调用走缓存
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/monitoring/check-update",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn it_admin_monitoring_unauthorized() {
    let app = spawn_test_server().await;
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/monitoring/health",
        None,
        &[],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ────────────────────── admin/settings ──────────────────────

#[tokio::test]
async fn it_admin_settings_validation_paths() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    // max_users 越界
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/admin/settings",
        Some(serde_json::json!({"maxUsers": 0})),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_MAX_USERS");

    // default_daily_words 越界
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/admin/settings",
        Some(serde_json::json!({"defaultDailyWords": 600})),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_DAILY_WORDS");

    // amas_auto_apply_max_per_day 越界
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/admin/settings",
        Some(serde_json::json!({"amasAutoApplyMaxPerDay": 21})),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_AUTO_APPLY_LIMIT");

    // amas_auto_apply_min_confidence 越界
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/admin/settings",
        Some(serde_json::json!({"amasAutoApplyMinConfidence": 1.5})),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_CONFIDENCE");

    // happy: 一次更新所有字段
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/admin/settings",
        Some(serde_json::json!({
            "maxUsers": 5000,
            "registrationEnabled": false,
            "maintenanceMode": true,
            "defaultDailyWords": 50,
            "wordbookCenterUrl": "https://example.com/wbc",
            "amasAutoApplyEnabled": true,
            "amasAutoApplyMaxPerDay": 5,
            "amasAutoApplyMinConfidence": 0.9
        })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["maintenanceMode"], true);
    assert_eq!(body["data"]["registrationEnabled"], false);

    // read back via GET
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/settings",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["maxUsers"], 5000);
}

#[tokio::test]
async fn it_admin_settings_reload_amas_with_invalid_config() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    // 完全无效的 payload -> 400 (JsonBody 拒绝)
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/settings/reload-amas",
        Some(serde_json::json!({"bogus": true})),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "expected 400/422, got {status}"
    );
}

#[tokio::test]
async fn it_admin_settings_unauthorized() {
    let app = spawn_test_server().await;
    let resp = request(&app.app, Method::GET, "/api/admin/settings", None, &[]).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ────────────────────── admin/updates ──────────────────────

#[tokio::test]
async fn it_admin_updates_disabled_returns_503() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    // 测试 app 没有注入 Updater，三个端点都应 503
    for (method, path) in [
        (Method::GET, "/api/admin/updates/status"),
        (Method::POST, "/api/admin/updates/check"),
    ] {
        let resp = request(
            &app.app,
            method,
            path,
            None,
            &[("authorization", auth_header(&admin_token))],
        )
        .await;
        let (status, _, body) = response_json(resp).await;
        assert_eq!(
            status,
            StatusCode::SERVICE_UNAVAILABLE,
            "path={path}: body={body}"
        );
        assert_eq!(body["code"], "UPDATER_DISABLED");
    }

    // apply 需要 body（v0.6.0-beta.3 起 channel 必填）
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/updates/apply",
        Some(serde_json::json!({
            "channel": "beta",
            "targetVersion": "v9.9.9",
            "confirmCurrentVersion": "v0.0.0"
        })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "UPDATER_DISABLED");
}

#[tokio::test]
async fn it_admin_updates_unauthorized() {
    let app = spawn_test_server().await;
    let resp = request(&app.app, Method::GET, "/api/admin/updates/status", None, &[]).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ────────────────────── admin/analytics ──────────────────────

#[tokio::test]
async fn it_admin_analytics_engagement_and_metrics() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    for (path, code) in [
        ("/api/admin/analytics/engagement", StatusCode::OK),
        ("/api/admin/analytics/learning", StatusCode::OK),
        ("/api/admin/analytics/daily-active-users?days=7", StatusCode::OK),
        ("/api/admin/analytics/daily-records?days=7", StatusCode::OK),
        ("/api/admin/analytics/study-overview?days=7", StatusCode::OK),
        ("/api/admin/analytics/study-overview?days=14&category=learning", StatusCode::OK),
        ("/api/admin/analytics/study-overview?days=14&category=review", StatusCode::OK),
        ("/api/admin/analytics/record-types?days=7", StatusCode::OK),
        ("/api/admin/analytics/word-states", StatusCode::OK),
        ("/api/admin/analytics/word-states?category=all", StatusCode::OK),
        ("/api/admin/analytics/word-states?category=learning", StatusCode::OK),
        ("/api/admin/analytics/word-states?category=review", StatusCode::OK),
        ("/api/admin/analytics/retention-curve", StatusCode::OK),
        ("/api/admin/analytics/retention-curve?category=learning", StatusCode::OK),
    ] {
        let resp = request(
            &app.app,
            Method::GET,
            path,
            None,
            &[("authorization", auth_header(&admin_token))],
        )
        .await;
        let (status, _, body) = response_json(resp).await;
        assert_eq!(status, code, "path={path}: body={body}");
    }
}

#[tokio::test]
async fn it_admin_analytics_validation_failures() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    // days 越界
    for path in [
        "/api/admin/analytics/daily-active-users?days=0",
        "/api/admin/analytics/daily-records?days=31",
        "/api/admin/analytics/study-overview?days=100",
        "/api/admin/analytics/record-types?days=0",
    ] {
        let resp = request(
            &app.app,
            Method::GET,
            path,
            None,
            &[("authorization", auth_header(&admin_token))],
        )
        .await;
        let (status, _, body) = response_json(resp).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "path={path}: body={body}");
        assert_eq!(body["code"], "INVALID_DAYS");
    }

    // invalid category
    for path in [
        "/api/admin/analytics/study-overview?days=7&category=bogus",
        "/api/admin/analytics/word-states?category=bogus",
        "/api/admin/analytics/retention-curve?category=bogus",
    ] {
        let resp = request(
            &app.app,
            Method::GET,
            path,
            None,
            &[("authorization", auth_header(&admin_token))],
        )
        .await;
        let (status, _, body) = response_json(resp).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "path={path}: body={body}");
        assert_eq!(body["code"], "INVALID_CATEGORY");
    }
}

#[tokio::test]
async fn it_admin_analytics_unauthorized() {
    let app = spawn_test_server().await;
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/analytics/engagement",
        None,
        &[],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ────────────────────── admin top-level: stats / users / password reset ──────────────────────

#[tokio::test]
async fn it_admin_stats_and_users_endpoints() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;
    let _user_token = login_and_get_token(&app.app).await;

    // stats
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/stats",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["users"].as_u64().unwrap_or(0) >= 1);

    // list users (no filter)
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/users?page=1&perPage=10",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body["data"]["data"].as_array().unwrap().is_empty());
    let target_user_id = body["data"]["data"][0]["id"].as_str().unwrap().to_string();

    // list users with search
    let email_prefix = body["data"]["data"][0]["email"]
        .as_str()
        .unwrap()
        .chars()
        .take(4)
        .collect::<String>();
    let resp = request(
        &app.app,
        Method::GET,
        &format!("/api/admin/users?search={email_prefix}&banned=false"),
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);

    // ban missing -> 404
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users/no-such-user/ban",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // unban missing -> 404
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users/no-such-user/unban",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // ban existing
    let resp = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/users/{target_user_id}/ban"),
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["banned"], true);

    // unban
    let resp = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/users/{target_user_id}/unban"),
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["banned"], false);

    // reset password missing user -> 404
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users/no-such-user/reset-password",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // reset password actual
    let resp = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/users/{target_user_id}/reset-password"),
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["resetKey"].is_string());

    // set password weak
    let resp = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/users/{target_user_id}/set-password"),
        Some(serde_json::json!({"newPassword": "weak"})),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "AUTH_WEAK_PASSWORD");

    // set password missing user
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users/no-such-user/set-password",
        Some(serde_json::json!({"newPassword": "Strongp4ssword!"})),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // set password ok
    let resp = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/users/{target_user_id}/set-password"),
        Some(serde_json::json!({"newPassword": "Strongp4ssword!"})),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["passwordReset"], true);
}

/// v1.1-P2.10：ban / unban / reset-password / set-password 四类 admin 敏感操作
/// 都应在 update_audit_log 留下 admin audit 记录（action 显式区分）。
/// 通过 GET /api/admin/updates/history 反查（无需启用 updater）。
#[tokio::test]
async fn it_admin_sensitive_actions_write_audit_log() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;
    let _user_token = login_and_get_token(&app.app).await;

    // 取一个真实存在的用户 ID
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/users?page=1&perPage=10",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (_, _, body) = response_json(resp).await;
    let target_user_id = body["data"]["data"][0]["id"].as_str().unwrap().to_string();

    // ban → unban → reset-password → set-password
    for (method, path, payload) in [
        (
            Method::POST,
            format!("/api/admin/users/{target_user_id}/ban"),
            None,
        ),
        (
            Method::POST,
            format!("/api/admin/users/{target_user_id}/unban"),
            None,
        ),
        (
            Method::POST,
            format!("/api/admin/users/{target_user_id}/reset-password"),
            None,
        ),
        (
            Method::POST,
            format!("/api/admin/users/{target_user_id}/set-password"),
            Some(serde_json::json!({ "newPassword": "Strongp4ssword!" })),
        ),
    ] {
        let resp = request(
            &app.app,
            method,
            &path,
            payload,
            &[("authorization", auth_header(&admin_token))],
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK, "{path} 应 200");
    }

    // 查 history（updater 未启用也可访问，handler 直接读 store）
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/updates/history",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let entries = body["data"]["entries"].as_array().expect("entries 数组");

    // 期望恰好 4 条新 audit（admin 还未触发任何 self_update）
    let admin_actions: Vec<&str> = entries
        .iter()
        .filter_map(|e| e["action"].as_str())
        .filter(|a| *a != "self_update")
        .collect();
    assert!(
        admin_actions.contains(&"user.ban"),
        "缺 user.ban：{admin_actions:?}"
    );
    assert!(
        admin_actions.contains(&"user.unban"),
        "缺 user.unban：{admin_actions:?}"
    );
    assert!(
        admin_actions.contains(&"user.reset_password"),
        "缺 user.reset_password：{admin_actions:?}"
    );
    assert!(
        admin_actions.contains(&"user.set_password"),
        "缺 user.set_password：{admin_actions:?}"
    );

    // 每条 audit 都应有 targetType=user / targetId=目标 / outcome=success
    for e in entries.iter().filter(|e| e["action"] != "self_update") {
        assert_eq!(e["targetType"], "user");
        assert_eq!(e["targetId"], target_user_id);
        assert_eq!(e["outcome"], "success");
    }
}

#[tokio::test]
async fn it_admin_top_level_unauthorized() {
    let app = spawn_test_server().await;
    for path in [
        "/api/admin/stats",
        "/api/admin/users",
    ] {
        let resp = request(&app.app, Method::GET, path, None, &[]).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "path {path}");
    }
}

// ────────────────────── admin/feedback (M1-G3) ──────────────────────

#[tokio::test]
async fn it_admin_feedback_patch_status_switch() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    // 用户先提交一条 feedback
    let resp = request(
        &app.app,
        axum::http::Method::POST,
        "/api/feedback",
        Some(serde_json::json!({
            "category": "bug",
            "body": "Something is broken on the dashboard.",
            "route": "/dashboard"
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let feedback_id = body["data"]["id"].as_str().unwrap().to_string();

    // admin 列表中能看到，默认 status=open、priority=normal
    let resp = request(
        &app.app,
        axum::http::Method::GET,
        "/api/admin/feedback",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["data"][0]["status"], "open");
    assert_eq!(body["data"]["data"][0]["priority"], "normal");

    // admin 切换 status → in_progress
    let resp = request(
        &app.app,
        axum::http::Method::PATCH,
        &format!("/api/admin/feedback/{}", feedback_id),
        Some(serde_json::json!({ "status": "in_progress", "priority": "high" })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, axum::http::StatusCode::OK, "PATCH feedback: {body}");
    assert_eq!(body["status"], "in_progress");
    assert_eq!(body["priority"], "high");

    // admin 切换 status → resolved，并附 resolution
    let resp = request(
        &app.app,
        axum::http::Method::PATCH,
        &format!("/api/admin/feedback/{}", feedback_id),
        Some(serde_json::json!({
            "status": "resolved",
            "resolution": "Fixed in v1.1"
        })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, axum::http::StatusCode::OK, "PATCH resolve: {body}");
    assert_eq!(body["status"], "resolved");
    assert_eq!(body["resolution"], "Fixed in v1.1");
    assert!(body["resolvedAt"].is_string(), "resolvedAt 应在 resolved 时被写入");

    // 状态过滤：resolved 应可查到
    let resp = request(
        &app.app,
        axum::http::Method::GET,
        "/api/admin/feedback?status=resolved",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(body["data"]["total"], 1);

    // 状态过滤：open 应为 0
    let resp = request(
        &app.app,
        axum::http::Method::GET,
        "/api/admin/feedback?status=open",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(body["data"]["total"], 0);
}

#[tokio::test]
async fn it_admin_feedback_patch_invalid_status_rejected() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        axum::http::Method::POST,
        "/api/feedback",
        Some(serde_json::json!({ "body": "test feedback" })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (_, _, body) = response_json(resp).await;
    let feedback_id = body["data"]["id"].as_str().unwrap().to_string();

    // 无效 status 应被拒绝
    let resp = request(
        &app.app,
        axum::http::Method::PATCH,
        &format!("/api/admin/feedback/{}", feedback_id),
        Some(serde_json::json!({ "status": "wont_fix" })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn it_admin_feedback_patch_nonexistent_returns_404() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        axum::http::Method::PATCH,
        "/api/admin/feedback/no-such-id",
        Some(serde_json::json!({ "status": "closed" })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
}
