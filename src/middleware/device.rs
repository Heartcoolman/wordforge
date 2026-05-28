use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::auth::{extract_token_from_headers, verify_jwt};
use crate::state::AppState;

pub async fn device_middleware(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    if req.uri().path().starts_with("/api/admin/") {
        return next.run(req).await;
    }

    let device_id = req
        .headers()
        .get("x-device-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

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

        let user_id = extract_token_from_headers(req.headers())
            .ok()
            .and_then(|token| verify_jwt(&token, &state.config().jwt_secret).ok())
            .filter(|c| c.token_type == "user")
            .map(|c| c.sub);

        if let Some(ref uid) = user_id {
            let upsert = {
                let did = did.clone();
                let platform = platform.to_string();
                let uid = uid.clone();
                let app_version = app_version.clone();
                state
                    .run_store_task("middleware.device.upsert_client_device", move |store| {
                        store.upsert_client_device_with_version(
                            &did,
                            &platform,
                            &uid,
                            app_version.as_deref(),
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

    next.run(req).await
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
            Store::open(
                tmp.path().join("device_mw.db").to_str().unwrap(),
                5000,
                4,
            )
            .unwrap(),
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
            .header(
                axum::http::header::AUTHORIZATION,
                format!("Bearer {token}"),
            )
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
        let admin_token =
            crate::auth::sign_jwt_for_admin("admin-1", &secret, 1).unwrap();
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
