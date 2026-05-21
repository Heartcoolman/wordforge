//! M1-A6：strict-mode / maintenance 豁免路由元数据驱动 — 集成验收测试
//!
//! 覆盖场景：
//! - admin SSE / `/api/v1/*` / `/api/status` / `/api/realtime/events` / `/api/telemetry`
//!   在 strict-mode hard-block + maintenance 下行为符合 RouteMetadata 声明
//! - 普通 `/api/auth/login` 在 maintenance 下返回 503
//! - 普通 `/api/auth/login` 在 strict-mode hard-block 下被拦截

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::app::{spawn_test_app, spawn_test_app_with_strict_mode};
use learning_backend::config::StrictModeConfig;
use serde_json::json;
use tower::ServiceExt;

/// 构造 strict-mode hard-block 测试 App（无合法 UA，触发拦截）
async fn strict_app() -> common::app::TestApp {
    spawn_test_app_with_strict_mode(StrictModeConfig {
        enabled: true,
        hard_block: true,
        min_client_version: None,
    })
    .await
}

// ── strict-mode 豁免 ────────────────────────────────────────────────────────

/// /api/admin/* 路由注入 strict_mode_exempt=true，非法 UA 也应放行
#[tokio::test]
async fn strict_mode_exempts_admin_routes() {
    let app = strict_app().await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/admin/auth/login")
        .header("User-Agent", "curl/7.84.0")
        .header("content-type", "application/json")
        .body(Body::from(json!({"username":"x","password":"y"}).to_string()))
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
    assert_ne!(v["code"], "MISSING_USER_AGENT", "admin 路由应豁免 strict-mode");
    assert_ne!(v["code"], "MISSING_OS", "admin 路由应豁免 strict-mode");
}

/// /api/status 注入 strict_mode_exempt=true
#[tokio::test]
async fn strict_mode_exempts_status() {
    let app = strict_app().await;
    let req = Request::builder()
        .method("GET")
        .uri("/api/status")
        .header("User-Agent", "Mozilla/5.0 Chrome/148.0.0.0")
        .body(Body::empty())
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
    assert_ne!(v["code"], "MISSING_USER_AGENT", "/status 应豁免 strict-mode");
}

/// /api/realtime/events 注入 strict_mode_exempt=true
#[tokio::test]
async fn strict_mode_exempts_realtime_events() {
    let app = strict_app().await;
    let req = Request::builder()
        .method("GET")
        .uri("/api/realtime/events")
        .header("User-Agent", "Mozilla/5.0 Chrome/148.0.0.0")
        .body(Body::empty())
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
    // 业务上 401（无 token），但绝不应是 MISSING_USER_AGENT
    assert_ne!(v["code"], "MISSING_USER_AGENT", "/realtime/events 应豁免 strict-mode");
    assert_ne!(v["code"], "MISSING_OS", "/realtime/events 应豁免 strict-mode");
}

/// /api/telemetry 注入 strict_mode_exempt=true
#[tokio::test]
async fn strict_mode_exempts_telemetry() {
    let app = strict_app().await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/telemetry")
        .header("User-Agent", "curl/7.84.0")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
    assert_ne!(v["code"], "MISSING_USER_AGENT", "/telemetry 应豁免 strict-mode");
}

/// /api/v1/* 注入 strict_mode_exempt=true（全部 410，无客户端契约校验意义）
#[tokio::test]
async fn strict_mode_exempts_v1_routes() {
    let app = strict_app().await;
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/words")
        .header("User-Agent", "curl/7.84.0")
        .body(Body::empty())
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    // 应该是 410 Gone，而不是 400 MISSING_USER_AGENT
    assert_eq!(resp.status(), StatusCode::GONE, "/api/v1/* 应豁免 strict-mode，直接 410");
}

/// 普通路由（/api/auth/login）在 strict-mode hard-block 下被拦截——证明豁免是选择性的
#[tokio::test]
async fn strict_mode_blocks_non_exempt_route() {
    let app = strict_app().await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("User-Agent", "curl/7.84.0")
        .header("content-type", "application/json")
        .body(Body::from(json!({"email":"x@x.com","password":"yyyyyyyy"}).to_string()))
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "/api/auth/login 不豁免，应被 strict-mode 拦截");
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["code"], "MISSING_USER_AGENT");
}

// ── maintenance 豁免 ────────────────────────────────────────────────────────

/// 在维护模式下，普通路由返回 503
#[tokio::test]
async fn maintenance_blocks_normal_routes() {
    let app = spawn_test_app().await;
    app.state.set_maintenance(true);
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("User-Agent", "WordForge-iOS/1.0.0")
        .header("x-device-platform", "ios")
        .header("content-type", "application/json")
        .body(Body::from(json!({"email":"x@x.com","password":"yyyyyyyy"}).to_string()))
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE, "维护模式下普通路由应返回 503");
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["code"], "MAINTENANCE");
}

/// /api/status 在维护模式下仍正常响应
#[tokio::test]
async fn maintenance_exempts_status() {
    let app = spawn_test_app().await;
    app.state.set_maintenance(true);
    let req = Request::builder()
        .method("GET")
        .uri("/api/status")
        .body(Body::empty())
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    assert_ne!(resp.status(), StatusCode::SERVICE_UNAVAILABLE, "/status 应豁免 maintenance");
}

/// /api/admin/* 在维护模式下仍可访问
#[tokio::test]
async fn maintenance_exempts_admin_routes() {
    let app = spawn_test_app().await;
    app.state.set_maintenance(true);
    let req = Request::builder()
        .method("POST")
        .uri("/api/admin/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(json!({"username":"x","password":"y"}).to_string()))
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    assert_ne!(resp.status(), StatusCode::SERVICE_UNAVAILABLE, "admin 路由应豁免 maintenance");
}

/// /api/realtime/events 在维护模式下仍可访问（SSE 通道，推送 maintenance 事件）
#[tokio::test]
async fn maintenance_exempts_realtime_events() {
    let app = spawn_test_app().await;
    app.state.set_maintenance(true);
    let req = Request::builder()
        .method("GET")
        .uri("/api/realtime/events")
        .body(Body::empty())
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    assert_ne!(resp.status(), StatusCode::SERVICE_UNAVAILABLE, "/realtime/events 应豁免 maintenance");
}

/// /api/telemetry 在维护模式下仍可上报（维护期间也要采集设备数据）
#[tokio::test]
async fn maintenance_exempts_telemetry() {
    let app = spawn_test_app().await;
    app.state.set_maintenance(true);
    let req = Request::builder()
        .method("POST")
        .uri("/api/telemetry")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    assert_ne!(resp.status(), StatusCode::SERVICE_UNAVAILABLE, "/telemetry 应豁免 maintenance");
}

/// /api/v1/* 在维护模式下仍返回 410（v1 全部 gone，不应被 maintenance 拦截为 503）
#[tokio::test]
async fn maintenance_exempts_v1_routes() {
    let app = spawn_test_app().await;
    app.state.set_maintenance(true);
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/words")
        .body(Body::empty())
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::GONE, "/api/v1/* 应豁免 maintenance，直接 410");
}
