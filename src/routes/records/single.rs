use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;

use crate::extractors::JsonBody;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::amas::types::{MasteryLevel, ProcessResult, RawEvent};
use crate::auth::AuthUser;
use crate::constants::{DEFAULT_HALF_LIFE_HOURS, DEFAULT_PAGE_SIZE_RECORDS, MAX_PAGE_SIZE};
use crate::response::{created, ok, paginated, AppError};
use crate::services::event_bus::DomainEvent;
use crate::state::AppState;
use crate::store::operations::learning_sessions::LearningSession;
use crate::store::operations::records::{LearningRecord, RecordType};
use crate::store::operations::word_states::{WordLearningState, WordState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_records).post(create_record))
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
pub(crate) struct CreateRecordRequest {
    pub(crate) client_record_id: Option<String>,
    pub(crate) word_id: String,
    pub(crate) is_correct: bool,
    pub(crate) response_time_ms: i64,
    pub(crate) session_id: Option<String>,
    pub(crate) is_quit: Option<bool>,
    pub(crate) dwell_time_ms: Option<i64>,
    pub(crate) pause_count: Option<i32>,
    pub(crate) switch_count: Option<i32>,
    pub(crate) retry_count: Option<i32>,
    pub(crate) focus_loss_duration_ms: Option<i64>,
    pub(crate) interaction_density: Option<f64>,
    pub(crate) paused_time_ms: Option<i64>,
    pub(crate) hint_used: Option<bool>,
    #[serde(default)]
    pub(crate) confused_with: Option<String>,
    #[serde(default)]
    pub(crate) record_type: Option<RecordType>,
    /// SRS 自评粒度（0=Again / 1=Hard / 2=Good / 3=Easy）；
    /// 客户端选填，落库供 AMAS half-life 模型分级回退使用。
    #[serde(default)]
    pub(crate) self_rating: Option<u8>,
    /// 内部派生路径专用：覆盖 created_at（不参与 JSON 反序列化）。
    #[serde(skip)]
    pub(crate) created_at_override: Option<DateTime<Utc>>,
}

impl CreateRecordRequest {
    pub(crate) fn record_type_or_default(&self) -> RecordType {
        self.record_type.unwrap_or_default()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreateRecordResponse {
    pub(crate) record: LearningRecord,
    pub(crate) amas_result: Option<ProcessResult>,
    pub(crate) duplicate: bool,
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
pub(crate) struct UserStateSnapshot {
    pub(crate) user_state: Option<serde_json::Value>,
    pub(crate) ige: Option<serde_json::Value>,
    pub(crate) swd: Option<serde_json::Value>,
    pub(crate) trust: Option<serde_json::Value>,
    pub(crate) user_elo: crate::amas::elo::EloRating,
}

pub(crate) fn capture_user_state_snapshot(
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

pub(crate) fn restore_user_state_snapshot(
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

pub(crate) fn restore_engine_algo_state(
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
        created_at: req.created_at_override.unwrap_or_else(Utc::now),
        record_type: req.record_type_or_default(),
        self_rating: req.self_rating,
    };
    let word_id = req.word_id.clone();
    let record_for_store = record.clone();
    let req_for_store = req.clone();

    let engine_snapshot = state
        .run_store_task("records.single.snapshot", {
            let user_id = user_id_owned.clone();
            let word_id = word_id.clone();
            move |store| capture_engine_state_snapshot(&store, &user_id, &word_id)
        })
        .await??;

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
                store.set_user_elo(&user_id_owned, &user_elo)?;
                store.set_word_elo(&word_id, &word_elo)?;

                let mut next_word_state: Option<WordLearningState> = None;
                if let Some(ref wm) = amas_result_for_store.word_mastery {
                    let new_state = match wm.mastery_level {
                        MasteryLevel::New => WordState::New,
                        MasteryLevel::Learning => WordState::Learning,
                        MasteryLevel::Reviewing => WordState::Reviewing,
                        MasteryLevel::Mastered => WordState::Mastered,
                        MasteryLevel::Forgotten => WordState::Forgotten,
                    };

                    let mut wls = store
                        .get_word_learning_state(&user_id_owned, &word_id)?
                        .unwrap_or_else(|| WordLearningState {
                            user_id: user_id_owned.clone(),
                            word_id: word_id.clone(),
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
                    if req_for_store.is_correct {
                        wls.correct_streak += 1;
                    } else {
                        wls.correct_streak = 0;
                    }
                    if wm.next_review_interval_secs > 0 {
                        wls.next_review_date = Some(
                            Utc::now() + chrono::Duration::seconds(wm.next_review_interval_secs),
                        );
                    }
                    wls.updated_at = Utc::now();
                    next_word_state = Some(wls);
                }

                let mut next_session: Option<LearningSession> = None;
                if let Some(ref sid) = req_for_store.session_id {
                    if let Some(mut session) = store.get_learning_session(sid)? {
                        session.total_questions += 1;
                        session.total_count += 1;
                        if req_for_store.is_correct {
                            session.correct_count += 1;
                        }
                        if let Some(ref wm) = amas_result_for_store.word_mastery {
                            if wm.mastery_level == MasteryLevel::Mastered {
                                session.actual_mastery_count += 1;
                            }
                        }
                        session.updated_at = Utc::now();
                        next_session = Some(session);
                    }
                }

                store
                    .create_record_with_updates(
                        &record_for_store,
                        next_word_state.as_ref(),
                        next_session.as_ref(),
                    )
                    .map_err(|error| {
                        restore_engine_state_snapshot(
                            &store,
                            &user_id_owned,
                            &word_id,
                            &engine_snapshot,
                        );
                        AppError::internal(&error.to_string())
                    })?;
                Ok(())
            },
        )
        .await??;

    // v1.1-P1 S2：写库成功后旁路 emit RecordCreated；
    // AMAS 已在前面同步通路执行过，这里只是事件总线基础设施落地，
    // 为未来 outbox 持久化 + 真异步消费铺路。fire-and-forget 不阻塞 HTTP 响应。
    state.event_bus().emit(DomainEvent::RecordCreated {
        user_id: record.user_id.clone(),
        word_id: record.word_id.clone(),
        record_id: record.id.clone(),
        is_correct: record.is_correct,
        response_time_ms: record.response_time_ms,
        session_id: record.session_id.clone(),
        record_type: record.record_type,
        created_at: record.created_at,
    });

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
