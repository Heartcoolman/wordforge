use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;

use crate::extractors::JsonBody;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::amas::types::{MasteryLevel, ProcessResult, RawEvent};
use crate::auth::AuthUser;
use crate::constants::{DEFAULT_HALF_LIFE_HOURS, DEFAULT_PAGE_SIZE_RECORDS, MAX_PAGE_SIZE};
use crate::response::{created, ok, paginated, AppError};
use crate::state::AppState;
use crate::store::operations::learning_sessions::LearningSession;
use crate::store::operations::records::LearningRecord;
use crate::store::operations::word_states::{WordLearningState, WordState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_records).post(create_record))
        .route("/statistics", get(get_statistics))
        .route("/statistics/enhanced", get(get_enhanced_statistics))
        .route("/batch", post(batch_create_records))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListRecordsQuery {
    page: Option<u64>,
    per_page: Option<u64>,
}

impl ListRecordsQuery {
    fn page(&self) -> u64 {
        self.page.unwrap_or(1).clamp(1, u64::MAX)
    }
    fn per_page(&self) -> u64 {
        self.per_page.unwrap_or(DEFAULT_PAGE_SIZE_RECORDS).clamp(1, MAX_PAGE_SIZE)
    }
}

async fn list_records(
    auth: AuthUser,
    Query(q): Query<ListRecordsQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let page = q.page();
    let per_page = q.per_page();
    let limit = per_page as usize;
    let offset = ((page - 1) * per_page) as usize;
    let records = state
        .store()
        .get_user_records_with_offset(&auth.user_id, limit, offset)?;
    let total = state.store().count_user_records(&auth.user_id)? as u64;
    Ok(paginated(records, total, page, per_page))
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CreateRecordRequest {
    client_record_id: Option<String>,
    word_id: String,
    is_correct: bool,
    response_time_ms: i64,
    session_id: Option<String>,
    is_quit: Option<bool>,
    dwell_time_ms: Option<i64>,
    pause_count: Option<i32>,
    switch_count: Option<i32>,
    retry_count: Option<i32>,
    focus_loss_duration_ms: Option<i64>,
    interaction_density: Option<f64>,
    paused_time_ms: Option<i64>,
    hint_used: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateRecordResponse {
    record: LearningRecord,
    amas_result: Option<ProcessResult>,
    duplicate: bool,
}

#[derive(Debug, Clone)]
struct EngineStateSnapshot {
    user_state: Option<serde_json::Value>,
    ige: Option<serde_json::Value>,
    swd: Option<serde_json::Value>,
    trust: Option<serde_json::Value>,
    mastery: Option<serde_json::Value>,
    mastery_key: String,
    user_elo: crate::amas::elo::EloRating,
    word_elo: crate::amas::elo::EloRating,
}

#[derive(Debug, Clone)]
struct UserStateSnapshot {
    user_state: Option<serde_json::Value>,
    ige: Option<serde_json::Value>,
    swd: Option<serde_json::Value>,
    trust: Option<serde_json::Value>,
    user_elo: crate::amas::elo::EloRating,
}

fn capture_user_state_snapshot(
    store: &crate::store::Store,
    user_id: &str,
) -> Result<UserStateSnapshot, AppError> {
    Ok(UserStateSnapshot {
        user_state: store.get_engine_user_state(user_id)?,
        ige: store.get_engine_algo_state(user_id, "ige")?,
        swd: store.get_engine_algo_state(user_id, "swd")?,
        trust: store.get_engine_algo_state(user_id, "trust")?,
        user_elo: store.get_user_elo(user_id)?,
    })
}

fn capture_engine_state_snapshot(
    store: &crate::store::Store,
    user_id: &str,
    word_id: &str,
) -> Result<EngineStateSnapshot, AppError> {
    let mastery_key = format!("mastery:{word_id}");

    Ok(EngineStateSnapshot {
        user_state: store.get_engine_user_state(user_id)?,
        ige: store.get_engine_algo_state(user_id, "ige")?,
        swd: store.get_engine_algo_state(user_id, "swd")?,
        trust: store.get_engine_algo_state(user_id, "trust")?,
        mastery: store.get_engine_algo_state(user_id, &mastery_key)?,
        mastery_key,
        user_elo: store.get_user_elo(user_id)?,
        word_elo: store.get_word_elo(word_id)?,
    })
}

fn restore_engine_state_snapshot(
    store: &crate::store::Store,
    user_id: &str,
    word_id: &str,
    snapshot: &EngineStateSnapshot,
) {
    match &snapshot.user_state {
        Some(previous) => {
            if let Err(error) = store.set_engine_user_state(user_id, previous) {
                tracing::warn!(user_id, error = %error, "Failed to rollback AMAS user state");
            }
        }
        None => {
            if let Err(error) = store.delete_engine_user_state(user_id) {
                tracing::warn!(user_id, error = %error, "Failed to delete AMAS user state during rollback");
            }
        }
    }

    restore_engine_algo_state(store, user_id, "ige", &snapshot.ige);
    restore_engine_algo_state(store, user_id, "swd", &snapshot.swd);
    restore_engine_algo_state(store, user_id, "trust", &snapshot.trust);
    restore_engine_algo_state(store, user_id, &snapshot.mastery_key, &snapshot.mastery);

    // 回滚 ELO 评分
    if let Err(error) = store.set_user_elo(user_id, &snapshot.user_elo) {
        tracing::warn!(user_id, error = %error, "Failed to rollback user ELO");
    }
    if let Err(error) = store.set_word_elo(word_id, &snapshot.word_elo) {
        tracing::warn!(word_id, error = %error, "Failed to rollback word ELO");
    }
}

fn restore_user_state_snapshot(
    store: &crate::store::Store,
    user_id: &str,
    snapshot: &UserStateSnapshot,
) {
    match &snapshot.user_state {
        Some(previous) => {
            if let Err(error) = store.set_engine_user_state(user_id, previous) {
                tracing::warn!(user_id, error = %error, "Failed to rollback AMAS user state");
            }
        }
        None => {
            if let Err(error) = store.delete_engine_user_state(user_id) {
                tracing::warn!(user_id, error = %error, "Failed to delete AMAS user state during rollback");
            }
        }
    }

    restore_engine_algo_state(store, user_id, "ige", &snapshot.ige);
    restore_engine_algo_state(store, user_id, "swd", &snapshot.swd);
    restore_engine_algo_state(store, user_id, "trust", &snapshot.trust);

    if let Err(error) = store.set_user_elo(user_id, &snapshot.user_elo) {
        tracing::warn!(user_id, error = %error, "Failed to rollback user ELO");
    }
}

fn restore_engine_algo_state(
    store: &crate::store::Store,
    user_id: &str,
    algo_id: &str,
    previous: &Option<serde_json::Value>,
) {
    let result = match previous {
        Some(value) => store.set_engine_algo_state(user_id, algo_id, value),
        None => store.delete_engine_algo_state(user_id, algo_id),
    };

    if let Err(error) = result {
        tracing::warn!(user_id, algo_id, error = %error, "Failed to rollback AMAS algorithm state");
    }
}

async fn process_single_record(
    user_id: &str,
    req: &CreateRecordRequest,
    state: &AppState,
) -> Result<CreateRecordResponse, AppError> {
    let record_id = req
        .client_record_id
        .as_ref()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    if let Some(existing) = state.store().get_user_record_by_id(user_id, &record_id)? {
        return Ok(CreateRecordResponse {
            record: existing,
            amas_result: None,
            duplicate: true,
        });
    }

    let record = LearningRecord {
        id: record_id,
        user_id: user_id.to_string(),
        word_id: req.word_id.clone(),
        is_correct: req.is_correct,
        response_time_ms: req.response_time_ms,
        session_id: req.session_id.clone(),
        created_at: Utc::now(),
    };

    let engine_snapshot = capture_engine_state_snapshot(state.store(), user_id, &req.word_id)?;

    let amas_result = state
        .amas()
        .process_event(
            user_id,
            RawEvent {
                word_id: req.word_id.clone(),
                is_correct: req.is_correct,
                response_time_ms: req.response_time_ms,
                session_id: req.session_id.clone(),
                is_quit: req.is_quit.unwrap_or(false),
                dwell_time_ms: req.dwell_time_ms,
                pause_count: req.pause_count,
                switch_count: req.switch_count,
                retry_count: req.retry_count,
                focus_loss_duration_ms: req.focus_loss_duration_ms,
                interaction_density: req.interaction_density,
                paused_time_ms: req.paused_time_ms,
                hint_used: req.hint_used.unwrap_or(false),
                confused_with: None,
            },
        )
        .await?;

    // 更新 ELO 评分
    {
        let amas_config = state.amas().get_config().await;
        let mut user_elo = state.store().get_user_elo(user_id)?;
        let mut word_elo = state.store().get_word_elo(&req.word_id)?;
        crate::amas::elo::update_elo(
            &mut user_elo,
            &mut word_elo,
            req.is_correct,
            &amas_config.elo,
        );
        state.store().set_user_elo(user_id, &user_elo)?;
        state.store().set_word_elo(&req.word_id, &word_elo)?;
    }

    let mut next_word_state: Option<WordLearningState> = None;
    if let Some(ref wm) = amas_result.word_mastery {
        let new_state = match wm.mastery_level {
            MasteryLevel::New => WordState::New,
            MasteryLevel::Learning => WordState::Learning,
            MasteryLevel::Reviewing => WordState::Reviewing,
            MasteryLevel::Mastered => WordState::Mastered,
            MasteryLevel::Forgotten => WordState::Forgotten,
        };

        let mut wls = state
            .store()
            .get_word_learning_state(user_id, &req.word_id)?
            .unwrap_or_else(|| WordLearningState {
                user_id: user_id.to_string(),
                word_id: req.word_id.clone(),
                state: WordState::New,
                mastery_level: 0.0,
                next_review_date: None,
                half_life: DEFAULT_HALF_LIFE_HOURS,
                correct_streak: 0,
                total_attempts: 0,
                updated_at: Utc::now(),
            });

        wls.state = new_state;
        wls.mastery_level = wm.memory_strength;
        wls.total_attempts += 1;
        if req.is_correct {
            wls.correct_streak += 1;
        } else {
            wls.correct_streak = 0;
        }
        if wm.next_review_interval_secs > 0 {
            wls.next_review_date =
                Some(Utc::now() + chrono::Duration::seconds(wm.next_review_interval_secs));
        }
        wls.updated_at = Utc::now();
        next_word_state = Some(wls);
    }

    let mut next_session: Option<LearningSession> = None;
    if let Some(ref sid) = req.session_id {
        if let Some(mut session) = state.store().get_learning_session(sid)? {
            session.total_questions += 1;
            session.total_count += 1;
            if req.is_correct {
                session.correct_count += 1;
            }
            if let Some(ref wm) = amas_result.word_mastery {
                if wm.mastery_level == MasteryLevel::Mastered {
                    session.actual_mastery_count += 1;
                }
            }
            session.updated_at = Utc::now();
            next_session = Some(session);
        }
    }

    state
        .store()
        .create_record_with_updates(&record, next_word_state.as_ref(), next_session.as_ref())
        .map_err(|error| {
            restore_engine_state_snapshot(state.store(), user_id, &req.word_id, &engine_snapshot);
            AppError::internal(&error.to_string())
        })?;

    Ok(CreateRecordResponse {
        record,
        amas_result: Some(amas_result),
        duplicate: false,
    })
}

async fn create_record(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<CreateRecordRequest>,
) -> Result<axum::response::Response, AppError> {
    let result = process_single_record(&auth.user_id, &req, &state).await?;
    if result.duplicate {
        Ok(ok(result).into_response())
    } else {
        Ok(created(result).into_response())
    }
}

// B33: Batch submit records
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchCreateRecordsRequest {
    records: Vec<CreateRecordRequest>,
}

async fn batch_create_records(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<BatchCreateRecordsRequest>,
) -> Result<axum::response::Response, AppError> {
    if req.records.len() > state.config().limits.max_batch_size {
        return Err(AppError::bad_request(
            "BATCH_TOO_LARGE",
            &format!(
                "批量创建记录数量上限为{}",
                state.config().limits.max_batch_size
            ),
        ));
    }

    // S6: 在批量首条前捕获一次用户级快照
    let user_snapshot = capture_user_state_snapshot(state.store(), &auth.user_id)?;

    let mut results: Vec<CreateRecordResponse> = Vec::new();
    let mut errors = Vec::new();
    for (index, item) in req.records.iter().enumerate() {
        match process_batch_record(&auth.user_id, item, &state).await {
            Ok(result) => results.push(result),
            Err(error) => {
                errors.push(serde_json::json!({
                    "index": index,
                    "code": error.code,
                    "message": error.message,
                }));
            }
        }
    }

    // 如果全部失败，回滚到初始用户状态
    if !results.is_empty() && results.iter().all(|r| r.duplicate) && !errors.is_empty() {
        restore_user_state_snapshot(state.store(), &auth.user_id, &user_snapshot);
    }

    let payload = serde_json::json!({
        "count": results.len(),
        "failed": errors.len(),
        "partial": !errors.is_empty(),
        "items": results,
        "errors": errors,
    });

    if payload["partial"].as_bool() == Some(true) {
        Ok(ok(payload).into_response())
    } else {
        Ok(created(payload).into_response())
    }
}

/// S5: 批量场景下的单条记录处理，只捕获 word 级快照（mastery + word_elo）
async fn process_batch_record(
    user_id: &str,
    req: &CreateRecordRequest,
    state: &AppState,
) -> Result<CreateRecordResponse, AppError> {
    let record_id = req
        .client_record_id
        .as_ref()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    if let Some(existing) = state.store().get_user_record_by_id(user_id, &record_id)? {
        return Ok(CreateRecordResponse {
            record: existing,
            amas_result: None,
            duplicate: true,
        });
    }

    let record = LearningRecord {
        id: record_id,
        user_id: user_id.to_string(),
        word_id: req.word_id.clone(),
        is_correct: req.is_correct,
        response_time_ms: req.response_time_ms,
        session_id: req.session_id.clone(),
        created_at: Utc::now(),
    };

    // S6: 只捕获 word 级状态
    let mastery_key = format!("mastery:{}", &req.word_id);
    let prev_mastery = state.store().get_engine_algo_state(user_id, &mastery_key)?;
    let prev_word_elo = state.store().get_word_elo(&req.word_id)?;

    let amas_result = state
        .amas()
        .process_event(
            user_id,
            RawEvent {
                word_id: req.word_id.clone(),
                is_correct: req.is_correct,
                response_time_ms: req.response_time_ms,
                session_id: req.session_id.clone(),
                is_quit: req.is_quit.unwrap_or(false),
                dwell_time_ms: req.dwell_time_ms,
                pause_count: req.pause_count,
                switch_count: req.switch_count,
                retry_count: req.retry_count,
                focus_loss_duration_ms: req.focus_loss_duration_ms,
                interaction_density: req.interaction_density,
                paused_time_ms: req.paused_time_ms,
                hint_used: req.hint_used.unwrap_or(false),
                confused_with: None,
            },
        )
        .await?;

    {
        let amas_config = state.amas().get_config().await;
        let mut user_elo = state.store().get_user_elo(user_id)?;
        let mut word_elo = state.store().get_word_elo(&req.word_id)?;
        crate::amas::elo::update_elo(
            &mut user_elo,
            &mut word_elo,
            req.is_correct,
            &amas_config.elo,
        );
        state.store().set_user_elo(user_id, &user_elo)?;
        state.store().set_word_elo(&req.word_id, &word_elo)?;
    }

    let mut next_word_state: Option<WordLearningState> = None;
    if let Some(ref wm) = amas_result.word_mastery {
        let new_state = match wm.mastery_level {
            MasteryLevel::New => WordState::New,
            MasteryLevel::Learning => WordState::Learning,
            MasteryLevel::Reviewing => WordState::Reviewing,
            MasteryLevel::Mastered => WordState::Mastered,
            MasteryLevel::Forgotten => WordState::Forgotten,
        };

        let mut wls = state
            .store()
            .get_word_learning_state(user_id, &req.word_id)?
            .unwrap_or_else(|| WordLearningState {
                user_id: user_id.to_string(),
                word_id: req.word_id.clone(),
                state: WordState::New,
                mastery_level: 0.0,
                next_review_date: None,
                half_life: DEFAULT_HALF_LIFE_HOURS,
                correct_streak: 0,
                total_attempts: 0,
                updated_at: Utc::now(),
            });

        wls.state = new_state;
        wls.mastery_level = wm.memory_strength;
        wls.total_attempts += 1;
        if req.is_correct {
            wls.correct_streak += 1;
        } else {
            wls.correct_streak = 0;
        }
        if wm.next_review_interval_secs > 0 {
            wls.next_review_date =
                Some(Utc::now() + chrono::Duration::seconds(wm.next_review_interval_secs));
        }
        wls.updated_at = Utc::now();
        next_word_state = Some(wls);
    }

    let mut next_session: Option<LearningSession> = None;
    if let Some(ref sid) = req.session_id {
        if let Some(mut session) = state.store().get_learning_session(sid)? {
            session.total_questions += 1;
            session.total_count += 1;
            if req.is_correct {
                session.correct_count += 1;
            }
            if let Some(ref wm) = amas_result.word_mastery {
                if wm.mastery_level == MasteryLevel::Mastered {
                    session.actual_mastery_count += 1;
                }
            }
            session.updated_at = Utc::now();
            next_session = Some(session);
        }
    }

    state
        .store()
        .create_record_with_updates(&record, next_word_state.as_ref(), next_session.as_ref())
        .map_err(|error| {
            // 回滚 word 级状态
            restore_engine_algo_state(state.store(), user_id, &mastery_key, &prev_mastery);
            if let Err(e) = state.store().set_word_elo(&req.word_id, &prev_word_elo) {
                tracing::warn!(error = %e, "Failed to rollback word ELO in batch");
            }
            AppError::internal(&error.to_string())
        })?;

    Ok(CreateRecordResponse {
        record,
        amas_result: Some(amas_result),
        duplicate: false,
    })
}

// B32: Statistics
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordStatistics {
    total: usize,
    correct: usize,
    accuracy: f64,
}

async fn get_statistics(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let (total, correct) = state.store().count_user_records_stats(&auth.user_id)?;
    let accuracy = if total > 0 {
        correct as f64 / total as f64
    } else {
        0.0
    };

    Ok(ok(RecordStatistics {
        total,
        correct,
        accuracy,
    }))
}

async fn get_enhanced_statistics(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // 限制单次查询量，后续应改为增量聚合以支持更大数据量
    let records = state.store().get_user_records(&auth.user_id, state.config().limits.max_stats_records)?;
    let total = records.len();
    let correct = records.iter().filter(|r| r.is_correct).count();
    let accuracy = if total > 0 {
        correct as f64 / total as f64
    } else {
        0.0
    };

    // By-day breakdown
    let mut by_day: std::collections::BTreeMap<String, (usize, usize)> =
        std::collections::BTreeMap::new();
    for r in &records {
        let day = r.created_at.format("%Y-%m-%d").to_string();
        let entry = by_day.entry(day).or_insert((0, 0));
        entry.0 += 1;
        if r.is_correct {
            entry.1 += 1;
        }
    }

    let daily: Vec<serde_json::Value> = by_day
        .iter()
        .map(|(day, (total, correct))| {
            serde_json::json!({
                "date": day,
                "total": total,
                "correct": correct,
                "accuracy": if *total > 0 { *correct as f64 / *total as f64 } else { 0.0 },
            })
        })
        .collect();

    // Current streak (consecutive days)
    let dates: std::collections::BTreeSet<chrono::NaiveDate> = by_day
        .keys()
        .filter_map(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .collect();
    let streak = super::users::compute_streak_from_dates(&dates);

    Ok(ok(serde_json::json!({
        "total": total,
        "correct": correct,
        "accuracy": accuracy,
        "streak": streak,
        "daily": daily,
    })))
}
