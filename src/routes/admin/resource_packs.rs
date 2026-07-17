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

/// Admin 上传单包硬上限：4 MiB（JSON 内容包，对接文档 §2.2 建议 ≤ 2 MB，留一倍余量）。
const MAX_PACK_UPLOAD_SIZE: usize = 4 * 1024 * 1024;
/// web-app 二进制工件（tar.gz of dist）上限：128 MiB。admin 鉴权 + 低频串行，可接受内存峰值。
const MAX_WEBAPP_UPLOAD_SIZE: usize = 128 * 1024 * 1024;
/// 服务端下发 web 应用的托管根：static/web-app/current 指向当前激活版本目录。
const WEBAPP_PACK_ID: &str = "web-app";
/// 上传临时文件名自增序号，保证并发上传的 tmp 路径互不冲突。
static UPLOAD_TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// web-app@stable 激活临界区互斥：把「解包+原子切 current」与「翻转 DB active 指针」串成一个临界区，
/// 避免两个并发激活不同版本时 current 符号链接与 DB active 指针交错 → 持久错配（status 与物理托管脱钩）。
/// 仅 web-app stable 激活极低频，串行无性能影响。
static WEBAPP_ACTIVATE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
/// 上传临界区互斥：把「去重预检 + 落盘 rename + DB insert」串成一个临界区。三者此前各自独立，
/// 两个并发上传同一 (pack_id, version) 都能通过预检（TOCTOU），随后各自 rename 到同一目标路径
/// （最后 rename 者的字节物理落地），DB insert 又各自独立提交——谁的 sha256/签名被记进 DB 与
/// 谁的字节最终躺在磁盘上互不保证是同一个请求，产生 DB 记录与实际文件不匹配。上传本就是极低频
/// admin 操作，全局串行无性能影响，同一粒度对齐 WEBAPP_ACTIVATE_LOCK。
static UPLOAD_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_packs))
        .route("/summary", get(pack_summary))
        // 上传端点单独叠加大 body 上限（覆盖路由组默认），支持 web-app tarball；
        // 其余路由仍受路由组 4 MiB 约束，不把大 body 面暴露给小端点。
        .route(
            "/:pack_id/versions",
            post(upload_version).layer(DefaultBodyLimit::max(MAX_WEBAPP_UPLOAD_SIZE)),
        )
        .route("/:pack_id/versions/:version", delete(deactivate_version))
        .route("/:pack_id/channel/:channel/active", put(set_active))
        .route("/:pack_id/stats", get(pack_stats))
        // 路由组默认 body 上限（小内容包 4 MiB）
        .layer(DefaultBodyLimit::max(MAX_PACK_UPLOAD_SIZE))
}

/// 工件类型 → 落盘/下载文件名。上传与（未来 manifest）共用，杜绝文件名漂移。
/// tarball（web-app 二进制）用 payload.tar.gz；其余 JSON 内容包沿用 payload.json。
fn payload_filename(artifact_type: &str) -> &'static str {
    if artifact_type == "tarball" {
        "payload.tar.gz"
    } else {
        "payload.json"
    }
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
    /// 工件类型：json（默认，内容包）| tarball（web-app 二进制，dist 的 tar.gz）。
    /// 未知 query 参数（如 compression=gzip）由 serde 默认忽略，无需声明。
    #[serde(default)]
    artifact_type: Option<String>,
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

/// 五态 outcome 计数（m070 起含 download_failed / apply_failed）。key 沿用 DB 真实枚举
/// （snake_case：`verify_failed`），故刻意不加 camelCase rename，与契约 `outcomes7d` /
/// `failures7dByOutcome` 一致。
#[derive(Debug, Serialize)]
struct OutcomeCounts {
    installed: i64,
    verify_failed: i64,
    rollback: i64,
    download_failed: i64,
    apply_failed: i64,
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
                        download_failed: o7.download_failed,
                        apply_failed: o7.apply_failed,
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

    let artifact_type = q.artifact_type.as_deref().unwrap_or("json");
    if artifact_type != "json" && artifact_type != "tarball" {
        return Err(AppError::bad_request(
            "VALIDATION_ERROR",
            "artifactType 仅支持 json | tarball",
        ));
    }
    let max_size = if artifact_type == "tarball" {
        MAX_WEBAPP_UPLOAD_SIZE
    } else {
        MAX_PACK_UPLOAD_SIZE
    };

    if body.is_empty() {
        return Err(AppError::bad_request("PACK_EMPTY", "payload 不能为空"));
    }
    if body.len() > max_size {
        return Err(AppError::payload_too_large(&format!(
            "payload 超过 {max_size} 字节上限"
        )));
    }

