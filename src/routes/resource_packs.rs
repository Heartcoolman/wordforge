//! v1.1-P0.3：客户端公开访问的资源包端点。
//!
//! 端点：
//!   GET /api/resource-packs                          列出所有 pack 元数据
//!   GET /api/resource-packs/public-key               Ed25519 公钥 (base64)
//!   GET /api/resource-packs/:pack_id/manifest        当前 channel 激活版本的 manifest
//!
//! 全部匿名（不需要 Authorization）。manifest 端点支持 `If-None-Match` 304。
//! downloadURL 由 host 头 + scheme 推断，物理路径 `static/packs/<pack>/<ver>/payload.json`
//! 被 ServeDir 自动托管在 `/packs/...`。

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::operations::resource_packs::{ResourcePackChannel, ResourcePackManifest};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_packs))
        .route("/public-key", get(public_key))
        .route("/:pack_id/manifest", get(fetch_manifest))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestQuery {
    /// 客户端 App marketing version（必填），用于服务端按 minAppVersion 决定返回哪版
    app_version: String,
    /// 客户端 Locale（必填，BCP-47），多语言包路由用
    #[allow(dead_code)] // POC 阶段尚未做 locale 路由，预留接口
    locale: String,
    /// 灰度通道，缺省 `stable`
    #[serde(default)]
    channel: Option<String>,
}

/// `GET /api/resource-packs` — 列出所有可见 pack（v1.1 production §7.2 提到的列表 API）。
async fn list_packs(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let packs = state
        .run_store_task("resource_packs.list", |store| store.list_resource_packs())
        .await??;
    Ok(ok(packs))
}

/// `GET /api/resource-packs/public-key` — 返回 Ed25519 公钥 base64（44 字符含 padding）。
/// 客户端发版时把公钥硬编码进 binary 是正式分发路径；本端点便于客户端 verify SDK 自检。
async fn public_key(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let signer = state.resource_pack_signer().await.ok_or_else(|| {
        AppError::service_unavailable("RESOURCE_PACK_SIGNER_UNAVAILABLE", "资源包签名器未初始化")
    })?;
    Ok((
        StatusCode::OK,
        [(header::CACHE_CONTROL, "public, max-age=300")],
        Json(serde_json::json!({
            "publicKey": signer.public_key_base64(),
            "algorithm": "ed25519",
        })),
    ))
}

/// `GET /api/resource-packs/:pack_id/manifest` — 返回当前 channel 激活版本元数据。
/// 字段名以 ResourcePackManifest 的 serde 序列化输出为准（downloadURL 三字母全大写），
/// 契约见 docs/api-endpoints.md §资源包热更。
async fn fetch_manifest(
    Path(pack_id): Path<String>,
    Query(q): Query<ManifestQuery>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    if pack_id.trim().is_empty() {
        return Err(AppError::bad_request("VALIDATION_ERROR", "packId 不能为空"));
    }
    let channel = match q.channel.as_deref().unwrap_or("stable") {
        "stable" => ResourcePackChannel::Stable,
        "beta" => ResourcePackChannel::Beta,
        "internal" => ResourcePackChannel::Internal,
        other => {
            return Err(AppError::bad_request(
                "VALIDATION_ERROR",
                &format!("未知 channel：{other}"),
            ));
        }
    };

    let pack_id_owned = pack_id.clone();
    let active = state
        .run_store_task("resource_packs.get_active", move |store| {
            store.get_active_pack_version(&pack_id_owned, channel)
        })
        .await??
        .ok_or_else(|| AppError::resource_pack_not_found("资源包不存在"))?;

    // minAppVersion 校验：客户端低于此版本拒绝
    if let Some(min) = active.min_app_version.as_deref() {
        if compare_semver_numeric(&q.app_version, min).is_lt() {
            return Err(AppError::resource_pack_app_version_too_low(&format!(
                "需 App {min} 或更高（当前 {}）",
                q.app_version
            )));
        }
    }

    // If-None-Match 304：客户端命中即不回传 body
    if let Some(etag) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|h| h.to_str().ok())
    {
        if etag.trim_matches('"') == active.version {
            return Ok((
                StatusCode::NOT_MODIFIED,
                [
                    (header::ETAG, format!("\"{}\"", active.version)),
                    (header::CACHE_CONTROL, "public, max-age=60".to_string()),
                ],
            )
                .into_response());
        }
    }

    let base = resolve_public_base_url(
        &headers,
        state.config().trust_proxy,
        &state.config().resource_pack_trusted_hosts,
    );
    // 下发文件名从 active.payload_path 派生(tarball 工件落盘为 payload.tar.gz,JSON 为
    // payload.json),不能硬编码 payload.json,否则 tarball pack 的 downloadURL 会 404。
    let fname = std::path::Path::new(&active.payload_path)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("payload.json");
    let download_url = format!(
        "{base}/packs/{pack}/{ver}/{fname}",
        pack = pack_id,
        ver = active.version,
    );

    let manifest = ResourcePackManifest {
        pack_id: pack_id.clone(),
        version: active.version.clone(),
        download_url,
        sha256: active.sha256,
        size_bytes: active.size_bytes,
        min_app_version: active.min_app_version,
        channel: active.channel,
        signature: active.signature,
        signature_algorithm: Some(active.signature_alg),
    };

    Ok((
        StatusCode::OK,
        [
            (header::ETAG, format!("\"{}\"", active.version)),
            (header::CACHE_CONTROL, "public, max-age=60".to_string()),
        ],
        Json(manifest),
    )
        .into_response())
}

