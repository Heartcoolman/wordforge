use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::Router;

use crate::extractors::JsonBody;
use chrono::Utc;
use serde::Deserialize;

use crate::auth::AdminAuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::operations::broadcasts::BroadcastFilter;
use crate::store::operations::scheduled_broadcasts::NewScheduledBroadcast;

pub fn router() -> Router<AppState> {
    Router::new()
        // m032:GET = 广播历史 + 统计看板；POST = 发送广播
        .route("/", post(broadcast_message).get(list_broadcasts))
        // m027:受众预估命中端点 — 不持久化,纯查询,供 DevicesPage 广播 section 实时显示
        .route("/preview", post(broadcast_preview))
        // m042/D2:推送编辑器「保存草稿」全局单份(存/取/删)
        .route(
            "/draft",
            get(get_push_draft)
                .post(save_push_draft)
                .delete(delete_push_draft),
        )
}

/// 历史列表分页参数。E2:新增 filter（all/week/failed）下推后端跨页生效。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    #[serde(default)]
    offset: Option<i64>,
    #[serde(default)]
    limit: Option<i64>,
    /// all（默认）/ week（近 7 天）/ failed（sent_count=0）。
    #[serde(default)]
    filter: Option<String>,
}

/// m032:GET /api/admin/broadcast —— 近 30 天广播历史 + 聚合统计 + 当前在线。
/// 供设计稿 broadcast.html 的 KPI 卡与历史列表（offset/limit 分页）。
/// E2:filter（all/week/failed）下推 WHERE 跨页生效，pagination.total 用过滤后计数。
async fn list_broadcasts(
    _admin: AdminAuthUser,
    Query(q): Query<ListQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let since = (Utc::now() - chrono::Duration::days(30)).to_rfc3339();
    // E2:week filter 起点（近 7 天）。仅 filter=week 时生效，其余 filter 忽略。
    let week_cutoff = (Utc::now() - chrono::Duration::days(7)).to_rfc3339();
    let online = state.active_sse().len() as i64;

    // 分页钳制：limit 1..=100（默认 50），offset >= 0（默认 0）
    let limit = q.limit.unwrap_or(50).clamp(1, 100);
    let offset = q.offset.unwrap_or(0).max(0);
    let filter = BroadcastFilter::from_param(q.filter.as_deref());

    // stats（KPI 卡）始终近 30 天全量、不随 filter 变；list 与 filtered_total 按 filter。
    let (stats, rows, filtered_total) = state
        .run_store_task(
            "admin.broadcast.list",
            move |store| -> Result<_, AppError> {
                let stats = store.broadcast_stats_30d(&since)?;
                let rows =
                    store.list_broadcasts_30d(&since, filter, &week_cutoff, limit, offset)?;
                let filtered_total = store.count_broadcasts_30d(&since, filter, &week_cutoff)?;
                Ok((stats, rows, filtered_total))
            },
        )
        .await??;

    let (total, total_sent, avg_read_rate) = stats;
    let broadcasts: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            let read_rate = if r.sent_count > 0 {
                r.read_count as f64 / r.sent_count as f64
            } else {
                0.0
            };
            serde_json::json!({
                "id": r.id,
                "title": r.title,
                "message": r.message,
                "author": r.admin_id,
                "sentCount": r.sent_count,
                "readCount": r.read_count,
                "readRate": read_rate,
                "createdAt": r.created_at,
            })
        })
        .collect();

    Ok(ok(serde_json::json!({
        "stats": {
            "total": total,
            "totalSent": total_sent,
            "avgReadRate": avg_read_rate,
            "online": online,
        },
        "broadcasts": broadcasts,
        // E2:分页 total 用「按 filter 过滤后的计数」（非 stats.total 全量），否则页脚分页数错
        "pagination": {
            "total": filtered_total,
            "offset": offset,
            "limit": limit,
        },
    })))
}

