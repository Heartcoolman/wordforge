//! §12 strict-mode 客户端契约校验中间件
//!
//! 校验维度（仅作用于受检查路由，豁免路由通过路由分组排除，见 routes/mod.rs）：
//! - `User-Agent` 必须匹配 `WordForge-(iOS|Android|Web)/<semver>` → `MISSING_USER_AGENT`
//! - `x-device-platform` header 必须存在且非 `unknown` → `MISSING_OS`
//! - 若配置 `min_client_version`，UA 中 semver 低于阈值 → `CLIENT_OUTDATED`
//!
//! D4：「最低版本门控」（CLIENT_OUTDATED）是独立于全量契约校验的**发布切流**开关，
//! 可在 strict-mode 整体关闭（`enabled=false`）时单独启用。配置来源为 `system_settings`
//! （admin 运行时可改，优先）回落 env `MIN_CLIENT_VERSION`，由 `AppState::get_version_gate`
//! 按 60s TTL 缓存、admin 写端点 `refresh_version_gate` 即时刷新。门控命中始终硬拒绝
//! （不走 soft-block，否则切流无效）；UA/平台两项契约校验仍仅在 `enabled=true` 时生效。
//!
//! 豁免机制（M1-A6）：豁免路由（admin / status / realtime / telemetry / v1）不加此中间件层，
//! 因此中间件无需内部路径匹配，硬编码路径列表已彻底消除。
//!
//! 失败行为：
//! - `enabled=false`：直接放行（默认）
//! - `enabled=true && hard_block=false`：tracing::warn 后放行（soft-block 灰度）
//! - `enabled=true && hard_block=true`：返回 400 + 错误码
//!
//! 注意 telemetry payload 内的 `MISSING_TIMEZONE / MISSING_LANGUAGE / MISSING_DEVICE_FINGERPRINT`
//! 校验由 `routes/telemetry.rs` 在 body 解析后完成（中间件看不到 body），二者协同覆盖契约。

use axum::extract::State;
use axum::http::{header, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::state::AppState;

pub async fn strict_mode_middleware(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let cfg = state.config().strict_mode.clone();
    // D4：版本门控是「发布切流」开关，可独立于全量契约校验（cfg.enabled）启用。
    // 运行时配置（admin 可改）优先于 env，由 AppState 缓存按 60s TTL 维护 + 写端点即时刷新。
    let gate = state.get_version_gate();

    // 两者皆关：直接放行（默认）。
    if !cfg.enabled && !gate.enabled {
        return next.run(req).await;
    }

    let path = req.uri().path();

    let user_agent = req
        .headers()
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok());

    let ua_parsed = user_agent.and_then(parse_wordforge_ua);

    // 1)+2) 全量契约校验（User-Agent / 平台）仅在 strict-mode 启用时执行。
    if cfg.enabled {
        let platform = req
            .headers()
            .get("x-device-platform")
            .and_then(|v| v.to_str().ok());

        if ua_parsed.is_none() {
            if let Some(resp) = enforce(&cfg, path, "MISSING_USER_AGENT", "缺少或非法的 User-Agent")
            {
                return resp;
            }
        }

        let platform_ok = platform.is_some_and(|p| !p.is_empty() && p != "unknown");
        if !platform_ok {
            if let Some(resp) = enforce(&cfg, path, "MISSING_OS", "缺少 x-device-platform 头") {
                return resp;
            }
        }
    }

    // 3) 最低版本门控（仅当 UA 解析成功且门控启用/配置）。
    //    门控独立于 cfg.hard_block：发布切流场景下「拒绝」必须是硬拒绝（否则切流无效）。
    if let (Some((_plat, client_ver)), Some(min_ver)) =
        (ua_parsed.as_ref(), gate.min_client_version.as_ref())
    {
        if gate.enabled || cfg.enabled {
            if let (Ok(c), Ok(m)) = (
                semver::Version::parse(client_ver),
                semver::Version::parse(min_ver),
            ) {
                if c < m {
                    return client_outdated_response(client_ver, min_ver);
                }
            }
        }
    }

    next.run(req).await
}

/// D4：版本门控命中——始终硬拒绝（发布切流不走 soft-block）。
fn client_outdated_response(client_ver: &str, min_ver: &str) -> Response {
    tracing::info!(
        client_ver,
        min_ver,
        "version-gate 拒绝旧客户端 CLIENT_OUTDATED"
    );
    (
        StatusCode::BAD_REQUEST,
        axum::Json(serde_json::json!({
            "success": false,
            "code": "CLIENT_OUTDATED",
            "message": "客户端版本过低，请升级到最新版本",
        })),
    )
        .into_response()
}

/// 根据 hard_block 配置决定拦截或放行
fn enforce(
    cfg: &crate::config::StrictModeConfig,
    path: &str,
    code: &'static str,
    message: &'static str,
) -> Option<Response> {
    if cfg.hard_block {
        Some(
            (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({
                    "success": false,
                    "code": code,
                    "message": message,
                })),
            )
                .into_response(),
        )
    } else {
        tracing::warn!(path, code, message, "strict-mode soft-block (放行)");
        None
    }
}

/// 解析 `WordForge-(iOS|Android|Web)/<semver>` 前缀，返回 (platform, semver)
/// 不依赖 regex；手工状态机即可。
pub(crate) fn parse_wordforge_ua(ua: &str) -> Option<(String, String)> {
    let prefix = "WordForge-";
    let rest = ua.strip_prefix(prefix)?;
    let slash = rest.find('/')?;
    let platform = &rest[..slash];
    if !matches!(platform, "iOS" | "Android" | "Web") {
        return None;
    }
    let after_slash = &rest[slash + 1..];
    // semver 截止到第一个 space / paren / 结束
    let end = after_slash
        .find(|c: char| c.is_whitespace() || c == '(')
        .unwrap_or(after_slash.len());
    let ver = &after_slash[..end];
    if ver.is_empty() {
        return None;
    }
    // 至少要 `\d+\.\d+\.\d+` 三段
    let mut parts = ver.split('.');
    let valid = (0..3).all(|_| {
        parts
            .next()
            .is_some_and(|s| s.chars().all(|c| c.is_ascii_digit()))
    });
    if !valid {
        return None;
    }
    Some((platform.to_string(), ver.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_ios_ua() {
        let parsed = parse_wordforge_ua("WordForge-iOS/1.0.0 (iPhone15,2; iOS 17.5; zh-CN)");
        assert_eq!(parsed, Some(("iOS".into(), "1.0.0".into())));
    }

    #[test]
    fn parses_valid_android_ua() {
        let parsed = parse_wordforge_ua("WordForge-Android/2.3.1 (Pixel 7; Android 14)");
        assert_eq!(parsed, Some(("Android".into(), "2.3.1".into())));
    }

    #[test]
    fn rejects_invalid_platform() {
        let parsed = parse_wordforge_ua("WordForge-Symbian/1.0.0");
        assert_eq!(parsed, None);
    }

    #[test]
    fn rejects_url_session_default() {
        let parsed = parse_wordforge_ua("WordForge-App/1 CFNetwork/1410.0.3 Darwin/22.6.0");
        assert_eq!(parsed, None);
    }

    #[test]
    fn rejects_non_semver() {
        assert_eq!(parse_wordforge_ua("WordForge-iOS/1.0"), None);
        assert_eq!(parse_wordforge_ua("WordForge-iOS/v1.0.0"), None);
    }
}
