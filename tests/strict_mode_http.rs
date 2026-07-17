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
        .body(Body::from(
            json!({"email":"x@x.com","password":"yyyyyyyy"}).to_string(),
        ))
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
        .body(Body::from(
            json!({"email":"x@x.com","password":"yyyyyyyy"}).to_string(),
        ))
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
        .body(Body::from(
            json!({"email":"x@x.com","password":"yyyyyyyy"}).to_string(),
        ))
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
        .body(Body::from(
            json!({"email":"x@x.com","password":"yyyyyyyy"}).to_string(),
        ))
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
        .body(Body::from(
            json!({"email":"x@x.com","password":"yyyyyyyy"}).to_string(),
        ))
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
        .body(Body::from(
            json!({"username":"x","password":"y"}).to_string(),
        ))
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
        .body(Body::from(
            json!({"email":"x@x.com","password":"yyyyyyyy"}).to_string(),
        ))
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    // soft-block: 不拒绝
    assert_ne!(resp.status(), StatusCode::BAD_REQUEST);
}

/// v0.5.2 加固：`/status` 是公共版本探测端点，hard-block 启用时也不应被拦
#[tokio::test]
async fn hard_block_bypasses_status_endpoint() {
    let app = build_strict_app(true, None).await;
    let req = Request::builder()
        .method("GET")
        .uri("/api/status")
        // 浏览器原生 UA：strict-mode 解析器不认，但 /status 应豁免
        .header(
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/148.0.0.0",
        )
        .body(Body::empty())
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    assert_ne!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "/status 应豁免 strict-mode 拦截"
    );
}

/// v0.5.2 加固：`/realtime/events` 是 SSE 通道，admin 浏览器直连，hard-block 不能拦
#[tokio::test]
async fn hard_block_bypasses_realtime_events() {
    let app = build_strict_app(true, None).await;
    let req = Request::builder()
        .method("GET")
        .uri("/api/realtime/events")
        .header(
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/148.0.0.0",
        )
        .body(Body::empty())
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
    // 业务上可能 401（无 token），但绝不应是 MISSING_USER_AGENT
    assert_ne!(
        v["code"], "MISSING_USER_AGENT",
        "/realtime/events 应豁免 strict-mode 拦截"
    );
    assert_ne!(
        v["code"], "MISSING_OS",
        "/realtime/events 应豁免 strict-mode 拦截"
    );
}

/// 安全加固回归：仅带 x-device-platform: web（无 sec-fetch-mode，也无 User-Agent）不再能豁免
/// UA 契约——此前任何客户端（旧 native / 裸 curl）发这一个头就能绕过 MISSING_USER_AGENT。
#[tokio::test]
async fn hard_block_rejects_spoofed_web_platform_without_fetch_metadata() {
    let app = build_strict_app(true, None).await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("x-device-platform", "web")
        // 故意不带 User-Agent，也不带 sec-fetch-mode——模拟仅伪造平台头的旧客户端/脚本。
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"email":"x@x.com","password":"yyyyyyyy"}).to_string(),
        ))
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["code"], "MISSING_USER_AGENT");
}

/// 真实浏览器请求（x-device-platform: web + sec-fetch-mode，二者均由浏览器强制自动附带、
/// JS 无法覆盖）仍应照常豁免 UA 契约，不能因加固而误伤合法 web 端流量。
#[tokio::test]
async fn hard_block_allows_genuine_web_without_user_agent() {
    let app = build_strict_app(true, None).await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("x-device-platform", "web")
        .header("sec-fetch-mode", "cors")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"email":"x@x.com","password":"yyyyyyyy"}).to_string(),
        ))
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
    assert_ne!(v["code"], "MISSING_USER_AGENT");
    assert_ne!(v["code"], "MISSING_OS");
}