/// m027:广播受众预估命中。POST 接 `BroadcastAudience`,返 `{matched, total}`。
/// audience 为 None 时 matched = total(全员)。
async fn broadcast_preview(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<PreviewRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let (matched, total) = state
        .run_store_task(
            "admin.broadcast.preview",
            move |store| -> Result<_, AppError> {
                let total = store.count_users()? as i64;
                let matched = if let Some(aud) = req.audience {
                    match store.list_user_ids_for_audience(
                        &aud.platforms,
                        aud.version_min.as_deref(),
                        aud.last_active_days,
                        &aud.user_ids,
                    )? {
                        Some(ids) => ids.len() as i64,
                        None => total,
                    }
                } else {
                    total
                };
                Ok((matched, total))
            },
        )
        .await??;
    Ok(ok(
        serde_json::json!({ "matched": matched, "total": total }),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewRequest {
    #[serde(default)]
    audience: Option<BroadcastAudience>,
}

// B63: System-wide broadcast
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BroadcastRequest {
    title: String,
    message: String,
    /// m027:可选受众条件。任意字段为空数组/None 都视为"该维度不过滤";
    /// 整个 audience 为 None 时走老的全员广播路径。
    #[serde(default)]
    audience: Option<BroadcastAudience>,
    /// m042/D2:投递时机。None/过去时间→立即 fan-out;未来 RFC3339 时间→入
    /// scheduled_broadcasts 队列,由 scheduled_broadcast worker 到期下发。
    #[serde(default)]
    scheduled_at: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BroadcastAudience {
    #[serde(default)]
    platforms: Vec<String>,
    #[serde(default)]
    version_min: Option<String>,
    #[serde(default)]
    last_active_days: Option<i64>,
    #[serde(default)]
    user_ids: Vec<String>,
}

impl BroadcastRequest {
    fn validate(&self) -> Result<(), AppError> {
        if self.title.is_empty() || self.title.len() > 200 {
            return Err(AppError::bad_request(
                "INVALID_TITLE",
                "标题长度需在1到200个字符之间",
            ));
        }
        if self.message.is_empty() || self.message.len() > 10000 {
            return Err(AppError::bad_request(
                "INVALID_MESSAGE",
                "消息内容长度需在1到10000个字符之间",
            ));
        }
        // 受众指定 version_min 时必须是合法 semver,否则二次过滤会静默匹配零
        if let Some(v) = self
            .audience
            .as_ref()
            .and_then(|a| a.version_min.as_deref())
        {
            let v_clean = v.trim_start_matches('v');
            if semver::Version::parse(v_clean).is_err() {
                return Err(AppError::bad_request(
                    "INVALID_VERSION_MIN",
                    "受众最低版本号需为合法语义化版本(如 1.2.3)",
                ));
            }
        }
        // m042/D2:投递时机必须是合法 RFC3339 时间(空字符串视为不填=立即)
        if let Some(s) = self.scheduled_at.as_deref().filter(|s| !s.is_empty()) {
            if chrono::DateTime::parse_from_rfc3339(s).is_err() {
                return Err(AppError::bad_request(
                    "INVALID_SCHEDULED_AT",
                    "投递时间需为合法 RFC3339 时间戳",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BroadcastUpdateRequest {
    version: Option<String>,
    message: Option<String>,
}

pub(crate) async fn broadcast_update(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<BroadcastUpdateRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    state.broadcast_update(req.version.as_deref(), req.message.as_deref());
    Ok(ok(serde_json::json!({ "broadcasted": true })))
}

async fn broadcast_message(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<BroadcastRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    req.validate()?;

    // 使用幂等 key 防止重复广播
    let broadcast_id = uuid::Uuid::new_v4().to_string();

    // m042/D2:投递时机分流。scheduled_at 为未来时间→入队,不立即下发。
    if let Some(when) = req.scheduled_at.as_deref().filter(|s| !s.is_empty()) {
        // validate() 已确保 RFC3339 合法
        let when_dt = chrono::DateTime::parse_from_rfc3339(when)
            .expect("validated rfc3339")
            .with_timezone(&Utc);
        if when_dt > Utc::now() {
            let aud = req.audience.unwrap_or_default();
            let id = broadcast_id.clone();
            let title = req.title.clone();
            let message = req.message.clone();
            let admin_id = admin.admin_id.clone();
            let scheduled_at = when_dt.to_rfc3339();
            let created_at = Utc::now().to_rfc3339();
            state
                .run_store_task("admin.broadcast.schedule", move |store| {
                    store
                        .insert_scheduled_broadcast(&NewScheduledBroadcast {
                            id: &id,
                            title: &title,
                            message: &message,
                            admin_id: &admin_id,
                            platforms: &aud.platforms,
                            version_min: aud.version_min.as_deref(),
                            last_active_days: aud.last_active_days,
                            user_ids: &aud.user_ids,
                            scheduled_at: &scheduled_at,
                            created_at: &created_at,
                        })
                        .map_err(|e| AppError::internal(&e.to_string()))
                })
                .await??;
            tracing::info!(
                admin_id = %admin.admin_id,
                action = "broadcast_scheduled",
                broadcast_id = %broadcast_id,
                scheduled_at = %when_dt.to_rfc3339(),
                "管理员排程定时广播"
            );
            return Ok(ok(serde_json::json!({
                "scheduled": true,
                "broadcastId": broadcast_id,
                "scheduledAt": when_dt.to_rfc3339(),
            })));
        }
        // 过去时间→视为立即下发(落到下方即时路径)
    }

    let audience = req
        .audience
        .map(|a| (a.platforms, a.version_min, a.last_active_days, a.user_ids));
    let total_sent =
        fan_out_broadcast(&state, &broadcast_id, &req.title, &req.message, audience).await?;

    record_broadcast_history(
        &state,
        &broadcast_id,
        &req.title,
        &req.message,
        &admin.admin_id,
        total_sent,
    )
    .await;

    tracing::info!(
        admin_id = %admin.admin_id,
        action = "broadcast",
        broadcast_id = %broadcast_id,
        total_sent = total_sent,
        "管理员发送系统广播"
    );

    Ok(ok(
        serde_json::json!({"sent": total_sent, "broadcastId": broadcast_id}),
    ))
}

/// 广播受众过滤四元组：(platforms, version_min, last_active_days, user_ids)。None 走全员。
type AudienceFilter = (Vec<String>, Option<String>, Option<i64>, Vec<String>);

/// 即时广播 fan-out（即时路径与 scheduled_broadcast worker 共用）。
///
/// `audience` 为 None 走全员路径;Some 时按受众过滤,过滤后零命中返回 `EMPTY_AUDIENCE`
/// 400(绝不静默成功)。返回投递条数。**不写广播历史**——由调用方决定何时记录。
pub(crate) async fn fan_out_broadcast(
    state: &AppState,
    broadcast_id: &str,
    title: &str,
    message: &str,
    audience: Option<AudienceFilter>,
) -> Result<usize, AppError> {
    let mut total_sent = 0usize;

    // m027:audience 受众 filter。None 走老的全员路径,Some 把命中 user_id 一次性拉出。
    let target_user_ids: Option<Vec<String>> =
        if let Some((platforms, version_min, days, uids)) = audience {
            state
                .run_store_task(
                    "admin.broadcast.resolve_audience",
                    move |store| -> Result<_, AppError> {
                        Ok(store.list_user_ids_for_audience(
                            &platforms,
                            version_min.as_deref(),
                            days,
                            &uids,
                        )?)
                    },
                )
                .await??
        } else {
            None
        };

    if let Some(user_ids) = target_user_ids {
        // 受众过滤已应用但无人命中:绝不静默成功,返回 400 让管理员知道未发送
        if user_ids.is_empty() {
            return Err(AppError::bad_request(
                "EMPTY_AUDIENCE",
                "受众过滤条件未匹配到任何用户,未发送广播,请检查受众条件",
            ));
        }
        // audience 路径:user_id list 分块写通知
        for chunk in user_ids.chunks(100) {
            let entries = build_notification_entries(broadcast_id, title, message, chunk.iter());
            total_sent += entries.len();
            state
                .run_store_task("admin.broadcast.persist", move |store| {
                    store
                        .batch_create_notifications(&entries)
                        .map_err(|e| AppError::internal(&e.to_string()))
                })
                .await??;
        }
        tracing::info!(
            audience = "filtered",
            matched = total_sent,
            "受众过滤广播完成"
        );
    } else {
        // 全员路径(老行为):分批加载 users → batch_create_notifications
        let batch_size = 100;
        let mut offset = 0;
        loop {
            let users = state
                .run_store_task("admin.broadcast.load_users", move |store| {
                    store.list_users(batch_size, offset)
                })
                .await??;
            if users.is_empty() {
                break;
            }
            let ids: Vec<String> = users.iter().map(|u| u.id.clone()).collect();
            let entries = build_notification_entries(broadcast_id, title, message, ids.iter());
            total_sent += entries.len();
            state
                .run_store_task("admin.broadcast.persist", move |store| {
                    store
                        .batch_create_notifications(&entries)
                        .map_err(|e| AppError::internal(&e.to_string()))
                })
                .await??;
            offset += users.len();
            tracing::info!("广播进度: 已发送 {} 条通知", total_sent);
        }
    }

    Ok(total_sent)
}

/// 为一批 user_id 构造 batch_create_notifications 入参(notification id = `{bid}_{uid}`)。
fn build_notification_entries<'a, I>(
    broadcast_id: &str,
    title: &str,
    message: &str,
    user_ids: I,
) -> Vec<(String, String, serde_json::Value)>
where
    I: Iterator<Item = &'a String>,
{
    user_ids
        .map(|uid| {
            let notification_id = format!("{}_{}", broadcast_id, uid);
            let value = serde_json::json!({
                "id": notification_id,
                "userId": uid,
                "type": "broadcast",
                "title": title,
                "message": message,
                "read": false,
                "createdAt": Utc::now().to_rfc3339(),
            });
            (uid.clone(), notification_id, value)
        })
        .collect()
}

/// m032:写广播历史(供 GET 看板)。失败仅 log,不影响主流程。即时/定时路径共用。
pub(crate) async fn record_broadcast_history(
    state: &AppState,
    broadcast_id: &str,
    title: &str,
    message: &str,
    admin_id: &str,
    total_sent: usize,
) {
    let record_id = broadcast_id.to_string();
    let record_title = title.to_string();
    let record_message = message.to_string();
    let record_admin = admin_id.to_string();
    let created_at = Utc::now().to_rfc3339();
    let bid = broadcast_id.to_string();
    if let Err(e) = state
        .run_store_task("admin.broadcast.record_history", move |store| {
            store.record_broadcast(
                &record_id,
                &record_title,
                &record_message,
                &record_admin,
                total_sent,
                &created_at,
            )
        })
        .await
        .map_err(AppError::from)
        .and_then(|r| r.map_err(AppError::from))
    {
        tracing::warn!(error = %e.message, broadcast_id = %bid, "写广播历史失败(不影响主流程)");
    }
}

/// m042/D2:推送草稿存取请求体（title/message 必填,受众维度可选）。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavePushDraftRequest {
    #[serde(default)]
    title: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    platforms: Vec<String>,
    #[serde(default)]
    version_min: Option<String>,
    #[serde(default)]
    last_active_days: Option<i64>,
}

/// 把 PushDraft 序列化为前端 composer 表单形态（camelCase）。
fn push_draft_json(
    d: &crate::store::operations::scheduled_broadcasts::PushDraft,
) -> serde_json::Value {
    serde_json::json!({
        "title": d.title,
        "message": d.message,
        "platforms": d.platforms,
        "versionMin": d.version_min,
        "lastActiveDays": d.last_active_days,
        "authorId": d.author_id,
        "updatedAt": d.updated_at,
    })
}

/// POST /draft —— 保存（覆盖）全局推送草稿。
async fn save_push_draft(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<SavePushDraftRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let author_id = admin.admin_id.clone();
    let draft = state
        .run_store_task("admin.broadcast.draft.save", move |store| {
            store
                .save_push_draft(
                    &req.title,
                    &req.message,
                    &req.platforms,
                    req.version_min.as_deref(),
                    req.last_active_days,
                    Some(author_id.as_str()),
                )
                .map_err(|e| AppError::internal(&e.to_string()))
        })
        .await??;
    Ok(ok(serde_json::json!({ "draft": push_draft_json(&draft) })))
}

/// GET /draft —— 取全局推送草稿（无则 draft=null）。
async fn get_push_draft(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let draft = state
        .run_store_task("admin.broadcast.draft.get", move |store| {
            store
                .get_push_draft()
                .map_err(|e| AppError::internal(&e.to_string()))
        })
        .await??;
    Ok(ok(serde_json::json!({
        "draft": draft.as_ref().map(push_draft_json),
    })))
}

/// DELETE /draft —— 删除全局推送草稿。
async fn delete_push_draft(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let deleted = state
        .run_store_task("admin.broadcast.draft.delete", move |store| {
            store
                .delete_push_draft()
                .map_err(|e| AppError::internal(&e.to_string()))
        })
        .await??;
    Ok(ok(serde_json::json!({ "deleted": deleted })))
}