    // 先验去重：(pack_id, version) 已存在则 409 提前拒绝，绝不覆盖磁盘 payload.json。
    // 否则重传同版本号会用新字节覆盖旧 payload，而 DB 内旧 sha256/签名若仍激活，
    // 全量在线客户端验签必败 → 该资源包「变砖」。
    // UPLOAD_LOCK：预检非原子，见该锁声明处注释——串起预检到 DB insert 整段临界区，
    // 保证并发上传同一 (pack,version) 时"谁的字节落盘"与"谁的 sha256 记进 DB"是同一个请求。
    let _upload_guard = UPLOAD_LOCK.lock().await;
    {
        let pack_id_check = pack_id.clone();
        let version_check = q.version.clone();
        let exists = state
            .run_store_task("admin.resource_packs.version_exists", move |store| {
                store.pack_version_exists(&pack_id_check, &version_check)
            })
            .await??;
        if exists {
            return Err(AppError::conflict(
                "PACK_VERSION_EXISTS",
                &format!("版本 {} 已存在，不可重传覆盖；请改用新版本号", q.version),
            ));
        }
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
    // 文件名按工件类型派生（tarball→payload.tar.gz），ServeDir 据扩展名给正确 Content-Type；
    // payload_path 同步，激活/下载据此定位（artifact_type 即由该文件名约定承载，免 DB 迁移）。
    let filename = payload_filename(artifact_type);
    // 先写临时文件再 rename，避免半截文件被 ServeDir 托管或被激活解包。
    // tmp 名按 (pid, 进程内自增序号) 唯一化：避免两个并发上传同一 (pack,version) 争抢同名 tmp——
    // 一方 rename 后另一方 tmp 已被移走 → ENOENT 致 500（同时也是集成测试共享 static/ 时的竞态根因）。
    let tmp_seq = UPLOAD_TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp_path = dir.join(format!("{filename}.{}.{}.tmp", std::process::id(), tmp_seq));
    tokio::fs::write(&tmp_path, &body)
        .await
        .map_err(|e| AppError::internal(&format!("写 payload 失败: {e}")))?;
    let path = dir.join(filename);
    tokio::fs::rename(&tmp_path, &path)
        .await
        .map_err(|e| AppError::internal(&format!("rename payload 失败: {e}")))?;

    let payload_path = format!("static/packs/{}/{}/{}", pack_id, q.version, filename);
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
    drop(_upload_guard);

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
        "artifactType": artifact_type,
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

    // web-app@stable 激活临界区锁：覆盖「解包+原子切 current」与下方「翻转 DB active」，
    // 二者作为整体串行，杜绝并发激活不同版本时 current 与 DB 指针交错错配。guard 持有至函数末。
    let _webapp_guard = if pack_id == WEBAPP_PACK_ID && channel == ResourcePackChannel::Stable {
        Some(WEBAPP_ACTIVATE_LOCK.lock().await)
    } else {
        None
    };

    // web-app（服务端下发的 web 二进制）切激活 stable：必须「先校验+解包+原子切 current 成功，
    // 再翻转 DB 激活指针」。否则一旦解包失败，DB/`status.webTargetVersion`/SSE 已对外报新版本，
    // 而托管根 current 仍指向旧版/不存在 → 客户端被导向不存在的构建。仅 stable 驱动托管根。
    // 回滚补偿：记录 (webapp_root, 激活前 current 指向)，供下方 DB 翻转失败时把 current 切回旧版，
    // 避免「current=新构建 但 DB/status=旧版本号」的持久错配。
    let mut webapp_rollback: Option<(PathBuf, Option<PathBuf>)> = None;
    if pack_id == WEBAPP_PACK_ID && channel == ResourcePackChannel::Stable {
        let pid = pack_id.clone();
        let ver = body.version.clone();
        let target = state
            .run_store_task("admin.resource_packs.get_webapp_version", move |store| {
                store.get_pack_version(&pid, &ver)
            })
            .await??
            .ok_or_else(|| {
                AppError::not_found(&format!("web-app 版本 {} 不存在，无法激活", body.version))
            })?;
        // 拒绝激活已软删除版本：get_pack_version 不过滤 deactivated_at，但 status 读取的
        // get_active_pack_version 带 deactivated_at IS NULL，若放行会令 current 指向软删除版本而 status 报 null。
        if target.deactivated_at.is_some() {
            return Err(AppError::bad_request(
                "WEBAPP_VERSION_DEACTIVATED",
                &format!("web-app 版本 {} 已下架，不能激活", body.version),
            ));
        }
        // 版本的 stored channel 必须与请求激活的 channel 一致：resource_pack_active FK 仅校验
        // (pack_id,version)，不约束 channel，若不校验则 beta/internal 工件可被挂到 stable 托管根，
        // 且事后 get_active_pack_version(Stable) 的 channel JOIN 不匹配 → status 与物理 current 脱钩。
        if target.channel != channel {
            return Err(AppError::bad_request(
                "WEBAPP_CHANNEL_MISMATCH",
                &format!(
                    "web-app 版本 {} 属 {} 通道，不能在 {} 通道激活",
                    body.version,
                    target.channel.as_str(),
                    channel.as_str()
                ),
            ));
        }
        if target.payload_path.ends_with(".tar.gz") {
            // 取签名器公钥做激活时验签纵深（防 DB sha256 与磁盘工件被同时篡改但无法伪造 Ed25519 签名）。
            let public_key = state
                .resource_pack_signer()
                .await
                .map(|s| s.public_key_base64().to_string());
            // 切 current 前记录旧指向（用于 DB 翻转失败回滚）
            let webapp_root = static_web_app_dir();
            webapp_rollback = Some((webapp_root.clone(), read_current_target(&webapp_root)));
            let version = target.version.clone();
            let payload_path = target.payload_path.clone();
            let sha256 = target.sha256.clone();
            let signature = target.signature.clone();
            tokio::task::spawn_blocking(move || {
                activate_web_bundle(
                    &version,
                    &payload_path,
                    &sha256,
                    signature.as_deref(),
                    public_key.as_deref(),
                )
            })
            .await
            .map_err(|e| AppError::internal(&format!("web-app 解包任务 join 失败: {e}")))?
            .map_err(|e| AppError::internal(&format!("web-app 解包/切换失败: {e}")))?;
            tracing::info!(version = %target.version, "web-app 已解包并原子切 current（翻转激活指针前）");
        } else {
            // web-app 语义上必为 tarball 二进制工件。非 .tar.gz 无法切 current，若放行翻转 DB
            // 会使 status.webTargetVersion 前移却托管旧/缺失构建 → 客户端永远追不上下发版本号。
            // 故直接拒绝，守住「current 切换成功才前移版本」不变式（不翻转 DB）。
            return Err(AppError::bad_request(
                "WEBAPP_ARTIFACT_NOT_TARBALL",
                "web-app 版本必须为 tarball 工件（payload.tar.gz）才能激活",
            ));
        }
    }

    // 泛化上面 web-app 专属的 WEBAPP_CHANNEL_MISMATCH 保护：非 web-app pack 此前完全没有这层
    // 校验——resource_pack_active FK 只约束 (pack_id,version)，不约束 channel，激活请求能把任意
    // channel 下的版本号错误地挂到另一个 channel 的激活指针上，悄悄覆盖掉该 channel 原本工作
    // 正常的旧指针；事后 get_active_pack_version 的 channel JOIN 不匹配，该 channel 直接 404，
    // 此前无任何报错提示（PUT 本身返回 200 activated:true）。
    if pack_id != WEBAPP_PACK_ID {
        let pid = pack_id.clone();
        let ver = body.version.clone();
        let target = state
            .run_store_task(
                "admin.resource_packs.get_version_for_channel_check",
                move |store| store.get_pack_version(&pid, &ver),
            )
            .await??
            .ok_or_else(|| {
                AppError::not_found(&format!("{} 版本 {} 不存在，无法激活", pack_id, body.version))
            })?;
        if target.channel != channel {
            return Err(AppError::bad_request(
                "PACK_CHANNEL_MISMATCH",
                &format!(
                    "{} 版本 {} 属 {} 通道，不能在 {} 通道激活",
                    pack_id,
                    body.version,
                    target.channel.as_str(),
                    channel.as_str()
                ),
            ));
        }
    }

    let pack_id_for_db = pack_id.clone();
    let version_for_db = body.version.clone();
    let db_res = state
        .run_store_task("admin.resource_packs.set_active", move |store| {
            store.set_active_pack_version(&pack_id_for_db, channel, &version_for_db, None)
        })
        .await;
    // DB 翻转失败且本次为 web-app 激活：current 已前移，回滚回旧指向以保持 current 与 DB 一致。
    if !matches!(&db_res, Ok(Ok(_))) {
        if let Some((webapp_root, prev)) = webapp_rollback.take() {
            match restore_current(&webapp_root, prev) {
                Ok(()) => tracing::warn!("web-app DB 翻转失败，已回滚 current 到激活前指向"),
                Err(e) => tracing::error!(
                    error = %e,
                    "web-app DB 翻转失败且 current 回滚失败：current 已前移但 DB 未翻转，需手动重激活对齐"
                ),
            }
        }
    }
    db_res??;

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

    // web-app 专属保护：其它 pack 类型的"软删当前激活版本"是既有、经测试覆盖的正常语义
    // （manifest 摘除后 404，文件保留可恢复，见 docs/resource-packs.md §『停用』），此处不动。
    // 但 web-app 的 status.webTargetVersion 被 status.rs 文档为"单一真相源，恒指向后端确已
    // 托管的构建"——deactivate_pack_version 只改 deactivated_at，不动 resource_pack_active
    // 也不动 static/web-app/current 符号链接，若删的正是当前激活版本，webTargetVersion 会变
    // null，但 /web-app/current/* 仍在物理服务这份"已下线"的构建，两者失配、直接违反该不变式。
    // 激活路径为 web-app 做了 WEBAPP_ACTIVATE_LOCK + 拒绝激活已停用版本等多层保护，这里补齐
    // 对称的另一半：删除前先查是否为当前激活版本，是则要求先切换，不做静默失配。
    if pack_id == WEBAPP_PACK_ID {
        let pid = pack_id.clone();
        let ver = version.clone();
        let active_channels = state
            .run_store_task(
                "admin.resource_packs.check_webapp_active_before_delete",
                move |store| {
                    [
                        ResourcePackChannel::Stable,
                        ResourcePackChannel::Beta,
                        ResourcePackChannel::Internal,
                    ]
                    .into_iter()
                    .filter(|&ch| {
                        store
                            .get_active_pack_version(&pid, ch)
                            .ok()
                            .flatten()
                            .is_some_and(|v| v.version == ver)
                    })
                    .collect::<Vec<_>>()
                },
            )
            .await?;
        if !active_channels.is_empty() {
            let names: Vec<&str> = active_channels.iter().map(|c| c.as_str()).collect();
            return Err(AppError::conflict(
                "PACK_VERSION_ACTIVE",
                &format!(
                    "版本 {} 正在 web-app {} 通道激活中，请先切换到其它版本再删除",
                    version,
                    names.join("/")
                ),
            ));
        }
    }

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
            "download_failed": sum.outcomes_7d.download_failed,
            "apply_failed": sum.outcomes_7d.apply_failed,
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

/// web-app 托管根：static/web-app（current 符号链接指向当前激活版本目录）。沿用 static_pack_dir 同款 CWD 优先范式。
fn static_web_app_dir() -> PathBuf {
    if PathBuf::from("static").is_dir() {
        return PathBuf::from("static").join("web-app");
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("static")
        .join("web-app")
}

/// web-app 工件解压后累计未压缩体积上限（512 MiB）：防解压炸弹/写满磁盘。dist 实际通常 < 50 MiB，留足余量。
const MAX_WEBAPP_UNCOMPRESSED: u64 = 512 * 1024 * 1024;
/// web-app 工件条目数上限：防「海量空文件条目」耗尽 inode/dentry（体积闸对 0 字节条目失效）。
/// dist 实际通常数百~数千文件，5 万留足余量。
const MAX_WEBAPP_ENTRIES: usize = 50_000;

/// web-app 切激活的解包+原子切换（blocking）。生产用 static_web_app_dir() 作托管根。
/// 调用方约定：本函数成功后才翻转 DB 激活指针，故失败时托管根/状态不前移（详见 set_active）。
fn activate_web_bundle(
    version: &str,
    payload_path: &str,
    expected_sha256: &str,
    signature_b64: Option<&str>,
    public_key_b64: Option<&str>,
) -> Result<(), String> {
    activate_web_bundle_in(
        &static_web_app_dir(),
        version,
        payload_path,
        expected_sha256,
        signature_b64,
        public_key_b64,
    )
}

/// activate_web_bundle 的可注入托管根变体（测试用 tempdir 注入）。
/// 步骤：① 读工件 → sha256 完整性闸 + Ed25519 验签纵深 → ② 准备 serve_dir（绝不触碰活动目录）→ ③ 原子切 current。
fn activate_web_bundle_in(
    webapp_root: &std::path::Path,
    version: &str,
    payload_path: &str,
    expected_sha256: &str,
    signature_b64: Option<&str>,
    public_key_b64: Option<&str>,
) -> Result<(), String> {
    // 1. 完整性闸（sha256）+ 验签纵深（Ed25519）。验签防「DB sha256 与磁盘工件被同时篡改、但无法伪造签名」。
    let bytes = std::fs::read(payload_path).map_err(|e| format!("读工件 {payload_path} 失败: {e}"))?;
    let actual = {
        let mut h = Sha256::new();
        h.update(&bytes);
        hex_lower(&h.finalize())
    };
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        return Err(format!("sha256 不匹配: 期望 {expected_sha256} 实际 {actual}"));
    }
    // web-app 是托管根的安全敏感路径：验签强制（fail-closed），不降级到仅 sha256。
    // 缺签名或公钥不可用都硬失败——签名器在上传时即必备（upload 无签名器返 503），
    // 故正常发布的 web-app 版本必有签名；激活时若签名器不可用则拒绝激活而非放行被篡改工件。
    match (signature_b64, public_key_b64) {
        (Some(sig), Some(pk)) => {
            crate::services::resource_pack_signing::verify_base64(&bytes, sig, pk)
                .map_err(|e| format!("web-app 工件 Ed25519 验签失败: {e:?}"))?;
        }
        (Some(_), None) => {
            return Err("web-app 激活：签名器公钥不可用，无法验签（拒绝降级到仅 sha256）".to_string())
        }
        _ => return Err("web-app 版本缺签名，拒绝激活（web-app 工件必须签名）".to_string()),
    }
    drop(bytes);

    // 2. 准备 serve_dir：
    //   - version_dir 已含 index.html（root 或单层子目录）→ 视为完好发布，幂等复用，绝不 remove_dir_all 活动目录（消除停服窗口）。
    //     注：复用判据仅为「index.html 存在」，不做深度完整性校验（不校验 assets/* 等）；本路径自身经 staging+原子 rename 不会自产
    //     「有 index.html 却缺资源」的残缺目录，故仅历史遗留/外部污染可能命中，按低危接受。
    //   - version_dir 不存在或缺 index.html → 解包到 staging 校验含 index.html 后，原子 rename 换入 version_dir（staging/残缺目录均非 current 指向，删除无害）。
    let version_dir = webapp_root.join(version);
    let serve_dir = match locate_serve_dir(&version_dir) {
        Some(dir) => dir,
        None => {
            let staging = webapp_root.join(format!("{version}.staging"));
            if staging.exists() {
                std::fs::remove_dir_all(&staging).map_err(|e| format!("清理 staging 失败: {e}"))?;
            }
            std::fs::create_dir_all(&staging).map_err(|e| format!("创建 staging 失败: {e}"))?;
            // 解包出错（体积/条目/穿越/link 闸）时清理已部分写入的 staging，避免磁盘残留（与缺 index.html 分支一致）。
            if let Err(e) = extract_tar_gz_safe(
                std::path::Path::new(payload_path),
                &staging,
                MAX_WEBAPP_UNCOMPRESSED,
                MAX_WEBAPP_ENTRIES,
            ) {
                let _ = std::fs::remove_dir_all(&staging);
                return Err(e);
            }
            // staging 必须含 index.html（根或单层子目录），否则工件无效，提前失败不污染托管根
            if locate_serve_dir(&staging).is_none() {
                let _ = std::fs::remove_dir_all(&staging);
                return Err("解包内容缺少 index.html".to_string());
            }
            if version_dir.exists() {
                std::fs::remove_dir_all(&version_dir)
                    .map_err(|e| format!("清理残缺版本目录失败: {e}"))?;
            }
            std::fs::rename(&staging, &version_dir).map_err(|e| format!("换入版本目录失败: {e}"))?;
            locate_serve_dir(&version_dir).ok_or_else(|| "换入后缺少 index.html".to_string())?
        }
    };

    // 3. 原子切 current 符号链接 → serve_dir（相对 webapp_root）
    swap_current_symlink(webapp_root, &serve_dir)
}

/// 在 root 下定位含 index.html 的服务目录：root 根，或单层子目录（容错 `tar -C dist .` 多套一层）。
fn locate_serve_dir(root: &std::path::Path) -> Option<PathBuf> {
    if root.join("index.html").is_file() {
        return Some(root.to_path_buf());
    }
    std::fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.is_dir() && p.join("index.html").is_file())
}

/// 读 current 符号链接当前指向的相对目标（不存在/非链接则 None）。供激活失败回滚记录旧指向。
fn read_current_target(webapp_root: &std::path::Path) -> Option<PathBuf> {
    std::fs::read_link(webapp_root.join("current")).ok()
}

/// 把 current 恢复到激活前指向：prev=Some(相对目标)→切回该目标；prev=None（激活前无 current）→移除 current。
fn restore_current(webapp_root: &std::path::Path, prev: Option<PathBuf>) -> Result<(), String> {
    match prev {
        Some(target) => swap_current_symlink(webapp_root, &webapp_root.join(target)),
        None => {
            let current = webapp_root.join("current");
            match std::fs::remove_file(&current) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(format!("移除 current 失败: {e}")),
            }
        }
    }
}

