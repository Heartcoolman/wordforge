use std::collections::BTreeSet;

use axum::body::Body;
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
                    let mut user = store
                        .get_user_by_id(&user_id)?
                        .ok_or_else(|| AppError::not_found("用户不存在"))?;
                    user.username = username;
                    user.updated_at = Utc::now();
                    store.update_user(&user)?;
                    Ok(user)
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
        let mut user = store
            .get_user_by_id(&user_id)?
            .ok_or_else(|| AppError::not_found("用户不存在"))?;

        if !verify_password(&current_password, &user.password_hash)? {
            return Err(AppError::unauthorized("当前密码不正确"));
        }

        user.password_hash = hash_password(&new_password)?;
        user.updated_at = Utc::now();
        store.update_user(&user)?;
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
                    total_words_learned: agg.word_ids.len() as u64,
                    total_sessions: agg.session_ids.len() as u64,
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

    // 频率检查（在 blocking 线程做，避免 async 跨越 SQLite 连接）
    let retry_after = state
        .run_store_task("users.gdpr_export.check_rate", {
            let user_id = user_id.clone();
            move |store| -> Result<Option<i64>, AppError> {
                let last = store.get_gdpr_export_last_at(&user_id)?;
                if let Some(last_at) = last {
                    let elapsed = (Utc::now() - last_at).num_seconds();
                    if elapsed < GDPR_EXPORT_COOLDOWN_SECS {
                        return Ok(Some(GDPR_EXPORT_COOLDOWN_SECS - elapsed));
                    }
                }
                Ok(None)
            }
        })
        .await??;

    if let Some(secs) = retry_after {
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

    // 记录本次导出（先记录，再读取；冷却从此刻开始计算）
    {
        let uid = user_id.clone();
        state
            .run_store_task("users.gdpr_export.log", move |store| {
                store.upsert_gdpr_export_log(&uid)
            })
            .await??;
    }

    // 逐块收集数据，每块在独立 blocking 任务中完成。
    let mut lines: Vec<String> = Vec::new();

    // profile
    {
        let uid = user_id.clone();
        let profile = state
            .run_store_task("users.gdpr_export.profile", move |store| {
                store.export_profile(&uid)
            })
            .await??;
        if let Some(p) = profile {
            lines.push(json_line("profile", &p));
        }
    }

    // study_config
    {
        let uid = user_id.clone();
        let cfg = state
            .run_store_task("users.gdpr_export.study_config", move |store| {
                store.export_study_config(&uid)
            })
            .await??;
        if let Some(c) = cfg {
            lines.push(json_line("study_config", &c));
        }
    }

    // word_states
    {
        let uid = user_id.clone();
        let states = state
            .run_store_task("users.gdpr_export.word_states", move |store| {
                store.export_word_states(&uid)
            })
            .await??;
        if !states.is_empty() {
            lines.push(json_line("word_states", &states));
        }
    }

    // favorites
    {
        let uid = user_id.clone();
        let favs = state
            .run_store_task("users.gdpr_export.favorites", move |store| {
                store.export_favorites(&uid)
            })
            .await??;
        if !favs.is_empty() {
            lines.push(json_line("favorites", &favs));
        }
    }

    // notes
    {
        let uid = user_id.clone();
        let notes = state
            .run_store_task("users.gdpr_export.notes", move |store| {
                store.export_notes(&uid)
            })
            .await??;
        if !notes.is_empty() {
            lines.push(json_line("notes", &notes));
        }
    }

    // sessions
    {
        let uid = user_id.clone();
        let sessions = state
            .run_store_task("users.gdpr_export.sessions", move |store| {
                store.export_sessions(&uid)
            })
            .await??;
        if !sessions.is_empty() {
            lines.push(json_line("sessions", &sessions));
        }
    }

    // records（分页，每批 EXPORT_RECORDS_PAGE 条，避免长事务）
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
            lines.push(json_line("records", &page));
        }
        if done {
            break;
        }
        offset += EXPORT_RECORDS_PAGE;
    }

    let body_str = lines.join("\n") + "\n";
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-ndjson")
        .header(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment; filename=\"wordforge-export.ndjson\""),
        )
        .body(Body::from(body_str))
        .map_err(|e| AppError::internal(&e.to_string()))
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
