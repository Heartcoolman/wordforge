use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::Router;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::auth::AdminAuthUser;
use crate::extractors::JsonBody;
use crate::response::{ok, AppError};
use crate::state::{AppState, SseEvent};
use crate::store::operations::clients::{ClientDevice, DataChannelStatus};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_clients))
        // m027:设备表后端分页 + 平台聚合 + 升级策略 CRUD + 强制升级广播
        .route("/paginated", get(list_clients_paginated))
        .route("/distribution", get(get_distribution))
        // m054:关联风控标记复核(B 层封禁绕过缓解)。静态路由须在 `/:id` 前注册。
        .route("/flagged", get(list_flagged_clients))
        .route("/upgrade-policy", get(list_upgrade_policy_handler))
        .route("/upgrade-policy/:platform", put(put_upgrade_policy_handler))
        .route(
            "/broadcast-upgrade/:platform",
            post(broadcast_upgrade_handler),
        )
        // 撤销强升:向该平台全部活跃 SSE 连接下发 upgrade_cleared,客户端清锁恢复。
        .route(
            "/broadcast-upgrade/:platform/revoke",
            post(revoke_upgrade_handler),
        )
        .route("/:id", get(get_client_detail))
        .route("/:id/ban", post(ban_client))
        .route("/:id/unban", post(unban_client))
        .route("/:id/clear-flag", post(clear_client_flag))
        .route("/:id/request-telemetry", post(request_telemetry))
}

pub fn telemetry_router() -> Router<AppState> {
    Router::new()
        // 静态路由须在 `/:device_id` 之前注册,避免被动态段吞掉。
        .route("/ownership-states", get(ownership_states))
        .route("/ingest-rejections", get(ingest_rejections))
        .route("/:device_id", get(get_telemetry))
        .route("/:device_id/summary", get(get_telemetry_summary))
}

