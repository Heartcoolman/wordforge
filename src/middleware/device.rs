use axum::extract::State;
use axum::http::{HeaderValue, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::auth::{extract_token_from_headers, verify_jwt};
use crate::state::AppState;

/// m027:X-Upgrade-Hint 响应头取值。老客户端不读不受影响,新 SDK 据此显示横幅 / 强制升级窗。
const HINT_REQUIRED: &str = "required";
const HINT_SUGGESTED: &str = "suggested";
const HINT_NONE: &str = "none";

/// m027:从 headers 提取 client IP。同 routes/auth.rs 实现,本仓库目前仅两处用,
/// 不抽公共模块避免跨 mod 引用扩散。
fn extract_client_ip(headers: &axum::http::HeaderMap) -> Option<String> {
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = xff.split(',').next() {
            let s = first.trim();
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// m027:semver "<" 比较;任一边 parse 失败返 false(降级为"无 hint")。
/// 接受带 v 前缀("v0.7.3")与不带的两种,内部统一去 v。
fn version_lt(actual: &str, threshold: &str) -> bool {
    let a = actual.trim_start_matches('v');
    let t = threshold.trim_start_matches('v');
    match (semver::Version::parse(a), semver::Version::parse(t)) {
        (Ok(av), Ok(tv)) => av < tv,
        _ => false,
    }
}

/// m027:据 policy 比较 app_version,算 X-Upgrade-Hint 取值。
/// 任一缺失返 None(不塞 header)。
fn compute_upgrade_hint(
    policy: Option<&crate::store::operations::clients::ClientUpgradePolicy>,
    app_version: Option<&str>,
) -> Option<&'static str> {
    let p = policy?;
    let v = app_version?;
    if let Some(min) = p.min_version.as_deref() {
        if version_lt(v, min) {
            return Some(HINT_REQUIRED);
        }
    }
    if let Some(sug) = p.suggested_version.as_deref() {
        if version_lt(v, sug) {
            return Some(HINT_SUGGESTED);
        }
    }
    Some(HINT_NONE)
}

pub async fn device_middleware(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    if req.uri().path().starts_with("/api/admin/") {
        return next.run(req).await;
    }

    // m038:遥测主上报端点的设备注册/归属/型号落库收归 handler,在硬识别核验通过后再写;
    // 中间件此处跳过 upsert——否则会在核验前无条件覆盖 owner,使 handler 的归属核验失效
    // 并污染受害者设备。ban 检查与 upgrade hint 仍生效。
    // 该中间件 layer 挂在 `/api` nest 之内,axum 已剥掉 `/api` 前缀,故此处看到的是
    // `/telemetry`(及带尾斜杠的 `/telemetry/`);两种形态都路由到 submit,均需跳过。
    let tele_path = req.uri().path();
    let skip_telemetry_upsert = tele_path == "/telemetry" || tele_path == "/telemetry/";

    let device_id = req
        .headers()
        .get("x-device-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let mut upgrade_hint: Option<&'static str> = None;

    if let Some(ref did) = device_id {
        let banned_check = {
            let did = did.clone();
            state
                .run_store_task("middleware.device.is_device_banned", move |store| {
                    store.is_device_banned(&did)
                })
                .await
        };

        match banned_check {
            Ok(Ok(true)) => {
                return (
                    StatusCode::FORBIDDEN,
                    axum::Json(serde_json::json!({
                        "code": "CLIENT_BANNED",
                        "message": "设备已被封禁"
                    })),
                )
                    .into_response();
            }
            Ok(Ok(false)) => {}
            Ok(Err(e)) => {
                tracing::error!(error = %e, device_id = %did, "Failed to check device ban");
            }
            Err(e) => {
                tracing::error!(error = %e, device_id = %did, "Device ban task failed");
            }
        }

        let platform = req
            .headers()
            .get("x-device-platform")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown");

        // m022:`x-app-version` 是可选头(老客户端不带),首次出现时落库;后续请求漏带
        // 时 upsert 用 COALESCE 保留 DB 已有值,不会被清空。
        let app_version = req
            .headers()
            .get("x-app-version")
            .and_then(|v| v.to_str().ok())
            .filter(|s| !s.is_empty() && s.len() <= 64)
            .map(String::from);

        // m027:据缓存 policy 算 X-Upgrade-Hint(任一字段缺都返 None)。
        if let Some(av) = app_version.as_deref() {
            let policies = state.get_upgrade_policies_cached();
            upgrade_hint = compute_upgrade_hint(policies.get(platform), Some(av));
        }

        // m027:IP → country。GeoIP 缺失或 IP 私网/未知都返 None,country 字段保持 NULL。
        let client_ip = extract_client_ip(req.headers());
        let country = client_ip
            .as_deref()
            .and_then(|s| s.parse::<std::net::IpAddr>().ok())
            .and_then(|ip| {
                state
                    .geoip()
                    .and_then(|r| crate::services::geoip::lookup_country(r, ip))
            });

        let user_id = extract_token_from_headers(req.headers())
            .ok()
            .and_then(|token| verify_jwt(&token, &state.config().jwt_secret).ok())
            .filter(|c| c.token_type == "user")
            .map(|c| c.sub);

        if let (Some(uid), false) = (user_id.as_ref(), skip_telemetry_upsert) {
            let upsert = {
                let did = did.clone();
                let platform = platform.to_string();
                let uid = uid.clone();
                let app_version = app_version.clone();
                let country = country.clone();
                let client_ip = client_ip.clone();
                state
                    .run_store_task("middleware.device.upsert_client_device", move |store| {
                        store.upsert_client_device_with_extras(
                            &did,
                            &platform,
                            &uid,
                            app_version.as_deref(),
                            country.as_deref(),
                            client_ip.as_deref(),
                            None, // model:中间件无 payload,型号由 telemetry handler 落库
                        )
                    })
                    .await
            };

            match upsert {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    tracing::error!(error = %e, "Failed to upsert client device");
                }
                Err(e) => {
                    tracing::error!(error = %e, "Client device upsert task failed");
                }
            }
        }
    }

    let mut resp = next.run(req).await;

    // m027:hint 在 next 前算好,run 之后塞。失败时(HeaderValue::from_static)直接丢弃。
    if let Some(hint) = upgrade_hint {
        resp.headers_mut()
            .insert("x-upgrade-hint", HeaderValue::from_static(hint));
    }

    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::amas::config::AMASConfig;
    use crate::amas::engine::AMASEngine;
    use crate::auth::sign_jwt_for_user;
    use crate::config::Config;
    use crate::store::Store;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use axum::Router;
    use std::sync::Arc;
    use tokio::sync::broadcast;
    use tower::ServiceExt;

    fn ensure_secrets() {
        let secret = "test_secret_that_is_at_least_32_characters_long_ok";
        std::env::set_var("JWT_SECRET", secret);
        std::env::set_var("ADMIN_JWT_SECRET", secret);
        std::env::set_var("REFRESH_JWT_SECRET", secret);
    }

    async fn build_state() -> (AppState, tempfile::TempDir) {
        ensure_secrets();
        let cfg = Config::from_env();
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(
            Store::open(tmp.path().join("device_mw.db").to_str().unwrap(), 5000, 4).unwrap(),
        );
        store.run_migrations().unwrap();
        let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
        let (tx, _) = broadcast::channel(4);
        let state = AppState::new(store, amas, &cfg, tx, false);
        (state, tmp)
    }

    fn build_router(state: AppState) -> Router {
        Router::new()
            .route("/api/ping", get(|| async { "pong" }))
            .route("/api/admin/x", get(|| async { "admin-ok" }))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                device_middleware,
            ))
            .with_state(state)
    }

    #[tokio::test]
    async fn admin_path_skips_middleware() {
        let (state, _tmp) = build_state().await;
        let app = build_router(state);
        let req = Request::builder()
            .uri("/api/admin/x")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn no_device_id_passes_through() {
        let (state, _tmp) = build_state().await;
        let app = build_router(state);
        let req = Request::builder()
            .uri("/api/ping")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn unbanned_device_passes_through() {
        let (state, _tmp) = build_state().await;
        let app = build_router(state);
        let req = Request::builder()
            .uri("/api/ping")
            .header("x-device-id", "device-unbanned")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn banned_device_is_rejected() {
        let (state, _tmp) = build_state().await;
        state
            .store()
            .upsert_client_device("dev-banned", "ios", "user-1")
            .unwrap();
        state
            .store()
            .ban_client_device("dev-banned", "admin", Some("test"))
            .unwrap();

        let app = build_router(state);
        let req = Request::builder()
            .uri("/api/ping")
            .header("x-device-id", "dev-banned")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "CLIENT_BANNED");
    }

    #[tokio::test]
    async fn authenticated_user_triggers_device_upsert() {
        let (state, _tmp) = build_state().await;
        let secret = state.config().jwt_secret.clone();
        let token = sign_jwt_for_user("user-token-1", &secret, 1).unwrap();

        let app = build_router(state.clone());
        let req = Request::builder()
            .uri("/api/ping")
            .header("x-device-id", "dev-auth")
            .header("x-device-platform", "android")
            .header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // 设备已被 upsert 到 client_devices
        let exists = state.store().client_device_exists("dev-auth").unwrap();
        assert!(exists);
    }

    #[tokio::test]
    async fn invalid_token_does_not_upsert_but_passes() {
        let (state, _tmp) = build_state().await;
        let app = build_router(state.clone());
        let req = Request::builder()
            .uri("/api/ping")
            .header("x-device-id", "dev-no-upsert")
            .header(axum::http::header::AUTHORIZATION, "Bearer not-a-token")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        // 没有有效 token，未 upsert
        let exists = state.store().client_device_exists("dev-no-upsert").unwrap();
        assert!(!exists);
    }

    #[tokio::test]
    async fn admin_token_does_not_trigger_upsert() {
        let (state, _tmp) = build_state().await;
        let secret = state.config().jwt_secret.clone();
        // admin token (token_type="admin") 应被 filter 掉
        let admin_token = crate::auth::sign_jwt_for_admin("admin-1", &secret, 1).unwrap();
        let app = build_router(state.clone());
        let req = Request::builder()
            .uri("/api/ping")
            .header("x-device-id", "dev-admin")
            .header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {admin_token}"),
            )
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let exists = state.store().client_device_exists("dev-admin").unwrap();
        assert!(!exists);
    }
}
