use axum::extract::{Query, State};
use axum::routing::post;
use axum::Router;

use crate::extractors::JsonBody;
use chrono::Utc;
use serde::Deserialize;

use crate::auth::AdminAuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        // m032:GET = 广播历史 + 统计看板；POST = 发送广播
        .route("/", post(broadcast_message).get(list_broadcasts))
        // m027:受众预估命中端点 — 不持久化,纯查询,供 DevicesPage 广播 section 实时显示
        .route("/preview", post(broadcast_preview))
}

/// 历史列表分页参数。`total` 取近 30 天全量条数（见 stats），故此处仅控制窗口。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    #[serde(default)]
    offset: Option<i64>,
    #[serde(default)]
    limit: Option<i64>,
}

/// m032:GET /api/admin/broadcast —— 近 30 天广播历史 + 聚合统计 + 当前在线。
/// 供设计稿 broadcast.html 的 KPI 卡与历史列表（offset/limit 分页，total 见 stats.total）。
async fn list_broadcasts(
    _admin: AdminAuthUser,
    Query(q): Query<ListQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let since = (Utc::now() - chrono::Duration::days(30)).to_rfc3339();
    let online = state.active_sse().len() as i64;

    // 分页钳制：limit 1..=100（默认 50），offset >= 0（默认 0）
    let limit = q.limit.unwrap_or(50).clamp(1, 100);
    let offset = q.offset.unwrap_or(0).max(0);

    let (stats, rows) = state
        .run_store_task(
            "admin.broadcast.list",
            move |store| -> Result<_, AppError> {
                let stats = store.broadcast_stats_30d(&since)?;
                let rows = store.list_broadcasts_30d(&since, limit, offset)?;
                Ok((stats, rows))
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
        // 分页元信息：total 复用 stats.total（近 30 天全量条数），供 "共 N 条" 与 "加载更多"
        "pagination": {
            "total": total,
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
    Ok(ok(serde_json::json!({ "matched": matched, "total": total })))
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
        if let Some(v) = self.audience.as_ref().and_then(|a| a.version_min.as_deref()) {
            let v_clean = v.trim_start_matches('v');
            if semver::Version::parse(v_clean).is_err() {
                return Err(AppError::bad_request(
                    "INVALID_VERSION_MIN",
                    "受众最低版本号需为合法语义化版本(如 1.2.3)",
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

    let title = req.title.clone();
    let message = req.message.clone();
    let mut total_sent = 0usize;

    // m027:audience 受众 filter。None 走老的全员路径,Some 把命中 user_id 一次性拉出。
    let target_user_ids: Option<Vec<String>> = if let Some(aud) = req.audience {
        state
            .run_store_task(
                "admin.broadcast.resolve_audience",
                move |store| -> Result<_, AppError> {
                    Ok(store.list_user_ids_for_audience(
                        &aud.platforms,
                        aud.version_min.as_deref(),
                        aud.last_active_days,
                        &aud.user_ids,
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
            let entries: Vec<(String, String, serde_json::Value)> = chunk
                .iter()
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
                .collect();
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

            let entries: Vec<(String, String, serde_json::Value)> = users
                .iter()
                .map(|user| {
                    let notification_id = format!("{}_{}", broadcast_id, user.id);
                    let value = serde_json::json!({
                        "id": notification_id,
                        "userId": user.id,
                        "type": "broadcast",
                        "title": title,
                        "message": message,
                        "read": false,
                        "createdAt": Utc::now().to_rfc3339(),
                    });
                    (user.id.clone(), notification_id, value)
                })
                .collect();

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

    // m032:写广播历史(供 GET 看板)。失败仅 log,不影响主流程返回。
    {
        let record_id = broadcast_id.clone();
        let record_title = title.clone();
        let record_message = message.clone();
        let record_admin = admin.admin_id.clone();
        let created_at = Utc::now().to_rfc3339();
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
            tracing::warn!(error = %e.message, broadcast_id = %broadcast_id, "写广播历史失败(不影响主流程)");
        }
    }

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
