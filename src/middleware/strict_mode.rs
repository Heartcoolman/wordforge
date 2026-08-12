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

    // web 声明判定 + Fetch Metadata 观测：入口算一次、本请求内复用（此前 is_genuine_web 在
    // UA 契约与 D4 守卫各调一次，「疑似伪造」warn 每请求翻倍）。豁免以「声明 web」为准；
    // sec-fetch-mode 检测降级为纯观测（限频 warn + metrics 计数），见 observe_web_claim 注释。
    let claims_web = observe_web_claim(&req);

    // 1)+2) 全量契约校验（User-Agent / 平台）仅在 strict-mode 启用时执行。
    if cfg.enabled {
        let platform = req
            .headers()
            .get("x-device-platform")
            .and_then(|v| v.to_str().ok());

        // web 端豁免 UA 契约：浏览器禁止 fetch 设置 User-Agent（forbidden header），无法满足
        // WordForge-Web/<ver>。后端同源托管 web 应用后没有 dev 代理注入 UA，故以
        // x-device-platform=web 作为平台标识豁免 UA 校验；平台头本身仍必须存在（见下 platform_ok）。
        // 注：web 的版本门控由 /api/status.webTargetVersion + X-Upgrade-Hint（软更新）承担，
        // 不依赖此处基于 UA semver 的硬切流（native 仍走 UA）。
        if !claims_web && ua_parsed.is_none() {
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

    // D4 切流防绕过：只要版本强制生效（gate.enabled 或 cfg.enabled）且配置了 min_client_version，
    // native 客户端（非 web）若缺失/非法 UA 就无法解析出 semver，会绕过下方 semver 门控直接放行——
    // 等于丢/改 UA 即可逃避切流。故反绕过守卫的条件必须与下方 semver 强制条件(gate.enabled || cfg.enabled)
    // 一致,否则 cfg.enabled=true 而 gate.enabled=false 时,丢 UA 仍可绕过。web 端不走 UA semver 门控，豁免。
    // 豁免判据是「声明 web」（claims_web）而非「声明 web 且带 sec-fetch-mode」：后者曾把不发送
    // Sec-Fetch-* 的老浏览器硬拒成 CLIENT_OUTDATED——版本门控本意拦旧 native 客户端，却把合法
    // web 用户锁在门外且无升级出路（web 无 UA semver 可升）。伪造平台头绕过门控的风险改由
    // observe_web_claim 的观测口径（warn + 计数）跟踪，评估后再决定是否需要更强的 web 证明通道。
    if (gate.enabled || cfg.enabled)
        && gate.min_client_version.is_some()
        && ua_parsed.is_none()
        && !claims_web
    {
        return client_outdated_response("unknown", gate.min_client_version.as_deref().unwrap_or(""));
    }

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

/// Fetch Metadata Request Headers（W3C）定义的 `sec-fetch-mode` 合法取值全集。
/// 存在该头但值不在此列 = 非浏览器伪造（浏览器不会发出集合外的值），观测口径按"缺失"计。
const VALID_SEC_FETCH_MODES: [&str; 5] = ["cors", "no-cors", "same-origin", "navigate", "websocket"];

/// 判定请求是否声明 `x-device-platform: web`，并做 Fetch Metadata **观测**（不影响判定结果）。
///
/// `x-device-platform` 是任意客户端可自由设置的普通 header，声明 web 即可豁免 UA 契约与
/// UA semver 版本门控——这**不是**加密级证明。曾以「必须同时带 sec-fetch-mode」作硬前置
/// （缺失按非-web 处理、可被 D4 守卫硬拒 CLIENT_OUTDATED），但该头是较新的浏览器特性，
/// 不发送 Sec-Fetch-* 的老浏览器会被误伤且无升级出路，故降级为纯观测：
/// - `sec-fetch-mode` 缺失或值非法（校验值 ∈ VALID_SEC_FETCH_MODES，浏览器强制自动附带、
///   JS 无法覆盖的 forbidden header）→ 限频 warn + metrics 计数，供评估误伤率/伪造率。
/// - 注意该信号即便作硬前置也只是「多伪造一个 header」的门槛（curl -H 'sec-fetch-mode: cors'
///   即可通过），并非"一整套浏览器专属信号"；中期若需给 web 更难伪造的版本上报通道
///   （如短时签名 token），依据本观测数据评估。
fn observe_web_claim(req: &Request<axum::body::Body>) -> bool {
    let claims_web = req
        .headers()
        .get("x-device-platform")
        .and_then(|v| v.to_str().ok())
        == Some("web");
    if !claims_web {
        return false;
    }
    let has_valid_fetch_metadata = req
        .headers()
        .get("sec-fetch-mode")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            VALID_SEC_FETCH_MODES
                .iter()
                .any(|m| v.trim().eq_ignore_ascii_case(m))
        });
    if !has_valid_fetch_metadata {
        crate::metrics_counters::incr_strict_web_claim_without_fetch_metadata();
        warn_web_claim_without_fetch_metadata(req.uri().path());
    }
    claims_web
}

/// 「声明 web 但缺/非法 sec-fetch-mode」观测 warn，60s 限频（计数不受限频影响）：
/// 该情形在老浏览器流量下可能高频出现，逐请求 warn 会刷爆日志。
fn warn_web_claim_without_fetch_metadata(path: &str) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static LAST_WARN_SECS: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let last = LAST_WARN_SECS.load(Ordering::Relaxed);
    if now.saturating_sub(last) >= 60
        && LAST_WARN_SECS
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    {
        tracing::warn!(
            path,
            total = crate::metrics_counters::strict_web_claim_without_fetch_metadata(),
            "strict-mode: x-device-platform=web 但缺失/非法 sec-fetch-mode（仅观测不拒绝；可能是老浏览器或伪造平台头，60s 限频）"
        );
    }
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
    // 必须恰好是 `\d+\.\d+\.\d+` 三段:消费三段数字后须无残余段。否则像 "1.2.3.4"
    // 会被 semver::Version::parse 拒绝(返回 Err),从而绕过版本门(release 切量硬开关)。
    let mut parts = ver.split('.');
    // 每段必须**非空**且全为数字:空串对 `all(is_ascii_digit)` 是真空真,会让 "1.2."、"1..3"、".."
    // 误判为合法 UA(返回 Some)却无法通过 semver::parse,导致版本门(release 切量硬开关)静默失效被绕过。
    let valid = (0..3).all(|_| {
        parts
            .next()
            .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
    });
    if !valid || parts.next().is_some() {
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

    #[test]
    fn rejects_empty_version_segment() {
        // 空版本段必须被拒绝,否则可绕过版本门(参见 parse_wordforge_ua 注释)。
        assert_eq!(parse_wordforge_ua("WordForge-iOS/1.2."), None);
        assert_eq!(parse_wordforge_ua("WordForge-iOS/1..3"), None);
        assert_eq!(parse_wordforge_ua("WordForge-iOS/.."), None);
    }

    #[test]
    fn rejects_extra_version_segment() {
        // 4 段语义版本会被 semver 拒绝,必须在解析阶段就剔除以免绕过版本门。
        assert_eq!(parse_wordforge_ua("WordForge-iOS/1.2.3.4"), None);
        assert_eq!(parse_wordforge_ua("WordForge-Android/1.2.3.0"), None);
    }
}
