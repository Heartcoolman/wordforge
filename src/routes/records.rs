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
use crate::store::operations::records::{LearningRecord, RecordType};
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
        self.per_page
            .unwrap_or(DEFAULT_PAGE_SIZE_RECORDS)
            .clamp(1, MAX_PAGE_SIZE)
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
    let (records, total) = state
        .run_store_task("records.list", move |store| {
            Ok::<_, crate::store::StoreError>((
                store.get_user_records_with_offset(&auth.user_id, limit, offset)?,
                store.count_user_records(&auth.user_id)? as u64,
            ))
        })
        .await??;
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
    #[serde(default)]
    confused_with: Option<String>,
    #[serde(default)]
    record_type: Option<RecordType>,
}

impl CreateRecordRequest {
    fn record_type_or_default(&self) -> RecordType {
        self.record_type.unwrap_or_default()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateRecordResponse {
    record: LearningRecord,
    amas_result: Option<ProcessResult>,
    duplicate: bool,
}

async fn process_single_record(
    user_id: &str,
    req: &CreateRecordRequest,
    state: &AppState,
) -> Result<CreateRecordResponse, AppError> {
    let user_id_owned = user_id.to_string();
    let record_id = req
        .client_record_id
        .as_ref()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    if let Some(existing) = state
        .run_store_task("records.single.check_duplicate", {
            let user_id = user_id_owned.clone();
            let record_id = record_id.clone();
            move |store| store.get_user_record_by_id(&user_id, &record_id)
        })
        .await??
    {
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
        record_type: req.record_type_or_default(),
    };
    let word_id = req.word_id.clone();
    let record_for_store = record.clone();
    let req_for_store = req.clone();

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
                confused_with: req.confused_with.clone(),
            },
        )
        .await?;
    let amas_config = state.amas().get_config();
    let amas_result_for_store = amas_result.clone();

    state
        .run_store_task(
            "records.single.persist",
            move |store| -> Result<_, AppError> {
                let mut user_elo = store.get_user_elo(&user_id_owned)?;
                let mut word_elo = store.get_word_elo(&word_id)?;
                crate::amas::elo::update_elo(
                    &mut user_elo,
                    &mut word_elo,
                    req_for_store.is_correct,
                    &amas_config.elo,
                );

                let next_word_state =
                    build_next_word_state(&store, &user_id_owned, &word_id, &req_for_store, &amas_result_for_store)?;
                let next_session =
                    build_next_session(&store, &req_for_store, &amas_result_for_store)?;

                store
                    .create_record_with_updates_full(
                        &record_for_store,
                        next_word_state.as_ref(),
                        next_session.as_ref(),
                        Some((&user_id_owned, &user_elo)),
                        Some((&word_id, &word_elo)),
                    )
                    .map_err(|error| AppError::internal(&error.to_string()))?;
                Ok(())
            },
        )
        .await??;

    Ok(CreateRecordResponse {
        record,
        amas_result: Some(amas_result),
        duplicate: false,
    })
}

