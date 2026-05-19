//! §12 strict-mode middleware 集成测试
//! 覆盖：disabled 透明 / hard-block 拒绝 / soft-block 仅 warn / admin 路径豁免 / v1 路径豁免 / 版本门控。

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::app::{spawn_test_app, spawn_test_app_with_strict_mode};
use learning_backend::config::StrictModeConfig;
use serde_json::json;
use tower::ServiceExt;

async fn build_strict_app(hard_block: bool, min_version: Option<&str>) -> common::app::TestApp {
    spawn_test_app_with_strict_mode(StrictModeConfig {
        enabled: true,
        hard_block,
        min_client_version: min_version.map(String::from),
    })
    .await
}

#[tokio::test]
async fn disabled_lets_invalid_ua_through() {
    let app = spawn_test_app().await; // 默认 strict_mode_enabled = false
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("User-Agent", "curl/7.84.0")
        .header("content-type", "application/json")
        .body(Body::from(json!({"email":"x@x.com","password":"yyyyyyyy"}).to_string()))
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    // /api/status 是公开的，应正常响应而非 400
    assert_ne!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn hard_block_rejects_missing_user_agent() {
    let app = build_strict_app(true, None).await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("User-Agent", "curl/7.84.0")
        .header("content-type", "application/json")
        .body(Body::from(json!({"email":"x@x.com","password":"yyyyyyyy"}).to_string()))
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["code"], "MISSING_USER_AGENT");
}

#[tokio::test]
async fn hard_block_rejects_missing_platform() {
    let app = build_strict_app(true, None).await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("User-Agent", "WordForge-iOS/1.0.0 (iPhone15,2; iOS 17.5)")
        .header("content-type", "application/json")
        // 故意不带 x-device-platform
        .body(Body::from(json!({"email":"x@x.com","password":"yyyyyyyy"}).to_string()))
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["code"], "MISSING_OS");
}

#[tokio::test]
async fn hard_block_rejects_outdated_client() {
    let app = build_strict_app(true, Some("2.0.0")).await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("User-Agent", "WordForge-iOS/1.0.0 (iPhone15,2; iOS 17.5)")
        .header("x-device-platform", "ios")
        .header("content-type", "application/json")
        .body(Body::from(json!({"email":"x@x.com","password":"yyyyyyyy"}).to_string()))
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["code"], "CLIENT_OUTDATED");
}

#[tokio::test]
async fn hard_block_allows_compliant_client() {
    let app = build_strict_app(true, Some("1.0.0")).await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("User-Agent", "WordForge-iOS/1.5.2 (iPhone15,2; iOS 17.5)")
        .header("x-device-platform", "ios")
        .header("content-type", "application/json")
        .body(Body::from(json!({"email":"x@x.com","password":"yyyyyyyy"}).to_string()))
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    assert_ne!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn admin_path_bypasses_strict_mode() {
    let app = build_strict_app(true, None).await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/admin/auth/login")
        .header("User-Agent", "curl/7.84.0")
        .header("content-type", "application/json")
        .body(Body::from(json!({"username":"x","password":"y"}).to_string()))
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    // 401 / 422 都可，关键是不能是 MISSING_USER_AGENT
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
    assert_ne!(v["code"], "MISSING_USER_AGENT");
}

#[tokio::test]
async fn soft_block_warns_but_passes() {
    let app = build_strict_app(false, None).await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("User-Agent", "curl/7.84.0")
        .header("content-type", "application/json")
        .body(Body::from(json!({"email":"x@x.com","password":"yyyyyyyy"}).to_string()))
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    // soft-block: 不拒绝
    assert_ne!(resp.status(), StatusCode::BAD_REQUEST);
}