/// GET /api/admin/telemetry/ownership-states —— 设备归属态计数(Telemetry 看板)。
/// claimed/unclaimed 由 client_devices.user_id 是否为空直接派生；mismatch/notRegistered
/// 是逐请求摄取结果(异主/无行),不可从表统计 → 恒返回 null。
async fn ownership_states(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let (claimed, unclaimed) = state
        .run_store_task(
            "admin.telemetry.ownership_states",
            move |store| -> Result<_, AppError> { Ok(store.admin_device_ownership_counts()?) },
        )
        .await??;
    Ok(ok(serde_json::json!({
        "claimed": claimed,
        "unclaimed": unclaimed,
        "mismatch": serde_json::Value::Null,
        "notRegistered": serde_json::Value::Null,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IngestRejectionsQuery {
    days: Option<u32>,
}

/// GET /api/admin/telemetry/ingest-rejections —— 近 N 天摄取拒绝码分布(Telemetry 看板)。
/// 数据来自 telemetry_ingest_rejections(m061，摄取早返时旁路留痕)；pct 在 Rust 计算。
async fn ingest_rejections(
    _admin: AdminAuthUser,
    Query(q): Query<IngestRejectionsQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let days = q.days.unwrap_or(7).clamp(1, 90);
    let rows = state
        .run_store_task(
            "admin.telemetry.ingest_rejections",
            move |store| -> Result<_, AppError> { Ok(store.aggregate_ingest_rejections(days)?) },
        )
        .await??;
    let total: u64 = rows.iter().map(|(_, c)| *c as u64).sum();
    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|(code, count)| {
            let count = count as u64;
            serde_json::json!({
                "code": code,
                "count": count,
                "pct": if total > 0 { count as f64 / total as f64 * 100.0 } else { 0.0 },
            })
        })
        .collect();
    Ok(ok(serde_json::json!({ "total": total, "items": items })))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SseLiveEntry {
    device_id: String,
    platform: String,
    user_id: String,
    connected_secs: u64,
    connection_count: usize,
    is_banned: bool,
    data_channels: DataChannelStatus,
    /// m022:`x-app-version` 头落库后透出。SseClientInfo 不存版本,这里从 client_devices 表反查。
    app_version: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecentlyActiveEntry {
    device_id: String,
    platform: String,
    user_id: Option<String>,
    last_seen_at: String,
    is_banned: bool,
    data_channels: DataChannelStatus,
    /// m022:同上,直接来自 ClientDevice.app_version 字段。
    app_version: Option<String>,
    /// m054:关联风控标记(共享 IP/账号被牵连),供设备列表内联红点提示。
    risk_flag: bool,
    risk_related_device: Option<String>,
}

async fn list_clients(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // SSE live connections (deduplicated by device_id)
    let sse_live: Vec<SseLiveEntry> = state
        .active_sse()
        .iter()
        .filter_map(|entry| {
            let device_id = entry.key().clone();
            let conns = entry.value();
            let first = conns.first()?;
            Some(SseLiveEntry {
                device_id: device_id.clone(),
                platform: first.platform.clone(),
                user_id: first.user_id.clone(),
                connected_secs: first.connected_at.elapsed().as_secs(),
                connection_count: conns.len(),
                is_banned: false,
                data_channels: DataChannelStatus::default(),
                app_version: None, // 稍后在 store task 后填充
            })
        })
        .collect();

    let live_user_ids: Vec<String> = sse_live.iter().map(|entry| entry.user_id.clone()).collect();
    let live_device_ids: Vec<String> = sse_live
        .iter()
        .map(|entry| entry.device_id.clone())
        .collect();
    let (banned_by_device, recently_active_devices, status, sse_app_versions) = state
        .run_store_task("admin.clients.list", move |store| -> Result<_, AppError> {
            let banned_by_device = live_device_ids
                .iter()
                .map(|device_id| Ok((device_id.clone(), store.is_device_banned(device_id)?)))
                .collect::<Result<std::collections::HashMap<String, bool>, crate::store::StoreError>>()?;

            // m022:为 SSE live 设备查 app_version(recently_active 自带 app_version 字段)
            let sse_app_versions = store.get_app_versions_for_devices(&live_device_ids)?;

            let recently_active_devices =
                exclude_live_devices(store.get_recently_active_clients(15)?, &live_device_ids);
            let user_ids: Vec<String> = live_user_ids
                .into_iter()
                .chain(
                    recently_active_devices
                        .iter()
                        .filter_map(|entry| entry.user_id.clone()),
                )
                .collect();
            let device_ids: Vec<String> = live_device_ids
                .iter()
                .cloned()
                .chain(
                    recently_active_devices
                        .iter()
                        .map(|entry| entry.device_id.clone()),
                )
                .collect();
            let status = store.get_data_upload_status(&user_ids, &device_ids)?;
            Ok((banned_by_device, recently_active_devices, status, sse_app_versions))
        })
        .await??;

    let mut sse_live = sse_live;
    for entry in &mut sse_live {
        entry.is_banned = banned_by_device
            .get(&entry.device_id)
            .copied()
            .unwrap_or(false);
        entry.app_version = sse_app_versions
            .get(&entry.device_id)
            .cloned()
            .unwrap_or(None);
    }

    let recently_active: Vec<RecentlyActiveEntry> = recently_active_devices
        .iter()
        .map(|d| RecentlyActiveEntry {
            device_id: d.device_id.clone(),
            platform: d.platform.clone(),
            user_id: d.user_id.clone(),
            last_seen_at: d.last_seen_at.clone(),
            is_banned: d.is_banned,
            data_channels: DataChannelStatus::default(),
            app_version: d.app_version.clone(),
            risk_flag: d.risk_flag,
            risk_related_device: d.risk_related_device.clone(),
        })
        .collect();

    for entry in &mut sse_live {
        let amas = status
            .amas_by_user
            .get(&entry.user_id)
            .copied()
            .unwrap_or("none");
        // AMAS exists but no events means learning is also "nil"
        let learning = status
            .learning_by_user
            .get(&entry.user_id)
            .copied()
            .unwrap_or_else(|| if amas != "none" { "nil" } else { "none" });
        let telemetry = status
            .telemetry_by_device
            .get(&entry.device_id)
            .copied()
            .unwrap_or("none");
        entry.data_channels = DataChannelStatus {
            amas,
            learning,
            telemetry,
        };
    }

    let mut recently_active = recently_active;
    for entry in &mut recently_active {
        let (amas, learning) = match &entry.user_id {
            Some(uid) => {
                let a = status.amas_by_user.get(uid).copied().unwrap_or("none");
                // AMAS exists but no events means learning is also "nil"
                let l = status
                    .learning_by_user
                    .get(uid)
                    .copied()
                    .unwrap_or_else(|| if a != "none" { "nil" } else { "none" });
                (a, l)
            }
            None => ("none", "none"),
        };
        let telemetry = status
            .telemetry_by_device
            .get(&entry.device_id)
            .copied()
            .unwrap_or("none");
        entry.data_channels = DataChannelStatus {
            amas,
            learning,
            telemetry,
        };
    }

    Ok(ok(serde_json::json!({
        "sseLive": sse_live,
        "recentlyActive": recently_active,
    })))
}

fn exclude_live_devices(
    devices: Vec<ClientDevice>,
    live_device_ids: &[String],
) -> Vec<ClientDevice> {
    let live: HashSet<&str> = live_device_ids.iter().map(String::as_str).collect();
    devices
        .into_iter()
        .filter(|device| !live.contains(device.device_id.as_str()))
        .collect()
}

#[derive(Deserialize)]
struct BanRequest {
    reason: Option<String>,
}

async fn ban_client(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
    body: Option<JsonBody<BanRequest>>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let reason = body.and_then(|JsonBody(b)| {
        b.reason
            .filter(|r| !r.is_empty())
            .map(|r| r.chars().take(500).collect::<String>())
    });
    let device_id_for_store = id.clone();
    let admin_id = admin.admin_id.clone();

    let reason_for_audit = reason.clone();
    let flagged = state
        .run_store_task(
            "admin.clients.ban",
            move |store| -> Result<Vec<String>, AppError> {
                if !store.client_device_exists(&device_id_for_store)? {
                    return Err(AppError::not_found("设备不存在"));
                }
                // 封禁 + m054 关联打标在单事务内原子完成:全成功或全回滚,
                // 避免关联打标失败时封禁已落库却返回 500 的状态不一致。
                let (_, flagged) = store.ban_device_with_flagging(
                    &device_id_for_store,
                    &admin_id,
                    reason.as_deref(),
                )?;
                // 审计为尽力而为(失败仅告警,不回滚封禁):与用户封禁审计口径一致。
                let _ = store.insert_admin_audit(
                    &admin_id,
                    "client.device.ban",
                    Some("device"),
                    Some(&device_id_for_store),
                    Some(&serde_json::json!({
                        "reason": reason_for_audit,
                        "flaggedRelated": flagged.len(),
                    })),
                );
                Ok(flagged)
            },
        )
        .await??;

    // Notify via SSE but keep connection alive for instant unban
    if let Some(conns) = state.active_sse().get(&id) {
        for conn in conns.value() {
            let _ = conn.tx.try_send(SseEvent::Banned);
        }
    }

    tracing::info!(admin_id = %admin.admin_id, device_id = %id, flagged_related = flagged.len(), "管理员封禁设备");
    Ok(ok(serde_json::json!({
        "banned": true,
        "deviceId": id,
        "flaggedRelated": flagged.len(),
        "flaggedDeviceIds": flagged,
    })))
}

async fn unban_client(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let device_id_for_store = id.clone();
    let admin_id = admin.admin_id.clone();
    let cleared = state
        .run_store_task(
            "admin.clients.unban",
            move |store| -> Result<usize, AppError> {
                if !store.client_device_exists(&device_id_for_store)? {
                    return Err(AppError::not_found("设备不存在"));
                }
                // 解封 + 重算关联标记在单事务内原子完成。重算(而非盲清)避免多源牵连时错清/漏清。
                let (_, cleared) =
                    store.unban_device_with_flag_recompute(&device_id_for_store)?;
                let _ = store.insert_admin_audit(
                    &admin_id,
                    "client.device.unban",
                    Some("device"),
                    Some(&device_id_for_store),
                    Some(&serde_json::json!({ "clearedRelated": cleared })),
                );
                Ok(cleared)
            },
        )
        .await??;

    // Notify via existing SSE connection for instant unban
    if let Some(conns) = state.active_sse().get(&id) {
        for conn in conns.value() {
            let _ = conn.tx.try_send(SseEvent::Unbanned);
        }
    }

    tracing::info!(admin_id = %admin.admin_id, device_id = %id, cleared_related = cleared, "管理员解封设备");
    Ok(ok(serde_json::json!({ "banned": false, "deviceId": id, "clearedRelated": cleared })))
}

/// m054:关联风控复核列表项。脱敏:不暴露 last_ip / banned_by(与设备详情口径一致)。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FlaggedClientEntry {
    device_id: String,
    platform: String,
    user_id: Option<String>,
    last_seen_at: String,
    is_banned: bool,
    app_version: Option<String>,
    risk_reason: Option<String>,
    risk_flagged_at: Option<String>,
    risk_related_device: Option<String>,
}

impl From<&ClientDevice> for FlaggedClientEntry {
    fn from(d: &ClientDevice) -> Self {
        Self {
            device_id: d.device_id.clone(),
            platform: d.platform.clone(),
            user_id: d.user_id.clone(),
            last_seen_at: d.last_seen_at.clone(),
            is_banned: d.is_banned,
            app_version: d.app_version.clone(),
            risk_reason: d.risk_reason.clone(),
            risk_flagged_at: d.risk_flagged_at.clone(),
            risk_related_device: d.risk_related_device.clone(),
        }
    }
}

/// m054:GET /api/admin/clients/flagged —— 列当前被关联风控标记的设备(按打标时间倒序),
/// 供 admin 复核;每条注明触发源设备与命中信号。
async fn list_flagged_clients(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let devices = state
        .run_store_task(
            "admin.clients.flagged",
            move |store| -> Result<_, AppError> { Ok(store.list_flagged_devices(200)?) },
        )
        .await??;
    let items: Vec<FlaggedClientEntry> = devices.iter().map(FlaggedClientEntry::from).collect();
    Ok(ok(serde_json::json!({ "flagged": items })))
}

/// m054:POST /api/admin/clients/:id/clear-flag —— 复核判定误报,清除单设备的关联标记。
async fn clear_client_flag(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let device_id = id.clone();
    let admin_id = admin.admin_id.clone();
    let cleared = state
        .run_store_task(
            "admin.clients.clear_flag",
            move |store| -> Result<bool, AppError> {
                let cleared = store.clear_device_risk_flag(&device_id)?;
                let _ = store.insert_admin_audit(
                    &admin_id,
                    "client.device.clear_flag",
                    Some("device"),
                    Some(&device_id),
                    Some(&serde_json::json!({ "cleared": cleared })),
                );
                Ok(cleared)
            },
        )
        .await??;
    tracing::info!(admin_id = %admin.admin_id, device_id = %id, "管理员清除设备关联风控标记");
    Ok(ok(serde_json::json!({ "cleared": cleared, "deviceId": id })))
}

async fn request_telemetry(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let conns = state
        .active_sse()
        .get(&id)
        .filter(|c| !c.is_empty())
        .ok_or_else(|| AppError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "DEVICE_OFFLINE".into(),
            message: "设备当前无活跃 SSE 连接".into(),
            is_operational: true,
        })?;

    // P2: 下发前查该 device 当前 owner，只投递给 user_id == owner 的连接，
    // 防 owner 变更后陈旧连接（旧用户）跨用户收到定向 telemetry_request。
    // Some(Some(uid)) = 已认领归属；Some(None)/None = 无确定 owner → 不投递。
    let device_for_owner = id.clone();
    let owner = state
        .run_store_task(
            "admin.clients.request_telemetry.owner",
            move |store| -> Result<_, AppError> {
                Ok(store.get_client_device_owner(&device_for_owner)?.flatten())
            },
        )
        .await??;
    let owner = match owner.as_deref() {
        Some(uid) => uid.to_string(),
        None => {
            return Err(AppError {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "DEVICE_OWNER_UNKNOWN".into(),
                message: "设备无确定归属（未注册/未认领），不向陈旧连接下发遥测请求".into(),
                is_operational: true,
            });
        }
    };

    let request_id = uuid::Uuid::new_v4().to_string();
    let event = SseEvent::TelemetryRequest {
        request_id: request_id.clone(),
    };
    for conn in conns.value() {
        if conn.user_id != owner {
            continue;
        }
        let _ = conn.tx.try_send(event.clone());
    }

    Ok(ok(serde_json::json!({ "requestId": request_id })))
}

/// m027:设备表后端分页 + 搜索 + 平台过滤。返回 paginated envelope。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PaginatedClientsQuery {
    page: Option<u32>,
    per_page: Option<u32>,
    q: Option<String>,
    platform: Option<String>,
    /// 仅保留 N 分钟内活跃(也包含 banned)。None 表示不过滤(全表 + 历史)。
    recent_minutes: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ListedDevice {
    device_id: String,
    platform: String,
    user_id: Option<String>,
    app_version: Option<String>,
    model: Option<String>,
    country: Option<String>,
    first_seen_at: String,
    last_seen_at: String,
    is_banned: bool,
}

impl From<ClientDevice> for ListedDevice {
    fn from(d: ClientDevice) -> Self {
        Self {
            device_id: d.device_id,
            platform: d.platform,
            user_id: d.user_id,
            app_version: d.app_version,
            model: d.model,
            country: d.country,
            first_seen_at: d.first_seen_at,
            last_seen_at: d.last_seen_at,
            is_banned: d.is_banned,
            // 故意不暴露 last_ip / banned_by / ban_reason 等审计敏感字段
        }
    }
}

async fn list_clients_paginated(
    _admin: AdminAuthUser,
    Query(q): Query<PaginatedClientsQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let page = q.page.unwrap_or(1).max(1) as i64;
    let per_page = q.per_page.unwrap_or(20).clamp(1, 200) as i64;
    let offset = (page - 1) * per_page;
    let (rows, total) = state
        .run_store_task(
            "admin.clients.list_paginated",
            move |store| -> Result<_, AppError> {
                Ok(store.list_client_devices_paginated(
                    q.q.as_deref(),
                    q.platform.as_deref(),
                    q.recent_minutes,
                    per_page,
                    offset,
                )?)
            },
        )
        .await??;
    let data: Vec<ListedDevice> = rows.into_iter().map(Into::into).collect();
    let total_pages = if total == 0 {
        0
    } else {
        (total + per_page - 1) / per_page
    };
    Ok(ok(serde_json::json!({
        "data": data,
        "total": total,
        "page": page,
        "perPage": per_page,
        "totalPages": total_pages,
    })))
}

/// m027:平台聚合 + 平台×版本分布 + 升级策略快照(给前端一站式渲染 hero+柱状+面板)。
async fn get_distribution(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let (platforms, versions, policies) = state
        .run_store_task(
            "admin.clients.distribution",
            move |store| -> Result<_, AppError> {
                Ok((
                    store.aggregate_clients_by_platform()?,
                    store.aggregate_clients_by_platform_version()?,
                    store.list_upgrade_policies()?,
                ))
            },
        )
        .await??;

    let platforms_json: Vec<serde_json::Value> = platforms
        .into_iter()
        .map(|(platform, total, active7d, pct)| {
            serde_json::json!({
                "platform": platform,
                "total": total,
                "active7d": active7d,
                "monthOverMonthPct": (pct * 10.0).round() / 10.0,
            })
        })
        .collect();
    let versions_json: Vec<serde_json::Value> = versions
        .into_iter()
        .map(|(platform, version, count)| {
            serde_json::json!({
                "platform": platform,
                "version": version,
                "count": count,
            })
        })
        .collect();
    Ok(ok(serde_json::json!({
        "platforms": platforms_json,
        "versions": versions_json,
        "policies": policies,
    })))
}

async fn list_upgrade_policy_handler(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let policies = state
        .run_store_task(
            "admin.clients.list_upgrade_policy",
            |store| -> Result<_, AppError> { Ok(store.list_upgrade_policies()?) },
        )
        .await??;
    Ok(ok(serde_json::json!({ "policies": policies })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpgradePolicyPayload {
    min_version: Option<String>,
    suggested_version: Option<String>,
    /// 0-100 整数;超出范围 400。
    grayscale_pct: Option<i64>,
    pwa_silent_update: Option<bool>,
}

fn normalize_platform(s: &str) -> Result<&'static str, AppError> {
    match s {
        "web" => Ok("web"),
        "ios" => Ok("ios"),
        "android" => Ok("android"),
        _ => Err(AppError::bad_request(
            "INVALID_PLATFORM",
            "平台必须是 web / ios / android 之一",
        )),
    }
}

async fn put_upgrade_policy_handler(
    admin: AdminAuthUser,
    Path(platform): Path<String>,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<UpgradePolicyPayload>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let platform = normalize_platform(&platform)?.to_string();
    let pct = req.grayscale_pct.unwrap_or(0);
    if !(0..=100).contains(&pct) {
        return Err(AppError::bad_request(
            "INVALID_GRAYSCALE",
            "灰度百分比需在 0-100 之间",
        ));
    }
    let pwa_silent = req.pwa_silent_update.unwrap_or(true);
    let admin_id = admin.admin_id.clone();
    let platform_for_db = platform.clone();
    let min_for_db = req.min_version.clone();
    let sug_for_db = req.suggested_version.clone();
    let admin_for_db = admin_id.clone();
    state
        .run_store_task(
            "admin.clients.upsert_upgrade_policy",
            move |store| -> Result<_, AppError> {
                store.upsert_upgrade_policy(
                    &platform_for_db,
                    min_for_db.as_deref(),
                    sug_for_db.as_deref(),
                    pct,
                    pwa_silent,
                    &admin_for_db,
                )?;
                let _ = store.insert_admin_audit(
                    &admin_for_db,
                    "client.upgrade_policy.update",
                    Some("platform"),
                    Some(&platform_for_db),
                    Some(&serde_json::json!({
                        "minVersion": min_for_db,
                        "suggestedVersion": sug_for_db,
                        "grayscalePct": pct,
                        "pwaSilentUpdate": pwa_silent,
                    })),
                );
                Ok(())
            },
        )
        .await??;
    state.invalidate_upgrade_cache();
    tracing::info!(admin_id = %admin.admin_id, platform = %platform, "升级策略已更新");
    Ok(ok(serde_json::json!({ "ok": true, "platform": platform })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BroadcastUpgradePayload {
    /// 目标"低于该版本"的设备会被推送 SSE 强制升级事件。
    below_version: String,
    latest_version: String,
    message: Option<String>,
}

async fn broadcast_upgrade_handler(
    admin: AdminAuthUser,
    Path(platform): Path<String>,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<BroadcastUpgradePayload>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let platform = normalize_platform(&platform)?;
    let below = req.below_version.trim().to_string();
    let latest = req.latest_version.trim().to_string();
    if below.is_empty() || latest.is_empty() {
        return Err(AppError::bad_request(
            "INVALID_VERSION",
            "belowVersion 与 latestVersion 不能为空",
        ));
    }

    // 取该平台全部设备的 (device_id, app_version) 一致快照(单次查询),按 version < below
    // 在 Rust 内过滤。替代此前的 OFFSET 分页循环——后者按易变 last_seen_at 排序,扫描期间
    // 设备活跃刷新会使 OFFSET 窗口错位导致漏推/重复推。广播是低频 admin 操作,全量可接受。
    let platform_owned = platform.to_string();
    let below_for_db = below.clone();
    let targets: Vec<String> = state
        .run_store_task(
            "admin.clients.broadcast_upgrade.list",
            move |store| -> Result<_, AppError> {
                let rows = store.list_device_versions_for_platform(&platform_owned)?;
                let t = below_for_db.trim_start_matches('v');
                let target_ver = semver::Version::parse(t).ok();
                let out: Vec<String> = rows
                    .into_iter()
                    .filter(|(_, app_version)| {
                        match (app_version.as_deref(), target_ver.as_ref()) {
                            (Some(v), Some(tv)) => {
                                let a = v.trim_start_matches('v');
                                semver::Version::parse(a).map(|av| av < *tv).unwrap_or(false)
                            }
                            _ => false,
                        }
                    })
                    .map(|(device_id, _)| device_id)
                    .collect();
                Ok(out)
            },
        )
        .await??;

    let mut hit = 0usize;
    for device_id in &targets {
        if let Some(conns) = state.active_sse().get(device_id) {
            let event = SseEvent::UpgradeRequired {
                latest_version: latest.clone(),
                message: req.message.clone(),
            };
            for conn in conns.value() {
                if conn.tx.try_send(event.clone()).is_ok() {
                    hit += 1;
                }
            }
        }
    }

    let admin_id = admin.admin_id.clone();
    let platform_str = platform.to_string();
    let targets_len = targets.len() as i64;
    let _ = state
        .run_store_task(
            "admin.clients.broadcast_upgrade.audit",
            move |store| -> Result<(), AppError> {
                store.insert_admin_audit(
                    &admin_id,
                    "client.broadcast_upgrade",
                    Some("platform"),
                    Some(&platform_str),
                    Some(&serde_json::json!({
                        "belowVersion": below,
                        "latestVersion": latest,
                        "matched": targets_len,
                        "pushedConnections": hit,
                    })),
                )?;
                Ok(())
            },
        )
        .await;

    tracing::info!(
        admin_id = %admin.admin_id, platform = %platform,
        matched = targets.len(), pushed_connections = hit,
        "强制升级广播已派发"
    );
    Ok(ok(serde_json::json!({
        "matched": targets.len(),
        "pushedConnections": hit,
    })))
}

/// POST /admin/clients/broadcast-upgrade/:platform/revoke —— 撤销该平台强制升级。
/// 向该平台全部设备的活跃 SSE 连接定向下发 `upgrade_cleared`,客户端收到即清除强升锁、
/// 恢复正常会话。未被强升的客户端静默忽略。仍真正低于全局版本门的客户端会在下一次
/// 受检请求时重新被 CLIENT_OUTDATED 拦截(自校正),故平台级广播解除是安全的。
async fn revoke_upgrade_handler(
    admin: AdminAuthUser,
    Path(platform): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let platform = normalize_platform(&platform)?;
    let platform_owned = platform.to_string();
    let device_ids: Vec<String> = state
        .run_store_task(
            "admin.clients.revoke_upgrade.list",
            move |store| -> Result<_, AppError> {
                let rows = store.list_device_versions_for_platform(&platform_owned)?;
                Ok(rows.into_iter().map(|(device_id, _)| device_id).collect())
            },
        )
        .await??;

    let mut hit = 0usize;
    for device_id in &device_ids {
        if let Some(conns) = state.active_sse().get(device_id) {
            for conn in conns.value() {
                if conn.tx.try_send(SseEvent::UpgradeCleared).is_ok() {
                    hit += 1;
                }
            }
        }
    }

    let admin_id = admin.admin_id.clone();
    let platform_str = platform.to_string();
    let devices_len = device_ids.len() as i64;
    let _ = state
        .run_store_task(
            "admin.clients.revoke_upgrade.audit",
            move |store| -> Result<(), AppError> {
                store.insert_admin_audit(
                    &admin_id,
                    "client.revoke_upgrade",
                    Some("platform"),
                    Some(&platform_str),
                    Some(&serde_json::json!({
                        "devices": devices_len,
                        "pushedConnections": hit,
                    })),
                )?;
                Ok(())
            },
        )
        .await;

    tracing::info!(
        admin_id = %admin.admin_id, platform = %platform,
        devices = device_ids.len(), pushed_connections = hit,
        "强制升级已撤销"
    );
    Ok(ok(serde_json::json!({
        "devices": device_ids.len(),
        "pushedConnections": hit,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TelemetryQuery {
    limit: Option<u32>,
    offset: Option<u32>,
    /// 按 event_type 过滤(分类 chip 选中时传);缺省/空 = 全部。
    event_type: Option<String>,
}

async fn get_telemetry(
    _admin: AdminAuthUser,
    Path(device_id): Path<String>,
    Query(q): Query<TelemetryQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(50).min(200);
    let offset = q.offset.unwrap_or(0);
    let event_type = q.event_type.unwrap_or_default();
    let (records, total) = state
        .run_store_task(
            "admin.clients.get_telemetry",
            move |store| -> Result<_, AppError> {
                if !store.client_device_exists(&device_id)? {
                    return Err(AppError::not_found("设备不存在"));
                }

                Ok(store.get_telemetry_summaries_by_device(&device_id, &event_type, limit, offset)?)
            },
        )
        .await??;

    Ok(ok(
        serde_json::json!({ "records": records, "total": total }),
    ))
}

/// 设备遥测分类总览:全量按 event_type 分组聚合 + 时间范围 + 设备画像。
/// 计数走全量(不受分页影响),给"遥测记录"面板的分类 chip 与每类聚合行。
async fn get_telemetry_summary(
    _admin: AdminAuthUser,
    Path(device_id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let summary = state
        .run_store_task(
            "admin.clients.get_telemetry_summary",
            move |store| -> Result<_, AppError> {
                if !store.client_device_exists(&device_id)? {
                    return Err(AppError::not_found("设备不存在"));
                }
                Ok(store.get_telemetry_device_summary(&device_id)?)
            },
        )
        .await??;

    Ok(ok(summary))
}

/// 单设备详情视图。脱敏:不暴露 last_ip / banned_by(审计字段),其余 client_devices
/// 列 + 在线状态 + 近期 telemetry 摘要给设计图"详情"抽屉。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClientDetail {
    device_id: String,
    platform: String,
    user_id: Option<String>,
    app_version: Option<String>,
    model: Option<String>,
    country: Option<String>,
    first_seen_at: String,
    last_seen_at: String,
    is_banned: bool,
    banned_at: Option<String>,
    ban_reason: Option<String>,
    /// m054:关联风控标记(是否因共享 IP/账号被某次封禁牵连)。
    risk_flag: bool,
    risk_reason: Option<String>,
    risk_flagged_at: Option<String>,
    risk_related_device: Option<String>,
    /// 当前是否有活跃 SSE 连接(在线)。
    online: bool,
    /// 活跃 SSE 连接数(0 = 离线)。
    connection_count: usize,
    /// 近期 telemetry 摘要:总条数 + 最近一条原始记录(None 表示从未上报)。
    telemetry: TelemetrySummaryView,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TelemetrySummaryView {
    total: i64,
    latest: Option<serde_json::Value>,
}

/// GET /api/admin/clients/:id —— 单设备详情(设计图设备列表"详情"按钮)。
/// 按 device_id 直查单设备(get_client_device),telemetry 摘要复用 get_telemetry_summaries_by_device。
async fn get_client_detail(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // 在线状态来自内存 SSE 表(与 list_clients 一致)。
    let connection_count = state.active_sse().get(&id).map(|c| c.len()).unwrap_or(0);

    let device_id = id.clone();
    let (device, telemetry_total, telemetry_latest) = state
        .run_store_task(
            "admin.clients.detail",
            move |store| -> Result<_, AppError> {
                // 按 device_id 直查单设备,不存在即 404。
                let device = store
                    .get_client_device(&device_id)?
                    .ok_or_else(|| AppError::not_found("设备不存在"))?;
                // telemetry 摘要:近 1 条 + 总数(复用 get_telemetry_summaries_by_device)。
                let (records, total) = store.get_telemetry_summaries_by_device(&device_id, "", 1, 0)?;
                let latest = serde_json::to_value(records)
                    .ok()
                    .and_then(|v| v.as_array().and_then(|a| a.first().cloned()));
                Ok((device, total, latest))
            },
        )
        .await??;

    let detail = ClientDetail {
        device_id: device.device_id,
        platform: device.platform,
        user_id: device.user_id,
        app_version: device.app_version,
        model: device.model,
        country: device.country,
        first_seen_at: device.first_seen_at,
        last_seen_at: device.last_seen_at,
        is_banned: device.is_banned,
        banned_at: device.banned_at,
        ban_reason: device.ban_reason,
        risk_flag: device.risk_flag,
        risk_reason: device.risk_reason,
        risk_flagged_at: device.risk_flagged_at,
        risk_related_device: device.risk_related_device,
        online: connection_count > 0,
        connection_count,
        telemetry: TelemetrySummaryView {
            total: telemetry_total as i64,
            latest: telemetry_latest,
        },
    };

    Ok(ok(serde_json::json!(detail)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(device_id: &str) -> ClientDevice {
        ClientDevice {
            device_id: device_id.to_string(),
            platform: "ios".to_string(),
            user_id: Some("user-1".to_string()),
            first_seen_at: "2026-04-25 12:00:00".to_string(),
            last_seen_at: "2026-04-25 12:01:00".to_string(),
            is_banned: false,
            banned_at: None,
            banned_by: None,
            ban_reason: None,
            app_version: None,
            country: None,
            last_ip: None,
            model: None,
            risk_flag: false,
            risk_reason: None,
            risk_flagged_at: None,
            risk_related_device: None,
        }
    }

    #[test]
    fn recent_clients_exclude_live_devices() {
        let devices = vec![client("live-device"), client("recent-only")];
        let live_device_ids = vec!["live-device".to_string()];

        let filtered = exclude_live_devices(devices, &live_device_ids);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].device_id, "recent-only");
    }
}