/// 强升切流下的同一条加固：伪造 web 平台头（无 sec-fetch-mode）不能绕过 CLIENT_OUTDATED。
#[tokio::test]
async fn runtime_version_gate_rejects_spoofed_web_platform() {
    let app = spawn_test_app().await;
    {
        let mut s = app.state.store().get_system_settings().unwrap();
        s.version_gate_enabled = true;
        s.min_client_version = Some("2.0.0".into());
        app.state.store().save_system_settings(&s).unwrap();
        app.state.refresh_version_gate();
    }
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("x-device-platform", "web")
        // 无 sec-fetch-mode：不应被当成真实 web 流量豁免版本门控。
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"email":"x@x.com","password":"yyyyyyyy"}).to_string(),
        ))
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["code"], "CLIENT_OUTDATED");
}

// ─────────────── D4:运行时版本门控（settings 驱动，独立于 strict-mode 开关） ───────────────

/// strict-mode 整体关闭时，仅靠 system_settings 的版本门控开关即可拒绝旧客户端。
/// 验证「门控独立于 cfg.enabled」+「settings 优先于 env」+「即时硬拒绝」。
#[tokio::test]
async fn runtime_version_gate_blocks_outdated_when_strict_mode_disabled() {
    let app = spawn_test_app().await; // strict_mode_enabled = false
                                      // admin 运行时开门控 + 设阈值
    {
        let mut s = app.state.store().get_system_settings().unwrap();
        s.version_gate_enabled = true;
        s.min_client_version = Some("2.0.0".into());
        app.state.store().save_system_settings(&s).unwrap();
        // 模拟写端点刷新运行时快照（即时生效）
        app.state.refresh_version_gate();
    }
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("User-Agent", "WordForge-iOS/1.0.0 (iPhone15,2; iOS 17.5)")
        .header("x-device-platform", "ios")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"email":"x@x.com","password":"yyyyyyyy"}).to_string(),
        ))
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["code"], "CLIENT_OUTDATED");
}

/// 门控开关关闭时（即使设了阈值），旧客户端放行；strict-mode 也关闭故 UA 校验不触发。
#[tokio::test]
async fn runtime_version_gate_disabled_lets_outdated_through() {
    let app = spawn_test_app().await;
    {
        let mut s = app.state.store().get_system_settings().unwrap();
        s.version_gate_enabled = false;
        s.min_client_version = Some("2.0.0".into());
        app.state.store().save_system_settings(&s).unwrap();
        app.state.refresh_version_gate();
    }
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("User-Agent", "WordForge-iOS/1.0.0 (iPhone15,2; iOS 17.5)")
        .header("x-device-platform", "ios")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"email":"x@x.com","password":"yyyyyyyy"}).to_string(),
        ))
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    assert_ne!(resp.status(), StatusCode::BAD_REQUEST);
}

/// strict-mode 启用时，settings 的 min_client_version 覆盖 env 兜底（settings 优先）。
#[tokio::test]
async fn runtime_version_gate_settings_override_env() {
    // env 兜底 1.0.0；settings 拉高到 3.0.0
    let app = build_strict_app(true, Some("1.0.0")).await;
    {
        let mut s = app.state.store().get_system_settings().unwrap();
        s.min_client_version = Some("3.0.0".into());
        app.state.store().save_system_settings(&s).unwrap();
        app.state.refresh_version_gate();
    }
    // 2.0.0 高于 env 1.0.0 但低于 settings 3.0.0 → 应被拒
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("User-Agent", "WordForge-iOS/2.0.0 (iPhone15,2; iOS 17.5)")
        .header("x-device-platform", "ios")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"email":"x@x.com","password":"yyyyyyyy"}).to_string(),
        ))
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["code"], "CLIENT_OUTDATED");
}

/// admin updates apply / status 路径也应畅通（admin 豁免路径已有，加 explicit 测试覆盖）
#[tokio::test]
async fn hard_block_bypasses_admin_updates_status() {
    let app = build_strict_app(true, None).await;
    let req = Request::builder()
        .method("GET")
        .uri("/api/admin/updates/status")
        .header("User-Agent", "Mozilla/5.0 Chrome/148.0.0.0")
        .body(Body::empty())
        .unwrap();
    let resp = app.app.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
    assert_ne!(v["code"], "MISSING_USER_AGENT");
    assert_ne!(v["code"], "MISSING_OS");
}
