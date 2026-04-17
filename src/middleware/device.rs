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
                state
                    .run_store_task("middleware.device.upsert_client_device", move |store| {
                        store.upsert_client_device(&did, &platform, &uid)
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
