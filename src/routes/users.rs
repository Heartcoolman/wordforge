use std::collections::BTreeSet;
use std::convert::Infallible;

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use axum::routing::{get, put};
use axum::Router;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::auth::{hash_password, verify_password, AuthUser};
use crate::extractors::JsonBody;
use crate::response::{ok, AppError};
use crate::routes::auth::UserProfile;
use crate::state::AppState;
use crate::store::operations::records::LearningRecord;
use crate::store::operations::users::GdprBeginResult;
use crate::validation::{validate_password, validate_username};

/// 每用户 GDPR 导出冷却时间（秒）。
const GDPR_EXPORT_COOLDOWN_SECS: i64 = 24 * 3600;

/// 每次 records 分页块大小，避免单次长事务。
const EXPORT_RECORDS_PAGE: usize = 500;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/me",
            get(get_profile).put(update_profile).delete(delete_me),
        )
        .route("/me/password", put(change_password))
        .route("/me/stats", get(get_stats))
        .route("/me/export", get(gdpr_export))
}

async fn get_profile(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id.clone();
    let user = state
        .run_store_task("users.get_profile", move |store| {
            store.get_user_by_id(&user_id)
        })
        .await??
        .ok_or_else(|| AppError::not_found("用户不存在"))?;
    Ok(ok(UserProfile::from(&user)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateProfileRequest {
    username: Option<String>,
}

async fn update_profile(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<UpdateProfileRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if let Some(username) = req.username {
        let trimmed = username.trim();
        if let Err(msg) = validate_username(trimmed) {
            return Err(AppError::bad_request("USER_INVALID_USERNAME", msg));
        }
        let user_id = auth.user_id.clone();
        let username = trimmed.to_string();
        let user = state
            .run_store_task(
                "users.update_profile",
                move |store| -> Result<_, AppError> {
                    // 字段级更新：只写 username，避免陈旧整行快照覆盖并发封禁等状态。
                    store.update_user_username(&user_id, &username)?;
                    store
                        .get_user_by_id(&user_id)?
                        .ok_or_else(|| AppError::not_found("用户不存在"))
                },
            )
            .await??;
        return Ok(ok(UserProfile::from(&user)));
    }

    let user_id = auth.user_id.clone();
    let user = state
        .run_store_task(
            "users.get_profile_passthrough",
            move |store| -> Result<_, AppError> {
                store
                    .get_user_by_id(&user_id)?
                    .ok_or_else(|| AppError::not_found("用户不存在"))
            },
        )
        .await??;
    Ok(ok(UserProfile::from(&user)))
}

async fn delete_me(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id.clone();
    state
        .run_store_task("users.delete_me", move |store| store.delete_user(&user_id))
        .await??;
    Ok(ok(serde_json::json!({ "deleted": true })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
}

async fn change_password(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<ChangePasswordRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if let Err(msg) = validate_password(&req.new_password) {
        return Err(AppError::bad_request("AUTH_WEAK_PASSWORD", msg));
    }
    let store = state.store().clone();
    let user_id = auth.user_id.clone();
    let current_password = req.current_password;
    let new_password = req.new_password;
    crate::blocking::run_blocking("users.change_password", move || -> Result<(), AppError> {
        let user = store
            .get_user_by_id(&user_id)?
            .ok_or_else(|| AppError::not_found("用户不存在"))?;

        if !verify_password(&current_password, &user.password_hash)? {
            return Err(AppError::unauthorized("当前密码不正确"));
        }

        // 字段级更新：只写 password_hash，避免陈旧整行快照覆盖并发封禁等状态。
        let new_hash = hash_password(&new_password)?;
        store.update_user_password(&user.id, &new_hash)?;
        let _ = store.delete_user_sessions(&user_id)?;
        Ok(())
    })
    .await??;

    Ok(ok(serde_json::json!({"passwordChanged": true})))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UserStats {
    total_words_learned: u64,
    total_sessions: u64,
    total_records: u64,
    streak_days: u32,
    accuracy_rate: f64,
}

async fn get_stats(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id.clone();
    let max_records_fetch = state.config().limits.max_records_fetch;
    let stats = state
        .run_store_task("users.get_stats", move |store| -> Result<_, AppError> {
            let agg = store.get_user_stats_agg(&user_id)?;
            if agg.total_records > 0 {
                let accuracy_rate = agg.correct_records as f64 / agg.total_records as f64;
                let records = store.get_user_records(&user_id, max_records_fetch)?;
                return Ok(UserStats {
                    total_words_learned: store.count_distinct_words(&user_id)?,
                    total_sessions: store.count_distinct_sessions(&user_id)?,
                    total_records: agg.total_records,
                    streak_days: compute_streak_days(&records),
                    accuracy_rate,
                });
            }

            let records = store.get_user_records(&user_id, max_records_fetch)?;
            let total_records = records.len() as u64;
            let correct = records.iter().filter(|r| r.is_correct).count() as u64;
            let accuracy_rate = if total_records == 0 {
                0.0
            } else {
                correct as f64 / total_records as f64
            };

            Ok(UserStats {
                total_words_learned: records
                    .iter()
                    .map(|r| r.word_id.clone())
                    .collect::<std::collections::HashSet<_>>()
                    .len() as u64,
                total_sessions: records
                    .iter()
                    .filter_map(|r| r.session_id.clone())
                    .collect::<std::collections::HashSet<_>>()
                    .len() as u64,
                total_records,
                streak_days: compute_streak_days(&records),
                accuracy_rate,
            })
        })
        .await??;
    Ok(ok(stats))
}

/// GET /api/users/me/export
///
/// GDPR Article 20 数据导出：流式输出 JSON Lines，每行是一个数据块。
/// 格式：`{"table":"<name>","data":<value>}\n`
///
/// 频率限制：每用户每 24h 1 次；超限返回 429 + Retry-After header。
async fn gdpr_export(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Response<Body>, AppError> {
    let user_id = auth.user_id.clone();

    // 原子「检查冷却 + 记录导出」（单事务，BEGIN IMMEDIATE）：避免两个并发请求都读到旧时间、
    // 都通过冷却门导致的 TOCTOU 双导出。通过则当场记录本次导出时间，冷却从此刻开始计算。
    let begin = state
        .run_store_task("users.gdpr_export.begin", {
            let user_id = user_id.clone();
            move |store| -> Result<GdprBeginResult, AppError> {
                Ok(store.try_begin_gdpr_export(&user_id, GDPR_EXPORT_COOLDOWN_SECS)?)
            }
        })
        .await??;

    let exported_at = match begin {
        GdprBeginResult::RateLimited(secs) => {
            tracing::warn!(user_id = %user_id, retry_after = secs, "GDPR export rate limited");
            let body = serde_json::json!({
                "success": false,
                "code": "GDPR_EXPORT_RATE_LIMITED",
                "message": "每 24 小时只能导出一次数据",
            });
            return Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .header(header::CONTENT_TYPE, "application/json")
                .header("retry-after", secs.to_string())
                .body(Body::from(body.to_string()))
                .map_err(|e| AppError::internal(&e.to_string()));
        }
        GdprBeginResult::Began { exported_at } => exported_at,
    };

    // 导出时间已在上面的原子 begin 中记录（保留原子 begin 防并发双导出）；但若下方流式生产失败，
    // 需回滚这条冷却记录，使瞬态失败不消耗用户 24h 配额（GDPR 数据可获取性）。

    // v1.1 P0：真流式 NDJSON。逐块读取后立即推入 mpsc channel，axum 用 ReceiverStream 包装为
    // Body，HTTP/1.1 自动 Transfer-Encoding: chunked。修复 C4/B4 报告：原 `Vec.join` 一次性
    // body 在大用户（几十 MB records）下内存膨胀，且无法被客户端边读边写盘。
    //
    // 错误处理：导出过程中若某个 store 任务失败，会向 channel 推一条 `{"error":"..."}` 行后
    // 立即关闭流；客户端会读到部分数据 + 一行 error 标记，**而不是**收到 HTTP 5xx —— 因为
    // status code 已经在第一个 chunk flush 时定下，无法回退。该取舍是流式 API 的固有约束。
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, Infallible>>(8);
    let state_for_stream = state.clone();
    let uid_for_stream = user_id.clone();
    tokio::spawn(async move {
        match run_export_stream(state_for_stream.clone(), uid_for_stream.clone(), tx.clone()).await
        {
            Ok(()) => {}
            // 客户端断连(mpsc 接收端被 drop):客户端已主动放弃下载,**不**回滚冷却记录,
            // 否则恶意客户端可反复"开流→读首块后立即断开→回滚配额"绕过 24h 限额,放大 DB/CPU。
            // 客户端已不在,也无需再发 _error 行。
            Err(ExportError::ClientDisconnected) => {
                tracing::debug!(user_id = %uid_for_stream, "GDPR export client disconnected; cooldown kept");
            }
            // 仅服务端内部/存储错误才回滚冷却记录,使真正的瞬态失败不消耗用户配额。
            Err(ExportError::Internal(err)) => {
                // 瞬态失败回滚冷却记录：仅当 exported_at 仍是本次写入的时间戳才删，避免误删并发重试的较新行。
                let rollback = state_for_stream
                    .run_store_task("users.gdpr_export.rollback", {
                        let uid = uid_for_stream.clone();
                        let ts = exported_at.clone();
                        move |store| store.rollback_gdpr_export_log(&uid, &ts)
                    })
                    .await;
                match rollback {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        tracing::warn!(user_id = %uid_for_stream, error = %e, "GDPR export cooldown rollback failed (store)");
                    }
                    Err(e) => {
                        tracing::warn!(user_id = %uid_for_stream, error = ?e, "GDPR export cooldown rollback failed (task)");
                    }
                }
                // AppError 无 Display 实现，用其暴露的 message 字段拼 NDJSON 错误行。
                let line = serde_json::json!({"table": "_error", "data": err.message}).to_string();
                let _ = tx.send(Ok(Bytes::from(line + "\n"))).await;
            }
        }
    });
    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-ndjson")
        .header(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment; filename=\"wordforge-export.ndjson\""),
        )
        .body(Body::from_stream(stream))
        .map_err(|e| AppError::internal(&e.to_string()))
}

/// GDPR 导出流式生产的失败类别:区分客户端断连与服务端内部错误,
/// 前者不应回滚冷却配额(防绕过限额),后者才回滚(真正的瞬态失败)。
enum ExportError {
    /// 客户端断连(mpsc 接收端被 drop):配额已被合理消耗,不回滚。
    ClientDisconnected,
    /// 服务端内部/存储错误:回滚冷却记录,使瞬态失败不消耗配额。
    Internal(AppError),
}

impl From<AppError> for ExportError {
    fn from(e: AppError) -> Self {
        ExportError::Internal(e)
    }
}

impl From<crate::blocking::BlockingTaskError> for ExportError {
    fn from(e: crate::blocking::BlockingTaskError) -> Self {
        ExportError::Internal(AppError::from(e))
    }
}

impl From<crate::store::StoreError> for ExportError {
    fn from(e: crate::store::StoreError) -> Self {
        ExportError::Internal(AppError::from(e))
    }
}

/// v1.1 P0：GDPR 导出真流式 body 生产者。
/// 每块单独在 blocking 线程读完后立即 push 到 channel；channel 容量 8 提供少量背压，
/// 客户端断连时 tx.send 返回 Err → 后续块直接早退（不再浪费 store 任务）。
///
/// 错误返回给调用方按类别处理（见 `gdpr_export` 内的 spawn）。
async fn run_export_stream(
    state: AppState,
    user_id: String,
    tx: tokio::sync::mpsc::Sender<Result<Bytes, Infallible>>,
) -> Result<(), ExportError> {
    // profile
    let profile = state
        .run_store_task("users.gdpr_export.profile", {
            let uid = user_id.clone();
            move |store| store.export_profile(&uid)
        })
        .await??;
    if let Some(p) = profile {
        send_line(&tx, "profile", &p).await?;
    }

    // study_config
    let cfg = state
        .run_store_task("users.gdpr_export.study_config", {
            let uid = user_id.clone();
            move |store| store.export_study_config(&uid)
        })
        .await??;
    if let Some(c) = cfg {
        send_line(&tx, "study_config", &c).await?;
    }

    // word_states
    let states = state
        .run_store_task("users.gdpr_export.word_states", {
            let uid = user_id.clone();
            move |store| store.export_word_states(&uid)
        })
        .await??;
    if !states.is_empty() {
        send_line(&tx, "word_states", &states).await?;
    }

    // favorites
    let favs = state
        .run_store_task("users.gdpr_export.favorites", {
            let uid = user_id.clone();
            move |store| store.export_favorites(&uid)
        })
        .await??;
    if !favs.is_empty() {
        send_line(&tx, "favorites", &favs).await?;
    }

    // notes
    let notes = state
        .run_store_task("users.gdpr_export.notes", {
            let uid = user_id.clone();
            move |store| store.export_notes(&uid)
        })
        .await??;
    if !notes.is_empty() {
        send_line(&tx, "notes", &notes).await?;
    }

    // sessions
    let sessions = state
        .run_store_task("users.gdpr_export.sessions", {
            let uid = user_id.clone();
            move |store| store.export_sessions(&uid)
        })
        .await??;
    if !sessions.is_empty() {
        send_line(&tx, "sessions", &sessions).await?;
    }

    // records 分页 push（与旧实现相同分页粒度，每批读完立即 flush 给客户端）。
    let mut offset = 0usize;
    loop {
        let uid = user_id.clone();
        let page = state
            .run_store_task("users.gdpr_export.records", move |store| {
                store.export_records(&uid, EXPORT_RECORDS_PAGE, offset)
            })
            .await??;
        let done = page.len() < EXPORT_RECORDS_PAGE;
        if !page.is_empty() {
            send_line(&tx, "records", &page).await?;
        }
        if done {
            break;
        }
        offset += EXPORT_RECORDS_PAGE;
    }

    Ok(())
}

/// 流式 helper：序列化一行并 push 到 channel；客户端断连时返回 `ClientDisconnected` 让上游早退。
/// 断连是客户端主动行为,**不**作为可回滚的内部错误处理,以免被用来绕过导出冷却配额。
async fn send_line<T: Serialize>(
    tx: &tokio::sync::mpsc::Sender<Result<Bytes, Infallible>>,
    table: &str,
    data: &T,
) -> Result<(), ExportError> {
    let line = json_line(table, data) + "\n";
    let chunk: Result<Bytes, Infallible> = Ok(Bytes::from(line));
    tx.send(chunk)
        .await
        .map_err(|_| ExportError::ClientDisconnected)
}

/// 序列化一个数据块为 JSON Lines 行。
fn json_line<T: Serialize>(table: &str, data: &T) -> String {
    let data_val = serde_json::to_value(data).unwrap_or(serde_json::Value::Null);
    serde_json::json!({"table": table, "data": data_val}).to_string()
}

pub fn compute_streak_days(records: &[LearningRecord]) -> u32 {
    if records.is_empty() {
        return 0;
    }

    let dates: BTreeSet<chrono::NaiveDate> =
        records.iter().map(|r| r.created_at.date_naive()).collect();

    compute_streak_from_dates(&dates)
}

pub fn compute_streak_from_dates(dates: &BTreeSet<chrono::NaiveDate>) -> u32 {
    if dates.is_empty() {
        return 0;
    }

    let today = Utc::now().date_naive();
    let mut streak = 0u32;
    let mut current = today;

    if !dates.contains(&current) {
        match current.pred_opt() {
            Some(yesterday) if dates.contains(&yesterday) => current = yesterday,
            _ => return 0,
        }
    }

    while dates.contains(&current) {
        streak += 1;
        current = match current.pred_opt() {
            Some(d) => d,
            None => break,
        };
    }

    streak
}