/// 推断 base URL：优先 env RESOURCE_PACK_BASE_URL，否则从请求 Host + 推断 Scheme。
/// 安全：x-forwarded-host / x-forwarded-proto 是客户端可伪造的头,仅在 trust_proxy(前置可信代理)
/// 时才信任(与 middleware 里的 extract_client_ip 取值口径一致)。本 manifest 端点匿名可达,直连暴露
/// (trust_proxy=false)时若信任 x-forwarded-host,攻击者即可把下发 downloadURL 指向恶意主机(主机头注入,
/// 致 DoS / 载荷替换),故此场景只用 Host 头 + 固定 http scheme。生产建议直接设 RESOURCE_PACK_BASE_URL。
/// `trusted_hosts`：TRUST_PROXY=true 时 X-Forwarded-Host 必须命中（纯 hostname，忽略端口，
/// 大小写不敏感）才采信，否则回退真实 Host 头——见 config.rs resource_pack_trusted_hosts 字段注释，
/// 该响应带公共缓存，伪造转发头曾可污染下游共享缓存给其它所有客户端。
fn resolve_public_base_url(headers: &HeaderMap, trust_proxy: bool, trusted_hosts: &[String]) -> String {
    if let Ok(env_url) = std::env::var("RESOURCE_PACK_BASE_URL") {
        return env_url.trim_end_matches('/').to_string();
    }
    let real_host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost");

    // 白名单命中后回填的是归一化的 hostname_only，而非原始头值：白名单只校验 `:` 前的
    // hostname，若回填原始值，`cdn.example.com:8080@evil` / 带逗号的多值头等能通过校验
    // 却把端口/userinfo/逗号残余注入 downloadURL（该响应带公共缓存，污染面放大）。
    let forwarded_hostname: Option<String> = if trust_proxy {
        headers
            .get("x-forwarded-host")
            .and_then(|v| v.to_str().ok())
            .map(|h| h.split(':').next().unwrap_or(h).trim().to_ascii_lowercase())
            .filter(|hostname| trusted_hosts.contains(hostname))
    } else {
        None
    };

    match forwarded_hostname {
        Some(hostname) => {
            // scheme 只认 https（其余含伪造任意串一律回落 http）：X-Forwarded-Proto 合法
            // 取值就 http/https 两个，原样拼接会把非法串带进公共缓存的 URL。
            let https = headers
                .get("x-forwarded-proto")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|p| p.trim().eq_ignore_ascii_case("https"));
            let scheme = if https { "https" } else { "http" };
            format!("{scheme}://{hostname}")
        }
        None => format!("http://{real_host}"),
    }
}

