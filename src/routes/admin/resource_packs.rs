//! v1.1-P0.7：admin 资源包管理。fork 自 `admin/updates.rs` 范式。
//!
//! 端点（全部受 AdminAuthUser 鉴权）：
//!   POST   /api/admin/resource-packs/:pack_id/versions       上传新版（raw body）
//!   PUT    /api/admin/resource-packs/:pack_id/channel/:channel/active  切激活 + SSE
//!   GET    /api/admin/resource-packs                         列表
//!   GET    /api/admin/resource-packs/:pack_id/stats          install_log 聚合
//!   DELETE /api/admin/resource-packs/:pack_id/versions/:version  软删除
//!
//! 上传 body 是 raw payload bytes（与 user_profile/avatar 同范式，不解析 multipart）；
//! 元数据通过 query params 传：version (semver) / channel / minAppVersion?。
//! handler 内自动：
//!   1. 校验 query
//!   2. 写 static/packs/<pack>/<ver>/payload.json
//!   3. 计算 SHA256（sha2 crate）
//!   4. 用 ResourcePackSigner 签 payload，得到 base64 64B
//!   5. 落 resource_pack_versions 表
//!
//! 切激活时调 state.broadcast_to_all_sse + 5min dedup（state.try_mark_pack_broadcast）。

use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::routing::{delete, get, post, put};
use axum::Router;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

use crate::auth::AdminAuthUser;
use crate::extractors::JsonBody;
use crate::response::{ok, AppError};
use crate::state::{AppState, SseEvent};
use crate::store::operations::resource_packs::{
    ResourcePack, ResourcePackChannel, ResourcePackVersion,
};
use chrono::Datelike;

/// Admin 上传单包硬上限：4 MiB（对接文档 §2.2 建议 ≤ 2 MB，这里留一倍余量）。
const MAX_PACK_UPLOAD_SIZE: usize = 4 * 1024 * 1024;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_packs))
        .route("/summary", get(pack_summary))
        .route("/:pack_id/versions", post(upload_version))
        .route("/:pack_id/versions/:version", delete(deactivate_version))
        .route("/:pack_id/channel/:channel/active", put(set_active))
        .route("/:pack_id/stats", get(pack_stats))
        // 单独覆盖 body 上限（默认 2 MiB；本路由组 4 MiB）
        .layer(DefaultBodyLimit::max(MAX_PACK_UPLOAD_SIZE))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadQuery {
    version: String,
    channel: String,
    #[serde(default)]
    min_app_version: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminPackEntry {
    #[serde(flatten)]
    pack: ResourcePack,
    versions: Vec<ResourcePackVersion>,
    /// 各通道当前激活版本号，无则 null。
    active: ActiveChannels,
    /// 该 pack 在 install_log 的全量记录数。
    total_installs: i64,
    /// 近 7 天该 pack 各 outcome 计数。
    outcomes7d: OutcomeCounts,
}

/// 三通道激活版本号（null 表示该通道无激活）。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ActiveChannels {
    stable: Option<String>,
    beta: Option<String>,
    internal: Option<String>,
}