/// 原子切换 static/web-app/current 符号链接指向 serve_dir（相对目标，可迁移）。
fn swap_current_symlink(webapp_root: &std::path::Path, serve_dir: &std::path::Path) -> Result<(), String> {
    let current = webapp_root.join("current");
    let tmp = webapp_root.join("current.tmp");
    let target = serve_dir
        .strip_prefix(webapp_root)
        .map_err(|e| format!("serve_dir 不在 webapp_root 下: {e}"))?;
    let _ = std::fs::remove_file(&tmp);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, &tmp).map_err(|e| format!("创建符号链接失败: {e}"))?;
        // rename 覆盖已存在的 current 符号链接，POSIX 原子
        std::fs::rename(&tmp, &current).map_err(|e| format!("切换 current 失败: {e}"))?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (target, current);
        Err("当前平台不支持符号链接切换".into())
    }
}

/// 安全解 tar.gz（拒绝绝对路径/.. 穿越/link + 累计未压缩体积上限，复用 updater::extract_tar_gz_safe 范式）。
fn extract_tar_gz_safe(
    tarball: &std::path::Path,
    dst: &std::path::Path,
    max_uncompressed: u64,
    max_entries: usize,
) -> Result<(), String> {
    let f = std::fs::File::open(tarball).map_err(|e| format!("打开工件失败: {e}"))?;
    let gz = flate2::read::GzDecoder::new(f);
    let mut archive = tar::Archive::new(gz);
    archive.set_overwrite(true);
    let dst_canon = std::fs::canonicalize(dst).unwrap_or_else(|_| dst.to_path_buf());
    let mut total: u64 = 0;
    let mut count: usize = 0;
    for entry_res in archive.entries().map_err(|e| format!("读 tar 条目失败: {e}"))? {
        let mut entry = entry_res.map_err(|e| format!("tar 条目失败: {e}"))?;
        // 条目数闸：防海量空条目耗尽 inode（体积闸对 0 字节条目无效）
        count += 1;
        if count > max_entries {
            return Err(format!("工件条目数超限: > {max_entries}"));
        }
        let rel = entry
            .path()
            .map_err(|e| format!("tar 路径失败: {e}"))?
            .into_owned();
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(format!("非法路径(穿越): {}", rel.to_string_lossy()));
        }
        if matches!(
            entry.header().entry_type(),
            tar::EntryType::Symlink | tar::EntryType::Link
        ) {
            return Err(format!("禁止 link 条目: {}", rel.to_string_lossy()));
        }
        // 解压炸弹防护：解包(写盘)前累计未压缩体积，超限即中止。
        // 必须用 entry.size()（PAX 感知，等于 unpack 实际 take 的字节数），不可用 header.size()——
        // 后者只读 ustar 头字段、不读 PAX size 覆盖，可被「ustar size=0 + PAX size=巨大」工件绕过。
        total = total.saturating_add(entry.size());
        if total > max_uncompressed {
            return Err(format!("解压体积超限: {total} > {max_uncompressed} 字节"));
        }
        let out = dst_canon.join(&rel);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
        }
        entry.unpack(&out).map_err(|e| format!("写出条目失败: {e}"))?;
    }
    Ok(())
}