/// 基于 amas 结果构造下一版 WordLearningState；事务前调用，闭包内传值。
fn build_next_word_state(
    store: &crate::store::Store,
    user_id: &str,
    word_id: &str,
    req: &CreateRecordRequest,
    amas_result: &ProcessResult,
) -> Result<Option<WordLearningState>, AppError> {
    let Some(wm) = amas_result.word_mastery.as_ref() else {
        return Ok(None);
    };
    let new_state = match wm.mastery_level {
        MasteryLevel::New => WordState::New,
        MasteryLevel::Learning => WordState::Learning,
        MasteryLevel::Reviewing => WordState::Reviewing,
        MasteryLevel::Mastered => WordState::Mastered,
        MasteryLevel::Forgotten => WordState::Forgotten,
    };
    let mut wls = store
        .get_word_learning_state(user_id, word_id)?
        .unwrap_or_else(|| WordLearningState {
            user_id: user_id.to_string(),
            word_id: word_id.to_string(),
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
    Ok(Some(wls))
}

fn build_next_session(
    store: &crate::store::Store,
    req: &CreateRecordRequest,
    amas_result: &ProcessResult,
) -> Result<Option<LearningSession>, AppError> {
    let Some(sid) = req.session_id.as_deref() else {
        return Ok(None);
    };
    let Some(mut session) = store.get_learning_session(sid)? else {
        return Ok(None);
    };
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
    Ok(Some(session))
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

    let user_id = auth.user_id;

    let mut results: Vec<CreateRecordResponse> = Vec::new();
    let mut errors = Vec::new();
    for (index, item) in req.records.iter().enumerate() {
        match process_batch_record(&user_id, item, &state).await {
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

/// 批量场景下的单条处理：每条都进入独立的单 SQLite 事务，失败由原生 ROLLBACK 撤销。
async fn process_batch_record(
    user_id: &str,
    req: &CreateRecordRequest,
    state: &AppState,
) -> Result<CreateRecordResponse, AppError> {
    let user_id_owned = user_id.to_string();
    let record_id = req
        .client_record_id
        .as_ref()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    if let Some(existing) = state
        .run_store_task("records.batch.check_duplicate", {
            let user_id = user_id_owned.clone();
            let record_id = record_id.clone();
            move |store| store.get_user_record_by_id(&user_id, &record_id)
        })
        .await??
    {
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
        record_type: req.record_type_or_default(),
    };
    let word_id = req.word_id.clone();
    let record_for_store = record.clone();
    let req_for_store = req.clone();

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
                confused_with: req.confused_with.clone(),
            },
        )
        .await?;
    let amas_config = state.amas().get_config();
    let amas_result_for_store = amas_result.clone();

    state
        .run_store_task(
            "records.batch.persist",
            move |store| -> Result<_, AppError> {
                let mut user_elo = store.get_user_elo(&user_id_owned)?;
                let mut word_elo = store.get_word_elo(&word_id)?;
                crate::amas::elo::update_elo(
                    &mut user_elo,
                    &mut word_elo,
                    req_for_store.is_correct,
                    &amas_config.elo,
                );

                let next_word_state =
                    build_next_word_state(&store, &user_id_owned, &word_id, &req_for_store, &amas_result_for_store)?;
                let next_session =
                    build_next_session(&store, &req_for_store, &amas_result_for_store)?;

                store
                    .create_record_with_updates_full(
                        &record_for_store,
                        next_word_state.as_ref(),
                        next_session.as_ref(),
                        Some((&user_id_owned, &user_elo)),
                        Some((&word_id, &word_elo)),
                    )
                    .map_err(|error| AppError::internal(&error.to_string()))?;
                Ok(())
            },
        )
        .await??;

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
    total_duration_secs: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatsCategoryQuery {
    category: Option<String>,
}

fn parse_category(raw: Option<&str>) -> Result<Option<RecordType>, AppError> {
    match raw.unwrap_or("all") {
        "all" => Ok(None),
        "learning" => Ok(Some(RecordType::Learning)),
        "review" => Ok(Some(RecordType::Review)),
        other => Err(AppError::bad_request(
            "INVALID_CATEGORY",
            &format!("category 必须是 all、learning 或 review，收到 {other}"),
        )),
    }
}

async fn get_statistics(
    auth: AuthUser,
    Query(q): Query<StatsCategoryQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let category = parse_category(q.category.as_deref())?;
    let (total, correct, total_duration_secs) = state
        .run_store_task("records.statistics", move |store| -> Result<_, AppError> {
            let (total, correct) =
                store.count_user_records_stats_filtered(&auth.user_id, category)?;
            let total_duration_secs = store.total_session_duration_secs(&auth.user_id)?;
            Ok((total, correct, total_duration_secs))
        })
        .await??;
    let accuracy = if total > 0 {
        correct as f64 / total as f64
    } else {
        0.0
    };

    Ok(ok(RecordStatistics {
        total,
        correct,
        accuracy,
        total_duration_secs,
    }))
}

async fn get_enhanced_statistics(
    auth: AuthUser,
    Query(q): Query<StatsCategoryQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let category = parse_category(q.category.as_deref())?;
    // 限制单次查询量，后续应改为增量聚合以支持更大数据量
    let max_stats_records = state.config().limits.max_stats_records;
    let user_id = auth.user_id.clone();
    let (records, first_times, daily_durations, total_duration_secs) = state
        .run_store_task(
            "records.enhanced_statistics",
            move |store| -> Result<_, AppError> {
                let records =
                    store.get_user_records_filtered(&user_id, max_stats_records, category)?;
                let word_ids: Vec<String> = records
                    .iter()
                    .map(|r| r.word_id.clone())
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .collect();
                let first_times = store.first_record_times_for_words(&user_id, &word_ids)?;
                // 时长走无 cap 的 DB 聚合，避免被 max_stats_records 截断
                let daily_durations = store.daily_session_durations(&user_id)?;
                let total_duration_secs = store.total_session_duration_secs(&user_id)?;
                Ok((records, first_times, daily_durations, total_duration_secs))
            },
        )
        .await??;

    let total = records.len();
    let correct = records.iter().filter(|r| r.is_correct).count();
    let accuracy = if total > 0 {
        correct as f64 / total as f64
    } else {
        0.0
    };

    #[derive(Default)]
    struct DayBucket {
        total: usize,
        correct: usize,
        new_words: std::collections::HashSet<String>,
        duration_secs: u64,
    }

    // daily 仅基于"有记录"的日期，session-only 日期不进 daily 也不计入 streak
    let mut by_day: std::collections::BTreeMap<String, DayBucket> =
        std::collections::BTreeMap::new();
    for r in &records {
        let day = r.created_at.format("%Y-%m-%d").to_string();
        let entry = by_day.entry(day.clone()).or_default();
        entry.total += 1;
        if r.is_correct {
            entry.correct += 1;
        }
        let first_at = first_times
            .get(&r.word_id)
            .copied()
            .unwrap_or(r.created_at);
        if first_at.format("%Y-%m-%d").to_string() == day {
            entry.new_words.insert(r.word_id.clone());
        }
    }
    // 把当日 session 时长贴回有记录的日期
    for (day, bucket) in by_day.iter_mut() {
        if let Some(secs) = daily_durations.get(day) {
            bucket.duration_secs = *secs;
        }
    }

    let daily: Vec<serde_json::Value> = by_day
        .iter()
        .map(|(day, b)| {
            serde_json::json!({
                "date": day,
                "total": b.total,
                "correct": b.correct,
                "accuracy": if b.total > 0 { b.correct as f64 / b.total as f64 } else { 0.0 },
                "newWords": b.new_words.len() as u64,
                "durationSecs": b.duration_secs,
            })
        })
        .collect();

    // 仅基于 record 日期计算 streak，避免 session-only 日期虚增
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
        "totalDurationSecs": total_duration_secs,
        "daily": daily,
    })))
}