/// 按字符串 numeric 比较 semver（与 iOS `compare(options:.numeric)` 等价）。
/// "1.10.0" > "1.9.0"。仅用于 minAppVersion 比较，不需要完整 semver 解析。
fn compare_semver_numeric(a: &str, b: &str) -> std::cmp::Ordering {
    let a_parts: Vec<&str> = a.split('.').collect();
    let b_parts: Vec<&str> = b.split('.').collect();
    for i in 0..a_parts.len().max(b_parts.len()) {
        let ai: u64 = a_parts.get(i).and_then(|s| s.parse().ok()).unwrap_or(0);
        let bi: u64 = b_parts.get(i).and_then(|s| s.parse().ok()).unwrap_or(0);
        match ai.cmp(&bi) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }
    std::cmp::Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    #[test]
    fn semver_numeric_compares_correctly() {
        assert_eq!(compare_semver_numeric("1.0.0", "1.0.0"), Ordering::Equal);
        assert_eq!(compare_semver_numeric("1.10.0", "1.9.0"), Ordering::Greater);
        assert_eq!(compare_semver_numeric("1.9.0", "1.10.0"), Ordering::Less);
        assert_eq!(
            compare_semver_numeric("2.0.0", "1.99.99"),
            Ordering::Greater
        );
        // 长度不等：1.1 == 1.1.0
        assert_eq!(compare_semver_numeric("1.1", "1.1.0"), Ordering::Equal);
        // 非数字段当 0 处理
        assert_eq!(compare_semver_numeric("1.x.0", "1.0.0"), Ordering::Equal);
    }

    /// env 与 header 推断合并为一个测试，避免并行执行时 RESOURCE_PACK_BASE_URL 互相干扰。
    #[test]
    fn resolve_base_url_priority() {
        // 起点：清空 env，仅靠 Host
        std::env::remove_var("RESOURCE_PACK_BASE_URL");
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, "api.example.com".parse().unwrap());
        let trusted = vec!["cdn.example.com".to_string()];
        // trust_proxy=false：只用 Host + 固定 http
        assert_eq!(
            resolve_public_base_url(&headers, false, &trusted),
            "http://api.example.com"
        );

        // 伪造的 x-forwarded-host/proto 在 trust_proxy=false 时必须被忽略（防主机头注入）
        headers.insert("x-forwarded-proto", "https".parse().unwrap());
        headers.insert("x-forwarded-host", "attacker.example".parse().unwrap());
        assert_eq!(
            resolve_public_base_url(&headers, false, &trusted),
            "http://api.example.com"
        );

        // trust_proxy=true 但转发的 host 不在白名单内：必须回退真实 Host，绝不采信伪造值
        // （曾经的漏洞：任何 trust_proxy=true 部署都会无条件信任这个客户端可控头）。
        assert_eq!(
            resolve_public_base_url(&headers, true, &trusted),
            "http://api.example.com"
        );

        // trust_proxy=true 且转发 host 命中白名单（大小写不敏感）：采信 forwarded host/proto，
        // 且回填的是归一化 hostname（小写、无端口），不是原始头值。
        headers.insert("x-forwarded-host", "CDN.Example.com".parse().unwrap());
        assert_eq!(
            resolve_public_base_url(&headers, true, &trusted),
            "https://cdn.example.com"
        );

        // 带端口的转发 host：白名单按 hostname 命中，但 URL 里不得回填端口/原始残余。
        headers.insert("x-forwarded-host", "cdn.example.com:8080".parse().unwrap());
        assert_eq!(
            resolve_public_base_url(&headers, true, &trusted),
            "https://cdn.example.com"
        );

        // 伪造的非法 x-forwarded-proto 不得原样拼进 URL：非 https 一律回落 http。
        headers.insert("x-forwarded-proto", "gopher".parse().unwrap());
        assert_eq!(
            resolve_public_base_url(&headers, true, &trusted),
            "http://cdn.example.com"
        );
        headers.insert("x-forwarded-proto", "https".parse().unwrap());

        // env 优先级高于一切（无论 trust_proxy/白名单），且会 strip 末尾 /
        std::env::set_var("RESOURCE_PACK_BASE_URL", "https://override.example.com/");
        assert_eq!(
            resolve_public_base_url(&headers, false, &trusted),
            "https://override.example.com"
        );
        assert_eq!(
            resolve_public_base_url(&headers, true, &trusted),
            "https://override.example.com"
        );
        std::env::remove_var("RESOURCE_PACK_BASE_URL");
    }
}
