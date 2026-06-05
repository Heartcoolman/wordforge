mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token, setup_admin_and_get_token};
use common::fixtures::seed_user;
use common::http::{assert_json_error, request, response_json};

// ── POST /api/admin/broadcast/ (broadcast_message) ──

#[tokio::test]
async fn broadcast_message_success_with_users() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;

    // 预置两个普通用户，广播会向其落库通知
    seed_user(app.state.store(), "bc-a@test.com", "bc-a", "Passw0rd!");
    seed_user(app.state.store(), "bc-b@test.com", "bc-b", "Passw0rd!");

    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/broadcast",
        Some(serde_json::json!({
            "title": "系统维护通知",
            "message": "今晚 22:00 进行例行维护",
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;

    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["sent"], 2);
    assert!(
        body["data"]["broadcastId"].as_str().is_some(),
        "broadcastId 应为字符串: {body}"
    );
}

#[tokio::test]
async fn broadcast_message_success_with_no_users() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;

    // 全新服务器无普通用户（admin 走独立表，不计为 user），sent 应为 0
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/broadcast",
        Some(serde_json::json!({
            "title": "空受众广播",
            "message": "无人接收",
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;

    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["data"]["sent"], 0);
}

#[tokio::test]
async fn broadcast_message_rejects_empty_title() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/broadcast",
        Some(serde_json::json!({
            "title": "",
            "message": "内容正常",
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;

    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_json_error(&body, "INVALID_TITLE");
}

#[tokio::test]
async fn broadcast_message_rejects_overlong_title() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;

    let long_title = "标".repeat(201);
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/broadcast",
        Some(serde_json::json!({
            "title": long_title,
            "message": "内容正常",
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;

    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_json_error(&body, "INVALID_TITLE");
}

#[tokio::test]
async fn broadcast_message_rejects_empty_message() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/broadcast",
        Some(serde_json::json!({
            "title": "标题正常",
            "message": "",
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;

    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_json_error(&body, "INVALID_MESSAGE");
}

#[tokio::test]
async fn broadcast_message_rejects_overlong_message() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;

    let long_message = "x".repeat(10001);
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/broadcast",
        Some(serde_json::json!({
            "title": "标题正常",
            "message": long_message,
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;

    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_json_error(&body, "INVALID_MESSAGE");
}

#[tokio::test]
async fn broadcast_message_rejects_missing_field() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;

    // 缺少 message 字段 → JsonBody 反序列化失败 → INVALID_REQUEST_BODY
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/broadcast",
        Some(serde_json::json!({
            "title": "只有标题",
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;

    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_json_error(&body, "INVALID_REQUEST_BODY");
}

#[tokio::test]
async fn broadcast_message_requires_admin_token() {
    let app = spawn_test_server().await;

    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/broadcast",
        Some(serde_json::json!({
            "title": "标题",
            "message": "内容",
        })),
        &[],
    )
    .await;

    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "body={body}");
    assert_eq!(body["success"], false);
}

#[tokio::test]
async fn broadcast_message_rejects_non_admin_token() {
    let app = spawn_test_server().await;
    // 普通用户 token（token_type != "admin"）应被 admin 鉴权拒绝
    let user_token = login_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/broadcast",
        Some(serde_json::json!({
            "title": "标题",
            "message": "内容",
        })),
        &[("authorization", auth_header(&user_token))],
    )
    .await;

    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "body={body}");
    assert_eq!(body["success"], false);
}

// ── POST /api/admin/broadcast-update (broadcast_update) ──

#[tokio::test]
async fn broadcast_update_success_with_version_and_message() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/broadcast-update",
        Some(serde_json::json!({
            "version": "1.2.0",
            "message": "新版本可用",
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;

    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["broadcasted"], true);
}

#[tokio::test]
async fn broadcast_update_success_with_empty_body() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;

    // version / message 均为 Option，空对象合法
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/broadcast-update",
        Some(serde_json::json!({})),
        &[("authorization", auth_header(&token))],
    )
    .await;

    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["data"]["broadcasted"], true);
}

#[tokio::test]
async fn broadcast_update_requires_admin_token() {
    let app = spawn_test_server().await;

    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/broadcast-update",
        Some(serde_json::json!({ "version": "1.2.0" })),
        &[],
    )
    .await;

    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "body={body}");
    assert_eq!(body["success"], false);
}

#[tokio::test]
async fn broadcast_update_rejects_non_admin_token() {
    let app = spawn_test_server().await;
    let user_token = login_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/broadcast-update",
        Some(serde_json::json!({ "version": "1.2.0" })),
        &[("authorization", auth_header(&user_token))],
    )
    .await;

    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "body={body}");
    assert_eq!(body["success"], false);
}
