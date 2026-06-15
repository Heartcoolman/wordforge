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

/// P1:x-device-id 格式+长度校验。拒绝空串、超长(>128)、含控制字符 / 空白的畸形 id。
/// 允许 UUID / 设备指纹常见字符集(字母数字与 `-_.:`),保守接受不破坏现有客户端取值。
/// `pub(crate)`:`/telemetry` 端点中间件跳过 upsert,需在 `submit_telemetry` claim 路径直接复用本校验。
pub(crate) fn is_valid_device_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

/// m027:从 headers 提取 client IP。同 routes/auth.rs 实现,本仓库目前仅两处用,
/// 不抽公共模块避免跨 mod 引用扩散。
fn extract_client_ip(headers: &axum::http::HeaderMap) -> Option<String> {
    // 优先取不可伪造的 x-real-ip(nginx $remote_addr);回退 XFF 取最右段(最近可信跳),
    // 避免客户端注入 XFF 首值伪造审计 IP。与 middleware/rate_limit.rs 限流取值口径一致。
    if let Some(real) = headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        return Some(real);
    }
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|xff| xff.split(',').next_back())
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
    let skip_telemetry_upsert =
        tele_path == "/telemetry" || tele_path.starts_with("/telemetry/");

    // P1 设备抢注硬化(可落地部分):对 x-device-id 做格式+长度校验,拒绝畸形/超长串。
    // 完整的不可伪造强绑定(登录签发设备绑定令牌 / 挑战-应答)为后续 follow-up,本轮不实现。
    // 校验通过的 id 才进入 ban 检查 / claim 写入;非法 id 视同"无设备头"静默放行(不阻断业务),
    // 仅不参与设备归属逻辑——避免畸形 id 污染 client_devices。
    let device_id = req
        .headers()
        .get("x-device-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| is_valid_device_id(s))
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

        // P2 语义边界说明:此处封禁的粒度是"设备"(x-device-id),不是账号。被封用户只需
        // 轮换 / 伪造 x-device-id 即可绕过(轮换头绕过),这是"仅封设备"的固有局限。主修复在
        // UI 提示侧;此处不擅自把设备封禁联动封号(那是越权改产品语义),仅留此注释标注边界。
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
            // P2 fail-closed:ban 查询出错时不得放行——否则被封设备可借 DB 抖动绕过封禁。
            // 仅"查询出错"返 503;正常"未封禁"(上面 Ok(Ok(false)))仍放行。
            Ok(Err(e)) => {
                tracing::error!(error = %e, device_id = %did, "Failed to check device ban");
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    axum::Json(serde_json::json!({
                        "code": "BAN_CHECK_UNAVAILABLE",
                        "message": "设备状态校验暂不可用，请稍后重试"
                    })),
                )
                    .into_response();
            }
            Err(e) => {
                tracing::error!(error = %e, device_id = %did, "Device ban task failed");
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    axum::Json(serde_json::json!({
                        "code": "BAN_CHECK_UNAVAILABLE",
                        "message": "设备状态校验暂不可用，请稍后重试"
                    })),
                )
                    .into_response();
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
                Ok(Ok(())) => {
                    // P1 首占审计:upsert 的 owner claim 沿用现有 CASE 语义(owner NULL→当前用户),
                    // 行为不变;此处补审计日志,留痕设备首次被某用户占用,便于事后排查抢注。
                    // 注:CASE 仅在 owner 为 NULL 时写入,故已有 owner 的设备此日志不代表改写归属。
                    // 强凭证绑定(令牌 / 挑战-应答)为后续 follow-up。
                    tracing::info!(
                        device_id = %did,
                        user_id = %uid,
                        ts = %chrono::Utc::now().to_rfc3339(),
                        "device claim upsert (first-claim audit)"
                    );
                }
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
