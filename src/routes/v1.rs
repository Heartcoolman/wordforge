//! B79: V1 compatibility routes
//! Thin wrappers mapping /api/v1/* to existing handlers.
//!
//! ⚠️  **设计警告（P3#6）**：V1 路由刻意**绕过 AMAS 引擎**——
//!     - POST /api/v1/records 仅做 5 秒去重，不更新 user_state/ELO/word_state；
//!     - POST /api/v1/learning/session 仅 create-or-resume，不计算 cross-session hint；
//!     - GET /api/v1/* 与 /api/* 等价。
//!
//!   现行客户端（iOS）一律调用 /api/* 主端点，无 v1 调用。如果未来有新客户端
//!   误用 v1，会**静默退化为非自适应学习**——届时务必显式弃用而非补打 AMAS 调用，
//!   否则等于把"轻量兼容层"重新做成主端点的副本。

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::Router;

use crate::extractors::JsonBody;
use serde::Deserialize;

use crate::auth::AuthUser;
use crate::constants::DEFAULT_PAGE_SIZE_RECORDS;
use crate::response::{ok, paginated, AppError};
use crate::routes::words::WordPublic;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/words", get(list_words))
        .route("/words/:id", get(get_word))
        .route("/records", get(list_records).post(create_record))
        .route("/study-config", get(get_study_config))
        .route("/learning/session", post(create_session))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PaginationQuery {
    page: Option<u64>,
    per_page: Option<u64>,
}

async fn list_words(
    _user: AuthUser,
    Query(q): Query<PaginationQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let page = q.page.unwrap_or(1).clamp(1, u64::MAX);
    let per_page = q
        .per_page
        .unwrap_or(state.config().pagination.default_page_size)
        .clamp(1, state.config().pagination.max_page_size);
    let limit = per_page as usize;
    let offset = ((page - 1) * per_page) as usize;
    let (items, total) = state
        .run_store_task("v1.list_words", move |store| {
            Ok::<_, crate::store::StoreError>((
                store.list_words(limit, offset)?,
                store.count_words()?,
            ))
        })
        .await??;
    let items: Vec<WordPublic> = items.iter().map(WordPublic::from).collect();
    Ok(paginated(items, total, page, per_page))
}

async fn get_word(
    _user: AuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let word = state
        .run_store_task("v1.get_word", move |store| store.get_word(&id))
        .await??
        .ok_or_else(|| AppError::not_found("单词不存在"))?;
    Ok(ok(WordPublic::from(&word)))
}

async fn list_records(
    auth: AuthUser,
    Query(q): Query<PaginationQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let page = q.page.unwrap_or(1).clamp(1, u64::MAX);
    let per_page = q
        .per_page
        .unwrap_or(DEFAULT_PAGE_SIZE_RECORDS)
        .clamp(1, state.config().pagination.max_page_size);
    let limit = per_page as usize;
    let offset = ((page - 1) * per_page) as usize;
    let (records, total) = state
        .run_store_task("v1.list_records", move |store| {
            Ok::<_, crate::store::StoreError>((
                store.get_user_records_with_offset(&auth.user_id, limit, offset)?,
                store.count_user_records(&auth.user_id)? as u64,
            ))
        })
        .await??;
    Ok(paginated(records, total, page, per_page))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct V1RecordRequest {
    word_id: String,
    is_correct: bool,
    response_time_ms: i64,
}

async fn create_record(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<V1RecordRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // 幂等性检查：同一用户对同一单词在 5 秒内的重复提交视为幂等请求
    const DEDUP_WINDOW_MS: i64 = 5_000;
    let now = chrono::Utc::now();
    let record = state
        .run_store_task("v1.create_record", move |store| -> Result<_, AppError> {
            let recent_records = store.get_user_records(&auth.user_id, 50)?;
            for r in &recent_records {
                if r.word_id == req.word_id
                    && r.is_correct == req.is_correct
                    && (now - r.created_at).num_milliseconds().abs() < DEDUP_WINDOW_MS
                {
                    return Ok(r.clone());
                }
            }

            let record = crate::store::operations::records::LearningRecord {
                id: uuid::Uuid::new_v4().to_string(),
                user_id: auth.user_id.clone(),
                word_id: req.word_id.clone(),
                is_correct: req.is_correct,
                response_time_ms: req.response_time_ms,
                session_id: None,
                created_at: now,
                record_type: crate::store::operations::records::RecordType::All,
                self_rating: None,
            };
            store.create_record(&record)?;
            Ok(record)
        })
        .await??;
    Ok(ok(record))
}

async fn get_study_config(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let config = state
        .run_store_task("v1.get_study_config", move |store| {
            store.get_study_config(&auth.user_id)
        })
        .await??;
    Ok(ok(config))
}

async fn create_session(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id;
    let user_id_for_active = user_id.clone();
    let active = state
        .run_store_task("v1.create_session.active", move |store| {
            store.get_active_sessions_for_user(&user_id_for_active)
        })
        .await??;
    if let Some(existing) = active.into_iter().next() {
        return Ok(ok(serde_json::json!({
            "sessionId": existing.id,
            "resumed": true,
        })));
    }

    let config = state
        .run_store_task("v1.create_session.config", {
            let user_id = user_id.clone();
            move |store| store.get_study_config(&user_id)
        })
        .await??;

    let session = crate::store::operations::learning_sessions::LearningSession {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user_id.clone(),
        status: crate::store::operations::learning_sessions::SessionStatus::Active,
        target_mastery_count: config.daily_mastery_target,
        total_questions: 0,
        actual_mastery_count: 0,
        context_shifts: 0,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        summary: None,
        correct_count: 0,
        total_count: 0,
    };
    let session_id = session.id.clone();

    state
        .run_store_task("v1.create_session.persist", move |store| {
            store.create_learning_session(&session)
        })
        .await??;
    Ok(ok(serde_json::json!({
        "sessionId": session_id,
        "resumed": false,
    })))
}