fn static_pack_dir() -> PathBuf {
    // 运行时优先用 CWD 下的 static（生产 WorkingDirectory 即仓库根，与 `ServeDir::new("static")` 托管路径一致），
    // 否则回退编译期源码目录（仅便于本地测试）。沿用 user_profile::resolve_avatar_dir 同款范式。
    // 不可直接用 env!("CARGO_MANIFEST_DIR")：那是编译期路径，部署后落在 ProtectSystem 只读区会 create_dir_all 失败。
    let cwd_static = PathBuf::from("static");
    if cwd_static.is_dir() {
        return cwd_static.join("packs");
    }
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
    fn static_pack_dir_ends_with_static_packs() {
        // 不论走 CWD 优先分支还是编译期回退分支，落盘根都必须是 .../static/packs，
        // 与 ServeDir::new("static") 托管的 /packs/* 下载路径对齐。
        assert!(static_pack_dir().ends_with("static/packs"));
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

    // ── web-app 激活链路集成测试（L8） ───────────────────────────────────────

    /// 构造仅含普通文件的 tar.gz 字节。
    fn tar_gz_with(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        ));
        for &(path, data) in files {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o644);
            h.set_entry_type(tar::EntryType::Regular);
            builder.append_data(&mut h, path, data).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(bytes);
        hex_lower(&h.finalize())
    }

    fn new_signer() -> (
        tempfile::TempDir,
        crate::services::resource_pack_signing::ResourcePackSigner,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let signer =
            crate::services::resource_pack_signing::ResourcePackSigner::load_or_generate(dir.path())
                .unwrap();
        (dir, signer)
    }

    /// 写工件到临时文件，返回 (payload_dir, payload_path, sha256)。
    fn stage_payload(bytes: &[u8]) -> (tempfile::TempDir, PathBuf, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("payload.tar.gz");
        std::fs::write(&path, bytes).unwrap();
        let sha = sha256_hex(bytes);
        (dir, path, sha)
    }

    #[test]
    fn activate_web_bundle_extracts_signs_and_swaps_current() {
        let (_kd, signer) = new_signer();
        let tar = tar_gz_with(&[
            ("index.html", b"<html>v1</html>"),
            ("assets/app.js", b"console.log(1)"),
        ]);
        let (_pd, payload, sha) = stage_payload(&tar);
        let sig = signer.sign_base64(&tar);
        let root = tempfile::tempdir().unwrap();

        activate_web_bundle_in(
            root.path(),
            "1.0.0",
            payload.to_str().unwrap(),
            &sha,
            Some(&sig),
            Some(signer.public_key_base64()),
        )
        .unwrap();

        let current = root.path().join("current");
        assert!(current.join("index.html").is_file(), "current 应含 index.html");
        assert_eq!(
            std::fs::read_to_string(current.join("index.html")).unwrap(),
            "<html>v1</html>"
        );
        assert!(current.join("assets/app.js").is_file());
    }

    #[test]
    fn activate_web_bundle_rejects_sha_mismatch_and_leaves_no_current() {
        let (_kd, signer) = new_signer();
        let tar = tar_gz_with(&[("index.html", b"x")]);
        let (_pd, payload, _sha) = stage_payload(&tar);
        let sig = signer.sign_base64(&tar);
        let root = tempfile::tempdir().unwrap();

        let r = activate_web_bundle_in(
            root.path(),
            "1.0.0",
            payload.to_str().unwrap(),
            &"0".repeat(64),
            Some(&sig),
            Some(signer.public_key_base64()),
        );
        assert!(r.is_err(), "sha 不匹配应失败");
        // 失败时托管根不前移：current 不存在
        assert!(!root.path().join("current").exists());
    }

    #[test]
    fn activate_web_bundle_rejects_bad_signature() {
        let (_kd, signer) = new_signer();
        let tar = tar_gz_with(&[("index.html", b"x")]);
        let (_pd, payload, sha) = stage_payload(&tar);
        // 对其它字节签名 → 与工件不匹配
        let wrong_sig = signer.sign_base64(b"other-bytes");
        let root = tempfile::tempdir().unwrap();

        let r = activate_web_bundle_in(
            root.path(),
            "1.0.0",
            payload.to_str().unwrap(),
            &sha,
            Some(&wrong_sig),
            Some(signer.public_key_base64()),
        );
        assert!(r.is_err(), "验签失败应拒绝激活");
        assert!(!root.path().join("current").exists());
    }

    #[test]
    fn activate_web_bundle_requires_signature_present() {
        let tar = tar_gz_with(&[("index.html", b"x")]);
        let (_pd, payload, sha) = stage_payload(&tar);
        let root = tempfile::tempdir().unwrap();
        // 无签名 → 强制验签应硬失败（不降级到仅 sha256）
        let r = activate_web_bundle_in(root.path(), "1.0.0", payload.to_str().unwrap(), &sha, None, None);
        assert!(r.is_err(), "缺签名应拒绝激活");
        assert!(!root.path().join("current").exists());
    }

    #[test]
    fn restore_current_rolls_back_to_previous_and_removes_when_none() {
        let (_kd, signer) = new_signer();
        let pk = signer.public_key_base64();
        let root = tempfile::tempdir().unwrap();

        // 激活版本 A → current 指向 A
        let tar_a = tar_gz_with(&[("index.html", b"A")]);
        let (_a, pa, sa) = stage_payload(&tar_a);
        activate_web_bundle_in(root.path(), "a", pa.to_str().unwrap(), &sa, Some(&signer.sign_base64(&tar_a)), Some(pk)).unwrap();
        let prev = read_current_target(root.path());
        assert!(prev.is_some(), "current 应已指向 A");
        let cur_index = || std::fs::read_to_string(root.path().join("current").join("index.html")).unwrap();
        assert_eq!(cur_index(), "A");

        // 激活版本 B → current 指向 B
        let tar_b = tar_gz_with(&[("index.html", b"B")]);
        let (_b, pb, sb) = stage_payload(&tar_b);
        activate_web_bundle_in(root.path(), "b", pb.to_str().unwrap(), &sb, Some(&signer.sign_base64(&tar_b)), Some(pk)).unwrap();
        assert_eq!(cur_index(), "B");

        // 回滚到 A（模拟 DB 翻转失败补偿）
        restore_current(root.path(), prev).unwrap();
        assert_eq!(cur_index(), "A");

        // 回滚到 None（激活前无 current）→ 移除 current，且幂等
        restore_current(root.path(), None).unwrap();
        assert!(!root.path().join("current").exists());
        restore_current(root.path(), None).unwrap();
    }

    #[test]
    fn activate_web_bundle_requires_public_key_available() {
        let (_kd, signer) = new_signer();
        let tar = tar_gz_with(&[("index.html", b"x")]);
        let (_pd, payload, sha) = stage_payload(&tar);
        let sig = signer.sign_base64(&tar);
        let root = tempfile::tempdir().unwrap();
        // 有签名但公钥不可用 → 强制验签应硬失败（不降级到仅 sha256）
        let r = activate_web_bundle_in(root.path(), "1.0.0", payload.to_str().unwrap(), &sha, Some(&sig), None);
        assert!(r.is_err(), "公钥不可用应拒绝激活");
        assert!(!root.path().join("current").exists());
    }

    #[test]
    fn activate_web_bundle_reactivate_same_version_is_idempotent_no_staging() {
        let (_kd, signer) = new_signer();
        let tar = tar_gz_with(&[("index.html", b"<html>v</html>")]);
        let (_pd, payload, sha) = stage_payload(&tar);
        let sig = signer.sign_base64(&tar);
        let root = tempfile::tempdir().unwrap();
        let args = |root: &std::path::Path| {
            activate_web_bundle_in(
                root,
                "2.0.0",
                payload.to_str().unwrap(),
                &sha,
                Some(&sig),
                Some(signer.public_key_base64()),
            )
        };
        args(root.path()).unwrap();
        // 重复激活同版本：幂等成功，且不应残留 staging（说明未触碰/重解包活动目录）
        args(root.path()).unwrap();
        assert!(root.path().join("current").join("index.html").is_file());
        assert!(
            !root.path().join("2.0.0.staging").exists(),
            "重激活不应产生 staging（应复用已存在的完好版本目录）"
        );
    }

    #[test]
    fn activate_web_bundle_handles_single_nested_dir() {
        let (_kd, signer) = new_signer();
        // 多套一层 dist/ 目录
        let tar = tar_gz_with(&[("dist/index.html", b"<html>nested</html>")]);
        let (_pd, payload, sha) = stage_payload(&tar);
        let sig = signer.sign_base64(&tar);
        let root = tempfile::tempdir().unwrap();

        activate_web_bundle_in(
            root.path(),
            "3.0.0",
            payload.to_str().unwrap(),
            &sha,
            Some(&sig),
            Some(signer.public_key_base64()),
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(root.path().join("current").join("index.html")).unwrap(),
            "<html>nested</html>"
        );
    }

    #[test]
    fn activate_web_bundle_rejects_missing_index_html() {
        let (_kd, signer) = new_signer();
        let tar = tar_gz_with(&[("assets/app.js", b"x")]);
        let (_pd, payload, sha) = stage_payload(&tar);
        let sig = signer.sign_base64(&tar);
        let root = tempfile::tempdir().unwrap();

        let r = activate_web_bundle_in(
            root.path(),
            "4.0.0",
            payload.to_str().unwrap(),
            &sha,
            Some(&sig),
            Some(signer.public_key_base64()),
        );
        assert!(r.is_err(), "缺 index.html 应失败");
        assert!(!root.path().join("current").exists());
    }

    #[test]
    fn activate_web_bundle_cleans_staging_on_extract_failure() {
        let (_kd, signer) = new_signer();
        // 穿越条目触发 extract 安全闸失败（体积 512MiB 难在测试构造，用穿越等价覆盖错误路径）
        let tar = gz(&raw_tar_single("../evil.txt", b"pwn"));
        let (_pd, payload, sha) = stage_payload(&tar);
        let sig = signer.sign_base64(&tar);
        let root = tempfile::tempdir().unwrap();

        let r = activate_web_bundle_in(
            root.path(),
            "9.0.0",
            payload.to_str().unwrap(),
            &sha,
            Some(&sig),
            Some(signer.public_key_base64()),
        );
        assert!(r.is_err(), "穿越工件应激活失败");
        assert!(
            !root.path().join("9.0.0.staging").exists(),
            "extract 失败后 staging 应被清理，不留磁盘残留"
        );
        assert!(!root.path().join("current").exists());
    }

    /// 手写裸 tar 头（USTAR）注入任意 name（tar::Builder 在写时即拒绝 `..`，故绕过它构造恶意工件）。
    fn raw_tar_single(name: &str, data: &[u8]) -> Vec<u8> {
        let mut h = [0u8; 512];
        let nb = name.as_bytes();
        h[..nb.len()].copy_from_slice(nb);
        h[100..108].copy_from_slice(b"0000644\0");
        h[108..116].copy_from_slice(b"0000000\0");
        h[116..124].copy_from_slice(b"0000000\0");
        h[124..136].copy_from_slice(format!("{:011o}\0", data.len()).as_bytes());
        h[136..148].copy_from_slice(b"00000000000\0");
        h[156] = b'0'; // typeflag: regular
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
        for b in &mut h[148..156] {
            *b = b' ';
        }
        let sum: u32 = h.iter().map(|&b| b as u32).sum();
        h[148..156].copy_from_slice(format!("{:06o}\0 ", sum).as_bytes());
        let mut out = Vec::new();
        out.extend_from_slice(&h);
        out.extend_from_slice(data);
        out.extend(std::iter::repeat(0u8).take((512 - data.len() % 512) % 512));
        out.extend(std::iter::repeat(0u8).take(1024)); // 两个 EOF 空块
        out
    }

    fn gz(bytes: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut e =
            flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(bytes).unwrap();
        e.finish().unwrap()
    }

    #[test]
    fn extract_tar_gz_safe_rejects_path_traversal() {
        let tar = gz(&raw_tar_single("../evil.txt", b"pwn"));
        let (_pd, payload, _sha) = stage_payload(&tar);
        let dst = tempfile::tempdir().unwrap();
        let r = extract_tar_gz_safe(&payload, dst.path(), MAX_WEBAPP_UNCOMPRESSED, MAX_WEBAPP_ENTRIES);
        assert!(r.is_err(), "穿越路径应被拒绝");
        assert!(r.unwrap_err().contains("穿越"));
    }

    #[test]
    fn extract_tar_gz_safe_rejects_symlink_entry() {
        // 手工构造 symlink 条目
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::default(),
        ));
        let mut h = tar::Header::new_gnu();
        h.set_entry_type(tar::EntryType::Symlink);
        h.set_size(0);
        h.set_mode(0o777);
        h.set_link_name("/etc/passwd").unwrap();
        builder
            .append_data(&mut h, "evil-link", std::io::empty())
            .unwrap();
        let tar = builder.into_inner().unwrap().finish().unwrap();
        let (_pd, payload, _sha) = stage_payload(&tar);
        let dst = tempfile::tempdir().unwrap();

        let r = extract_tar_gz_safe(&payload, dst.path(), MAX_WEBAPP_UNCOMPRESSED, MAX_WEBAPP_ENTRIES);
        assert!(r.is_err(), "symlink 条目应被拒绝");
        assert!(r.unwrap_err().contains("link"));
    }

    #[test]
    fn extract_tar_gz_safe_enforces_size_cap() {
        let tar = tar_gz_with(&[("index.html", &[b'a'; 100][..])]);
        let (_pd, payload, _sha) = stage_payload(&tar);
        let dst = tempfile::tempdir().unwrap();
        // max=10 < 100 字节 → 超限
        let r = extract_tar_gz_safe(&payload, dst.path(), 10, MAX_WEBAPP_ENTRIES);
        assert!(r.is_err(), "超体积上限应中止");
        assert!(r.unwrap_err().contains("解压体积超限"));
    }

    #[test]
    fn extract_tar_gz_safe_enforces_entry_count_cap() {
        // 3 个 0 字节条目，体积闸（512MiB）永不触发，但条目数闸 max=2 应中止——
        // 复现「海量空条目耗 inode」炸弹被拦截。
        let tar = tar_gz_with(&[
            ("a.txt", b""),
            ("b.txt", b""),
            ("c.txt", b""),
        ]);
        let (_pd, payload, _sha) = stage_payload(&tar);
        let dst = tempfile::tempdir().unwrap();
        let r = extract_tar_gz_safe(&payload, dst.path(), MAX_WEBAPP_UNCOMPRESSED, 2);
        assert!(r.is_err(), "超条目数上限应中止");
        assert!(r.unwrap_err().contains("条目数超限"));
    }
}