/// 三态 outcome 计数。key 沿用 DB 真实枚举（snake_case：`verify_failed`），
/// 故刻意不加 camelCase rename，与契约 `outcomes7d` / `failures7dByOutcome` 一致。
#[derive(Debug, Serialize)]
struct OutcomeCounts {
    installed: i64,
    verify_failed: i64,
    rollback: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetActiveBody {
    version: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PackStatsEntry {
    version: String,
    outcome: String,
    count: i64,
}

/// `GET /api/admin/resource-packs` — 列表（每 pack 附所有版本，admin UI 用）。
async fn list_packs(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let since_7d = seven_days_ago_rfc3339();
    let entries = state
        .run_store_task("admin.resource_packs.list", move |store| {
            let packs = store.list_resource_packs()?;
            let mut out = Vec::with_capacity(packs.len());
            for p in packs {
                let versions = store.list_pack_versions(&p.pack_id)?;
                let total_installs = store.pack_total_installs(&p.pack_id)?;
                let o7 = store.pack_outcomes_since(&p.pack_id, &since_7d)?;
                let mut active = ActiveChannels {
                    stable: None,
                    beta: None,
                    internal: None,
                };
                for (channel, version) in store.pack_active_versions(&p.pack_id)? {
                    match channel {
                        ResourcePackChannel::Stable => active.stable = Some(version),
                        ResourcePackChannel::Beta => active.beta = Some(version),
                        ResourcePackChannel::Internal => active.internal = Some(version),
                    }
                }
                out.push(AdminPackEntry {
                    pack: p,
                    versions,
                    active,
                    total_installs,
                    outcomes7d: OutcomeCounts {
                        installed: o7.installed,
                        verify_failed: o7.verify_failed,
                        rollback: o7.rollback,
                    },
                });
            }
            Ok::<_, crate::store::StoreError>(out)
        })
        .await??;
    Ok(ok(entries))
}

/// `POST /api/admin/resource-packs/:pack_id/versions?version=&channel=&minAppVersion=`
/// raw body 是 payload bytes（Content-Type 不限，但建议 application/json）。
async fn upload_version(
    admin: AdminAuthUser,
    Path(pack_id): Path<String>,
    Query(q): Query<UploadQuery>,
    State(state): State<AppState>,
    body: axum::body::Bytes,
) -> Result<impl axum::response::IntoResponse, AppError> {
    validate_pack_id(&pack_id)?;
    validate_version(&q.version)?;
    let channel = parse_channel(&q.channel)?;

    if body.is_empty() {
        return Err(AppError::bad_request("PACK_EMPTY", "payload 不能为空"));
    }
    if body.len() > MAX_PACK_UPLOAD_SIZE {
        return Err(AppError::payload_too_large(&format!(
            "payload 超过 {MAX_PACK_UPLOAD_SIZE} 字节上限"
        )));
    }

    let signer = state.resource_pack_signer().await.ok_or_else(|| {
        AppError::service_unavailable(
            "RESOURCE_PACK_SIGNER_UNAVAILABLE",
            "资源包签名器未初始化，无法签名",
        )
    })?;

    // SHA256
    let sha256_hex = {
        let mut hasher = Sha256::new();
        hasher.update(&body);
        hex_lower(&hasher.finalize())
    };

    // Ed25519 签名
    let signature_b64 = signer.sign_base64(&body);

    // 落盘：static/packs/<pack>/<version>/payload.json
    let dir = static_pack_dir().join(&pack_id).join(&q.version);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| AppError::internal(&format!("create_dir_all 失败: {e}")))?;
    let path = dir.join("payload.json");
    tokio::fs::write(&path, &body)
        .await
        .map_err(|e| AppError::internal(&format!("写 payload 失败: {e}")))?;

    let payload_path = format!("static/packs/{}/{}/payload.json", pack_id, q.version);
    let size_bytes = body.len() as i64;
    let published_at = chrono::Utc::now().to_rfc3339();

    let v = ResourcePackVersion {
        pack_id: pack_id.clone(),
        version: q.version.clone(),
        sha256: sha256_hex.clone(),
        signature: Some(signature_b64.clone()),
        signature_alg: "ed25519".to_string(),
        size_bytes,
        min_app_version: q.min_app_version.clone(),
        channel,
        payload_path,
        published_at,
        deactivated_at: None,
    };

    let pack_id_owned = pack_id.clone();
    let description = q.description.clone();
    let v_clone = v.clone();
    state
        .run_store_task("admin.resource_packs.upload", move |store| {
            store.upsert_resource_pack(&pack_id_owned, description.as_deref())?;
            store.insert_pack_version(&v_clone)?;
            Ok::<_, crate::store::StoreError>(())
        })
        .await??;

    // v1.1-P2.10：写 admin 审计（资源包上传），失败不影响主流程
    write_admin_audit(
        &state,
        &admin.admin_id,
        "resource_pack.upload",
        &pack_id,
        serde_json::json!({
            "version": q.version,
            "channel": channel.as_str(),
            "sha256": sha256_hex,
            "sizeBytes": size_bytes,
            "minAppVersion": q.min_app_version,
        }),
    );

    tracing::info!(
        pack_id = %pack_id,
        version = %q.version,
        channel = %channel.as_str(),
        sha256 = %sha256_hex,
        size_bytes,
        "资源包版本上传成功"
    );

    Ok(ok(serde_json::json!({
        "packId": pack_id,
        "version": q.version,
        "sha256": sha256_hex,
        "signature": signature_b64,
        "sizeBytes": size_bytes,
        "channel": channel.as_str(),
    })))
}

/// `PUT /api/admin/resource-packs/:pack_id/channel/:channel/active` — 切激活并广播 SSE。
async fn set_active(
    admin: AdminAuthUser,
    Path((pack_id, channel_str)): Path<(String, String)>,
    State(state): State<AppState>,
    JsonBody(body): JsonBody<SetActiveBody>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    validate_pack_id(&pack_id)?;
    let channel = parse_channel(&channel_str)?;
    validate_version(&body.version)?;

    // 受众客户端数：当前在线 SSE 连接数。切激活通过 broadcast_to_all_sse 推达全部在线连接，
    // 注册表不按 channel 分桶，故目标通道受众即在线总数（设计稿 #audience-count）。
    let audience_clients = state.active_sse().len() as i64;

    let pack_id_for_db = pack_id.clone();
    let version_for_db = body.version.clone();
    state
        .run_store_task("admin.resource_packs.set_active", move |store| {
            store.set_active_pack_version(&pack_id_for_db, channel, &version_for_db, None)
        })
        .await??;

    // v1.1-P2.10：切激活审计
    write_admin_audit(
        &state,
        &admin.admin_id,
        "resource_pack.set_active",
        &pack_id,
        serde_json::json!({
            "channel": channel.as_str(),
            "version": body.version,
        }),
    );

    // SSE 广播，5 分钟内 dedup（同 pack × channel 不重复推送）
    if state.try_mark_pack_broadcast(&pack_id, channel.as_str()) {
        state.broadcast_to_all_sse(SseEvent::ResourcePackAvailable {
            pack_id: pack_id.clone(),
            version: body.version.clone(),
            channel,
        });
        tracing::info!(
            pack_id = %pack_id,
            version = %body.version,
            channel = %channel.as_str(),
            "已广播 resource_pack_available SSE 事件"
        );
    } else {
        tracing::info!(
            pack_id = %pack_id,
            channel = %channel.as_str(),
            "5 分钟内已广播过，本次激活跳过 SSE"
        );
    }

    Ok(ok(serde_json::json!({
        "packId": pack_id,
        "channel": channel.as_str(),
        "version": body.version,
        "activated": true,
        // 受众客户端数（当前在线 SSE 连接数）—— 设计稿切换激活对话框 #audience-count
        "audienceClients": audience_clients,
    })))
}

/// `DELETE /api/admin/resource-packs/:pack_id/versions/:version` — 软删除（manifest 摘除）。
/// 物理文件保留 30 天供回滚，由 GC worker 删盘（暂未实现，P2 范围）。
async fn deactivate_version(
    admin: AdminAuthUser,
    Path((pack_id, version)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    validate_pack_id(&pack_id)?;
    validate_version(&version)?;
    let pack_id_for_db = pack_id.clone();
    let version_for_db = version.clone();
    state
        .run_store_task("admin.resource_packs.deactivate", move |store| {
            store.deactivate_pack_version(&pack_id_for_db, &version_for_db)
        })
        .await??;

    // v1.1-P2.10：软删除（下架）审计
    write_admin_audit(
        &state,
        &admin.admin_id,
        "resource_pack.deactivate",
        &pack_id,
        serde_json::json!({ "version": version }),
    );

    Ok(ok(serde_json::json!({
        "packId": pack_id,
        "version": version,
        "deactivated": true,
    })))
}

/// `GET /api/admin/resource-packs/:pack_id/stats` — install_log 按 (version, outcome) 聚合。
async fn pack_stats(
    _admin: AdminAuthUser,
    Path(pack_id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    validate_pack_id(&pack_id)?;
    let pack_id_for_db = pack_id.clone();
    let rows = state
        .run_store_task("admin.resource_packs.stats", move |store| {
            store.pack_install_stats(&pack_id_for_db)
        })
        .await??;
    let entries: Vec<PackStatsEntry> = rows
        .into_iter()
        .map(|(version, outcome, count)| PackStatsEntry {
            version,
            outcome,
            count,
        })
        .collect();
    Ok(ok(serde_json::json!({
        "packId": pack_id,
        "stats": entries,
    })))
}

/// `GET /api/admin/resource-packs/summary` — 跨包全局聚合 KPI。
/// 时间边界在 Rust 侧用 chrono 算出 RFC3339 后传 SQL（同格式同时区下字典序==时间序）。
async fn pack_summary(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let month_start = month_start_rfc3339();
    let today_start = today_start_rfc3339();
    let since_7d = seven_days_ago_rfc3339();

    // 受众客户端数：当前在线 SSE 连接数（与 set_active 的 audience_clients 同源），
    // 供切激活对话框预览 SSE 通告范围（设计稿 #audience-count）。
    let online_clients = state.active_sse().len() as i64;

    let sum = state
        .run_store_task("admin.resource_packs.summary", move |store| {
            store.resource_pack_summary(&month_start, &today_start, &since_7d)
        })
        .await??;

    // 近 7 天失败率：分母为 0 时返回 0（不除零）
    let total_7d = sum.outcomes_7d.total();
    let failure_rate_7d = if total_7d == 0 {
        0.0
    } else {
        sum.outcomes_7d.failures() as f64 / total_7d as f64
    };

    Ok(ok(serde_json::json!({
        "totalPacks": sum.total_packs,
        "newPacksThisMonth": sum.new_packs_this_month,
        "totalVersions": sum.total_versions,
        "versionsByChannel": {
            "stable": sum.versions_by_channel.stable,
            "beta": sum.versions_by_channel.beta,
            "internal": sum.versions_by_channel.internal,
        },
        "installsToday": sum.installs_today,
        "installsTodaySuccess": sum.installs_today_success,
        "failureRate7d": failure_rate_7d,
        "failures7dByOutcome": {
            "verify_failed": sum.outcomes_7d.verify_failed,
            "rollback": sum.outcomes_7d.rollback,
        },
        // 当前在线客户端数（在线 SSE 连接），切激活对话框「受众」与 SSE 通告范围
        "onlineClients": online_clients,
    })))
}

// ── helpers ────────────────────────────────────────────────────────────────

/// 今日 00:00:00Z 的 RFC3339 字符串。
fn today_start_rfc3339() -> String {
    let now = chrono::Utc::now();
    now.date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("00:00:00 always valid")
        .and_utc()
        .to_rfc3339()
}

/// now - 7 天的 RFC3339 字符串（近 7 天窗口下界）。
fn seven_days_ago_rfc3339() -> String {
    (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339()
}

/// 本月 1 号 00:00:00Z 的 RFC3339 字符串。
fn month_start_rfc3339() -> String {
    let now = chrono::Utc::now();
    now.date_naive()
        .with_day(1)
        .expect("day 1 always valid")
        .and_hms_opt(0, 0, 0)
        .expect("00:00:00 always valid")
        .and_utc()
        .to_rfc3339()
}

/// v1.1-P2.10：写一条 admin 资源包审计。与 `updates.rs::insert_update_audit` 同范式，
/// 同步入 DB（SQLite 写入廉价），失败仅打 warn，不阻塞主响应。
fn write_admin_audit(
    state: &AppState,
    admin_id: &str,
    action: &str,
    pack_id: &str,
    metadata: serde_json::Value,
) {
    if let Err(e) = state.store().insert_admin_audit(
        admin_id,
        action,
        Some("resource_pack"),
        Some(pack_id),
        Some(&metadata),
    ) {
        tracing::warn!(error=%e, action=%action, "写 admin audit 失败（不影响主流程）");
    }
}

fn parse_channel(s: &str) -> Result<ResourcePackChannel, AppError> {
    ResourcePackChannel::from_str(s)
        .ok_or_else(|| AppError::bad_request("VALIDATION_ERROR", &format!("未知 channel: {s}")))
}

fn validate_pack_id(pack_id: &str) -> Result<(), AppError> {
    // kebab-case 业务 ID：[a-z0-9-]+，避免路径穿越
    if pack_id.is_empty()
        || pack_id.contains('/')
        || pack_id.contains('\\')
        || pack_id.contains('.')
        || pack_id.contains('\0')
        || !pack_id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(AppError::bad_request(
            "VALIDATION_ERROR",
            "packId 必须 kebab-case（仅 a-z 0-9 -）",
        ));
    }
    Ok(())
}

fn validate_version(v: &str) -> Result<(), AppError> {
    // 防路径穿越 + 限定字符集
    if v.is_empty()
        || v.contains('/')
        || v.contains('\\')
        || v.contains('\0')
        || v.starts_with('.')
        || !v
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '+')
    {
        return Err(AppError::bad_request(
            "VALIDATION_ERROR",
            "version 含非法字符（仅 a-zA-Z0-9 . - +）",
        ));
    }
    Ok(())
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

fn static_pack_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("static")
        .join("packs")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_pack_id_accepts_kebab() {
        assert!(validate_pack_id("wordbook-core").is_ok());
        assert!(validate_pack_id("a").is_ok());
        assert!(validate_pack_id("homepage-banners-v2").is_ok());
    }

    #[test]
    fn validate_pack_id_rejects_path_chars() {
        for bad in &[
            "",
            "../etc",
            "with/slash",
            "with\\back",
            "Upper",
            "dot.case",
            "with space",
        ] {
            assert!(validate_pack_id(bad).is_err(), "packId {:?} 应被拒绝", bad);
        }
    }

    #[test]
    fn validate_version_accepts_semver_like() {
        assert!(validate_version("1.0.0").is_ok());
        assert!(validate_version("1.1.0-rc.1").is_ok());
        assert!(validate_version("2.0.0+build.42").is_ok());
    }

    #[test]
    fn validate_version_rejects_traversal() {
        for bad in &["", "../1.0.0", "1.0/0", "..", ".hidden", "1 0 0"] {
            assert!(validate_version(bad).is_err(), "version {:?} 应被拒绝", bad);
        }
    }

    #[test]
    fn hex_lower_64_chars_for_sha256() {
        let input = b"";
        let mut hasher = Sha256::new();
        hasher.update(input);
        let hex = hex_lower(&hasher.finalize());
        assert_eq!(hex.len(), 64);
        assert_eq!(
            hex,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
