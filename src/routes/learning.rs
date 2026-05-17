use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::Router;

use crate::extractors::JsonBody;
use chrono::{DateTime, Duration, LocalResult, NaiveDate, TimeZone, Utc};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::amas::word_selector::{self, SessionSelectionContext};
use crate::auth::AuthUser;
use crate::response::{ok, paginated, AppError};
use crate::routes::records::{
    capture_user_state_snapshot, process_batch_record, restore_user_state_snapshot,
    CreateRecordRequest,
};
use crate::routes::words::WordPublic;
use crate::state::AppState;
use crate::store::operations::learning_sessions::{LearningSession, SessionStatus, SessionSummary};
use crate::store::operations::records::RecordType;
use crate::store::operations::word_states::WordState;
use std::collections::HashSet;

const SYNC_PROGRESS_EVENTS_LIMIT: usize = 100;
const COMPLETE_SESSION_EVENTS_LIMIT: usize = 200;
const EVENT_RESPONSE_TIME_MAX_MS: i64 = 300_000;
const EVENT_CLIENT_TS_BACKFILL_LIMIT_MIN: i64 = 30;
const EVENT_CLIENT_TS_FUTURE_TOLERANCE_MIN: i64 = 1;
const EVENT_CLIENT_EVENT_ID_MAX_LEN: usize = 128;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/session", post(create_or_resume_session))
        .route("/sessions", get(list_sessions))
        .route("/study-words", get(get_study_words))
        .route("/next-words", post(next_words))
        .route("/adjust-words", post(adjust_words))
        .route("/sync-progress", post(sync_progress))
        .route("/complete-session", post(complete_session))
        .route("/pick-next-word", post(pick_next_word))
        .route("/generate-options", post(generate_options))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListSessionsQuery {
    page: Option<u64>,
    per_page: Option<u64>,
    start_date: Option<String>,
    end_date: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LearningSessionHistoryItem {
    session_id: String,
    status: SessionStatus,
    started_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    duration_secs: u64,
    record_count: u64,
    correct_count: u64,
    accuracy: Option<f64>,
    mastered_word_count: u64,
    error_prone_word_count: u64,
}

fn shanghai_midnight(date: NaiveDate) -> Result<DateTime<Utc>, AppError> {
    let tz: chrono_tz::Tz = "Asia/Shanghai"
        .parse()
        .map_err(|_| AppError::internal("invalid default timezone"))?;
    let naive = date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| AppError::internal("invalid date"))?;
    let local = match tz.from_local_datetime(&naive) {
        LocalResult::Single(dt) => dt,
        LocalResult::Ambiguous(first, _) => first,
        LocalResult::None => {
            return Err(AppError::internal(
                "failed to resolve Asia/Shanghai local midnight",
            ))
        }
    };
    Ok(local.with_timezone(&Utc))
}

fn parse_query_date(value: &str, field: &str) -> Result<NaiveDate, AppError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| {
        AppError::bad_request(
            "LEARNING_INVALID_DATE",
            &format!("{field} 必须是 yyyy-MM-dd 格式"),
        )
    })
}

fn history_duration_secs(session: &LearningSession) -> u64 {
    if let Some(summary) = &session.summary {
        return summary.duration_secs.max(0) as u64;
    }
    if session.status == SessionStatus::Completed || session.status == SessionStatus::Abandoned {
        return (session.updated_at - session.created_at)
            .num_seconds()
            .max(0) as u64;
    }
    0
}

fn history_item(session: LearningSession) -> LearningSessionHistoryItem {
    let duration_secs = history_duration_secs(&session);
    let record_count = if session.total_count > 0 {
        session.total_count as u64
    } else {
        session.total_questions as u64
    };
    let correct_count = session.correct_count as u64;
    let accuracy = session
        .summary
        .as_ref()
        .map(|summary| summary.accuracy)
        .or_else(|| {
            if record_count > 0 {
                Some(correct_count as f64 / record_count as f64)
            } else {
                None
            }
        });
    let mastered_word_count = session
        .summary
        .as_ref()
        .map(|summary| summary.mastered_word_ids.len() as u64)
        .unwrap_or(session.actual_mastery_count as u64);
    let error_prone_word_count = session
        .summary
        .as_ref()
        .map(|summary| summary.error_prone_word_ids.len() as u64)
        .unwrap_or(0);
    let completed_at = if session.status == SessionStatus::Completed {
        Some(session.updated_at)
    } else {
        None
    };
    let status = session.status.clone();

    LearningSessionHistoryItem {
        session_id: session.id,
        status,
        started_at: session.created_at,
        completed_at,
        duration_secs,
        record_count,
        correct_count,
        accuracy,
        mastered_word_count,
        error_prone_word_count,
    }
}

async fn list_sessions(
    auth: AuthUser,
    Query(q): Query<ListSessionsQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q
        .per_page
        .unwrap_or(state.config().pagination.default_page_size)
        .clamp(1, state.config().pagination.max_page_size);
    let start_at = match q.start_date.as_deref() {
        Some(raw) => Some(shanghai_midnight(parse_query_date(raw, "startDate")?)?),
        None => None,
    };
    let end_before = match q.end_date.as_deref() {
        Some(raw) => {
            let date = parse_query_date(raw, "endDate")?;
            Some(shanghai_midnight(date + Duration::days(1))?)
        }
        None => None,
    };
    if let (Some(start), Some(end)) = (start_at, end_before) {
        if start >= end {
            return Err(AppError::bad_request(
                "LEARNING_INVALID_DATE_RANGE",
                "startDate 必须早于或等于 endDate",
            ));
        }
    }

    let limit = per_page as usize;
    let offset = ((page - 1) * per_page) as usize;
    let user_id = auth.user_id.clone();
    let (sessions, total) = state
        .run_store_task("learning.sessions", move |store| {
            Ok::<_, crate::store::StoreError>((
                store.list_learning_sessions_for_user(
                    &user_id, limit, offset, start_at, end_before,
                )?,
                store.count_learning_sessions_for_user(&user_id, start_at, end_before)?,
            ))
        })
        .await??;

    Ok(paginated(
        sessions.into_iter().map(history_item).collect::<Vec<_>>(),
        total,
        page,
        per_page,
    ))
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct CreateSessionRequest {
    target_mastery_count: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionResponse {
    session_id: String,
    status: SessionStatus,
    resumed: bool,
    target_mastery_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    cross_session_hint: Option<CrossSessionHint>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CrossSessionHint {
    prev_accuracy: f64,
    prev_mastered_count: usize,
    gap_minutes: i64,
    suggested_difficulty: f64,
    error_prone_word_ids: Vec<String>,
    recently_mastered_word_ids: Vec<String>,
}

async fn create_or_resume_session(
    auth: AuthUser,
    State(state): State<AppState>,
    body: Option<JsonBody<CreateSessionRequest>>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let req = body.map(|JsonBody(r)| r).unwrap_or_default();
    let user_id = auth.user_id;

    // Check for existing active session
    let active = state
        .run_store_task("learning.create_or_resume.active", {
            let user_id = user_id.clone();
            move |store| store.get_active_sessions_for_user(&user_id)
        })
        .await??;

    if let Some(existing) = active.into_iter().next() {
        return Ok(ok(SessionResponse {
            session_id: existing.id,
            status: SessionStatus::Active,
            resumed: true,
            target_mastery_count: existing.target_mastery_count,
            cross_session_hint: None,
        }));
    }

    let (config, recent_sessions) = state
        .run_store_task("learning.create_or_resume.load", {
            let user_id = user_id.clone();
            move |store| {
                Ok::<_, crate::store::StoreError>((
                    store.get_study_config(&user_id)?,
                    store.get_recent_completed_sessions(&user_id, 7200)?,
                ))
            }
        })
        .await??;

    let target = req
        .target_mastery_count
        .unwrap_or(config.daily_mastery_target);

    // 查询最近完成的会话（2小时内），构建 CrossSessionHint
    let cross_session_hint = if let Some(prev) = recent_sessions.first() {
        let gap_minutes = (Utc::now() - prev.updated_at).num_minutes();
        let (prev_accuracy, error_prone_word_ids, recently_mastered_word_ids) =
            if let Some(ref summary) = prev.summary {
                (
                    summary.accuracy,
                    summary.error_prone_word_ids.clone(),
                    summary.mastered_word_ids.clone(),
                )
            } else {
                let acc = if prev.total_questions > 0 {
                    prev.actual_mastery_count as f64 / prev.total_questions as f64
                } else {
                    0.0
                };
                (acc, vec![], vec![])
            };

        let amas_config = state.amas().get_config();
        let ls = &amas_config.learning_strategy;
        let suggested_difficulty = if prev_accuracy >= ls.cross_session_high_accuracy {
            ls.cross_session_high_difficulty
        } else if prev_accuracy >= ls.cross_session_medium_accuracy {
            ls.cross_session_medium_difficulty
        } else {
            ls.cross_session_low_difficulty
        };

        Some(CrossSessionHint {
            prev_accuracy,
            prev_mastered_count: prev.actual_mastery_count as usize,
            gap_minutes,
            suggested_difficulty,
            error_prone_word_ids,
            recently_mastered_word_ids,
        })
    } else {
        None
    };

    let session = LearningSession {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user_id.clone(),
        status: SessionStatus::Active,
        target_mastery_count: target,
        total_questions: 0,
        actual_mastery_count: 0,
        context_shifts: 0,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        summary: None,
        correct_count: 0,
        total_count: 0,
    };
    let session_id = session.id.clone();

    state
        .run_store_task("learning.create_or_resume.persist", move |store| {
            store.create_learning_session(&session)
        })
        .await??;

    Ok(ok(SessionResponse {
        session_id,
        status: SessionStatus::Active,
        resumed: false,
        target_mastery_count: target,
        cross_session_hint,
    }))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MasteryStudyWordsResponse {
    words: Vec<WordPublic>,
    strategy: StudyStrategy,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StudyStrategy {
    difficulty_range: (f64, f64),
    new_ratio: f64,
    batch_size: u32,
}

async fn get_study_words(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id;
    let config = state
        .run_store_task("learning.get_study_words.config", {
            let user_id = user_id.clone();
            move |store| store.get_study_config(&user_id)
        })
        .await??;

    // Get AMAS strategy if available
    let amas_state = state.amas().get_user_state_async(&user_id).await?;
    let strategy_params = state.amas().compute_strategy_from_state(&amas_state);

    let batch_size = strategy_params.batch_size as usize;
    let new_ratio = strategy_params.new_ratio;
    let difficulty = strategy_params.difficulty;
    let pool_size = state.config().limits.candidate_word_pool_size;
    let amas_config = state.amas().get_config();
    let selected_wordbook_ids = config.selected_wordbook_ids.clone();
    let words = state
        .run_store_task(
            "learning.get_study_words.select",
            move |store| -> Result<_, AppError> {
                let mut candidate_word_ids = Vec::new();
                for book_id in &selected_wordbook_ids {
                    let wids = store.list_wordbook_words(book_id, pool_size, 0)?;
                    candidate_word_ids.extend(wids);
                }

                if candidate_word_ids.is_empty() {
                    let words = store.list_words(pool_size, 0)?;
                    for w in &words {
                        candidate_word_ids.push(w.id.clone());
                    }
                }

                candidate_word_ids.sort();
                candidate_word_ids.dedup();

                let scored = word_selector::select_words(
                    &store,
                    &user_id,
                    &candidate_word_ids,
                    &strategy_params,
                    batch_size,
                    None,
                    &word_selector::SelectionConfigs {
                        word_selector: &amas_config.word_selector,
                        elo: &amas_config.elo,
                        memory_model: &amas_config.memory_model,
                    },
                )?;

                let scored_word_ids: Vec<String> =
                    scored.iter().map(|sw| sw.word_id.clone()).collect();
                let words_by_id = store.get_words_by_ids(&scored_word_ids)?;
                Ok(scored
                    .iter()
                    .filter_map(|sw| words_by_id.get(&sw.word_id).map(WordPublic::from))
                    .collect::<Vec<_>>())
            },
        )
        .await??;

    Ok(ok(MasteryStudyWordsResponse {
        words,
        strategy: StudyStrategy {
            difficulty_range: ((difficulty - 0.2).max(0.0), (difficulty + 0.2).min(1.0)),
            new_ratio,
            batch_size: batch_size as u32,
        },
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NextWordsRequest {
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    batch_index: Option<u32>,
    exclude_word_ids: Vec<String>,
    mastered_word_ids: Option<Vec<String>>,
    session_performance: Option<SessionPerformanceData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionPerformanceData {
    recent_accuracy: f64,
    mastered_count: u32,
    target_mastery_count: u32,
    error_prone_word_ids: Vec<String>,
}

async fn next_words(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<NextWordsRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if req.exclude_word_ids.len() > state.config().limits.max_exclude_word_ids {
        return Err(AppError::bad_request(
            "LEARNING_TOO_MANY_EXCLUDES",
            &format!(
                "排除单词数量不能超过{}",
                state.config().limits.max_exclude_word_ids
            ),
        ));
    }

    let user_id = auth.user_id;
    // 显式 batchIndex：启用幂等重放路径。
    // 缺省：服务端在 claim 事务内分配下一个 batch_index，避免老客户端无限 replay batch 0。
    let requested_batch_index = req.batch_index;
    let session_id = state
        .run_store_task("learning.next_words.resolve_session", {
            let user_id = user_id.clone();
            let requested = req.session_id.clone();
            move |store| -> Result<Option<String>, AppError> {
                if let Some(sid) = requested {
                    let session = store
                        .get_learning_session(&sid)?
                        .ok_or_else(|| AppError::not_found("学习会话不存在"))?;
                    if session.user_id != user_id {
                        return Err(AppError::forbidden("该会话属于其他用户"));
                    }
                    if session.status != SessionStatus::Active {
                        return Err(AppError::bad_request(
                            "LEARNING_SESSION_NOT_ACTIVE",
                            "学习会话已结束，无法继续取词",
                        ));
                    }
                    return Ok(Some(session.id));
                }
                Ok(store
                    .get_active_sessions_for_user(&user_id)?
                    .into_iter()
                    .next()
                    .map(|s| s.id))
            }
        })
        .await??;

    let config = state
        .run_store_task("learning.next_words.config", {
            let user_id = user_id.clone();
            move |store| store.get_study_config(&user_id)
        })
        .await??;
    let amas_state = state.amas().get_user_state_async(&user_id).await?;
    let mut strategy_params = state.amas().compute_strategy_from_state(&amas_state);

    // 获取 AMAS 配置用于动态调整和选词
    let amas_config = state.amas().get_config();
    let ls = &amas_config.learning_strategy;

    // 根据 session_performance 动态调整策略
    let session_context = if let Some(ref perf) = req.session_performance {
        if perf.recent_accuracy >= ls.session_boost_accuracy {
            strategy_params.difficulty =
                (strategy_params.difficulty + ls.difficulty_boost_step).min(1.0);
            strategy_params.new_ratio = (strategy_params.new_ratio + ls.ratio_boost_step).min(1.0);
        } else if perf.recent_accuracy <= ls.session_drop_accuracy {
            strategy_params.difficulty =
                (strategy_params.difficulty - ls.difficulty_drop_step).max(0.0);
            strategy_params.new_ratio = (strategy_params.new_ratio - ls.ratio_drop_step).max(0.0);
        }

        // 冲刺模式：接近目标时大量用新词
        if perf.mastered_count
            >= (perf.target_mastery_count as f64 * ls.sprint_mastery_ratio) as u32
        {
            strategy_params.new_ratio = ls.sprint_new_ratio;
        }

        // 构建 SessionSelectionContext
        let temporal_boost = state
            .amas()
            .get_temporal_boost_async(
                &user_id,
                Utc::now()
                    .format("%H")
                    .to_string()
                    .parse::<u8>()
                    .unwrap_or(12),
            )
            .await?;

        Some(SessionSelectionContext {
            error_prone_word_ids: perf.error_prone_word_ids.clone(),
            recently_mastered_word_ids: req.mastered_word_ids.clone().unwrap_or_default(),
            temporal_boost,
        })
    } else {
        None
    };

    let batch_size = strategy_params.batch_size as usize;
    let pool_size = state.config().limits.candidate_word_pool_size;
    let selected_wordbook_ids = config.selected_wordbook_ids.clone();
    let exclude_word_ids = req.exclude_word_ids;
    let session_id_for_select = session_id.clone();
    let (effective_batch_index, words) = state
        .run_store_task(
            "learning.next_words.select",
            move |store| -> Result<(u32, Vec<WordPublic>), AppError> {
                let mut candidate_word_ids = Vec::new();
                for book_id in &selected_wordbook_ids {
                    let wids = store.list_wordbook_words(book_id, pool_size, 0)?;
                    candidate_word_ids.extend(wids);
                }
                if candidate_word_ids.is_empty() {
                    let words = store.list_words(pool_size, 0)?;
                    candidate_word_ids.extend(words.into_iter().map(|w| w.id));
                }

                candidate_word_ids.sort();
                candidate_word_ids.dedup();

                let mut exclude_set: HashSet<String> = exclude_word_ids.into_iter().collect();
                if let Some(ref sid) = session_id_for_select {
                    for wid in store.get_session_shown_word_ids(sid)? {
                        exclude_set.insert(wid);
                    }
                }
                let filtered: Vec<String> = candidate_word_ids
                    .into_iter()
                    .filter(|wid| !exclude_set.contains(wid))
                    .collect();

                let scored = word_selector::select_words(
                    &store,
                    &user_id,
                    &filtered,
                    &strategy_params,
                    batch_size,
                    session_context.as_ref(),
                    &word_selector::SelectionConfigs {
                        word_selector: &amas_config.word_selector,
                        elo: &amas_config.elo,
                        memory_model: &amas_config.memory_model,
                    },
                )?;
                let scored_word_ids: Vec<String> =
                    scored.iter().map(|sw| sw.word_id.clone()).collect();

                let (used_idx, canonical_ids) = if let Some(ref sid) = session_id_for_select {
                    store.claim_session_batch(sid, requested_batch_index, &scored_word_ids)?
                } else {
                    (requested_batch_index.unwrap_or(0), scored_word_ids.clone())
                };

                let words_by_id = store.get_words_by_ids(&canonical_ids)?;
                let result: Vec<WordPublic> = canonical_ids
                    .iter()
                    .filter_map(|wid| words_by_id.get(wid).map(WordPublic::from))
                    .collect();
                Ok((used_idx, result))
            },
        )
        .await??;

    Ok(ok(serde_json::json!({
        "words": words,
        "batchSize": batch_size,
        "sessionId": session_id,
        "batchIndex": effective_batch_index,
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdjustWordsRequest {
    recent_performance: Option<f64>,
    user_state: Option<String>,
}

async fn adjust_words(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<AdjustWordsRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let amas_state = state.amas().get_user_state_async(&auth.user_id).await?;
    let mut strategy = state.amas().compute_strategy_from_state(&amas_state);
    let amas_config = state.amas().get_config();
    let ls = &amas_config.learning_strategy;

    if let Some(recent_performance) = req.recent_performance {
        if !recent_performance.is_finite() || !(0.0..=1.0).contains(&recent_performance) {
            return Err(AppError::bad_request(
                "LEARNING_INVALID_RECENT_PERFORMANCE",
                "recentPerformance 必须是0到1之间的数值",
            ));
        }

        if recent_performance >= ls.session_boost_accuracy {
            strategy.difficulty = (strategy.difficulty + ls.difficulty_boost_step).min(1.0);
            strategy.new_ratio = (strategy.new_ratio + ls.ratio_boost_step).min(1.0);
        } else if recent_performance <= ls.session_drop_accuracy {
            strategy.difficulty = (strategy.difficulty - ls.difficulty_drop_step).max(0.0);
            strategy.new_ratio = (strategy.new_ratio - ls.ratio_drop_step).max(0.0);
        }
    }

    if let Some(user_state) = req.user_state.as_deref() {
        match user_state.trim().to_ascii_lowercase().as_str() {
            "focused" | "engaged" | "confident" => {
                strategy.difficulty = (strategy.difficulty + ls.difficulty_boost_step).min(1.0);
                strategy.new_ratio = (strategy.new_ratio + ls.ratio_boost_step).min(1.0);
            }
            "tired" | "fatigued" | "frustrated" | "distracted" => {
                strategy.difficulty = (strategy.difficulty - ls.fatigue_difficulty_drop).max(0.0);
                strategy.new_ratio = (strategy.new_ratio - ls.ratio_drop_step).max(0.0);
                strategy.batch_size = ((strategy.batch_size as f64 * ls.fatigue_batch_scale)
                    .round()
                    .max(1.0)) as u32;
            }
            "review" => {
                strategy.review_mode = true;
                strategy.new_ratio = 0.0;
            }
            "sprint" => {
                strategy.new_ratio = ls.sprint_new_ratio;
            }
            _ => {}
        }
    }

    Ok(ok(serde_json::json!({
        "adjustedStrategy": strategy,
    })))
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct SessionEvent {
    client_event_id: String,
    word_id: String,
    is_correct: bool,
    response_time_ms: i64,
    client_ts_ms: i64,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct EventIngestSummary {
    count: usize,
    duplicates: usize,
    failed: usize,
    errors: Vec<EventIngestError>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EventIngestError {
    index: usize,
    client_event_id: String,
    code: String,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionWithEventIngest {
    #[serde(flatten)]
    session: LearningSession,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_ingest: Option<EventIngestSummary>,
}

fn clamp_event_created_at(client_ts_ms: i64, session: &LearningSession) -> DateTime<Utc> {
    let now = Utc::now();
    let parsed = match Utc.timestamp_millis_opt(client_ts_ms) {
        LocalResult::Single(dt) => dt,
        _ => return now,
    };
    let upper = now + Duration::minutes(EVENT_CLIENT_TS_FUTURE_TOLERANCE_MIN);
    // 时钟漂移防御：若 session.created_at 超过 upper（极端时钟错位），令 lower 不超过 upper，
    // 避免 clamp 在 lower > upper 时 panic。
    let lower = std::cmp::max(
        session.created_at,
        now - Duration::minutes(EVENT_CLIENT_TS_BACKFILL_LIMIT_MIN),
    )
    .min(upper);
    parsed.clamp(lower, upper)
}

async fn ingest_session_events(
    user_id: &str,
    session: &LearningSession,
    events: Vec<SessionEvent>,
    limit: usize,
    state: &AppState,
) -> Result<EventIngestSummary, AppError> {
    if events.len() > limit {
        return Err(AppError::bad_request(
            "LEARNING_EVENTS_TOO_LARGE",
            &format!("events 数量上限为 {limit}"),
        ));
    }

    let user_snapshot = state
        .run_store_task("learning.events.snapshot", {
            let user_id = user_id.to_string();
            move |store| capture_user_state_snapshot(&store, &user_id)
        })
        .await??;

    let mut summary = EventIngestSummary::default();
    let mut seen: HashSet<String> = HashSet::new();
    let mut has_committed_record = false;
    // 仅当至少一次进入 process_batch_record（可能触碰 AMAS user 级状态）时才允许外层回滚；
    // 纯前置验证失败不应触发回滚，避免覆盖并发请求写入的较新 user_state / ELO。
    let mut amas_attempted = false;

    for (index, event) in events.into_iter().enumerate() {
        let client_event_id = event.client_event_id.trim().to_string();
        let push_err = |summary: &mut EventIngestSummary, code: &str, message: &str| {
            summary.errors.push(EventIngestError {
                index,
                client_event_id: client_event_id.clone(),
                code: code.to_string(),
                message: message.to_string(),
            });
        };

        if client_event_id.is_empty() {
            push_err(&mut summary, "LEARNING_INVALID_EVENT_ID", "clientEventId 不能为空");
            continue;
        }
        if client_event_id.len() > EVENT_CLIENT_EVENT_ID_MAX_LEN {
            push_err(
                &mut summary,
                "LEARNING_INVALID_EVENT_ID",
                "clientEventId 长度超过上限",
            );
            continue;
        }
        if !seen.insert(client_event_id.clone()) {
            push_err(
                &mut summary,
                "LEARNING_DUPLICATE_EVENT_ID",
                "同一请求内 clientEventId 不能重复",
            );
            continue;
        }
        if !(0..=EVENT_RESPONSE_TIME_MAX_MS).contains(&event.response_time_ms) {
            push_err(
                &mut summary,
                "LEARNING_INVALID_RESPONSE_TIME",
                "responseTimeMs 必须在 0 到 300000 之间",
            );
            continue;
        }
        if event.client_ts_ms <= 0 {
            push_err(&mut summary, "LEARNING_INVALID_CLIENT_TS", "clientTsMs 必须大于 0");
            continue;
        }

        let req = CreateRecordRequest {
            client_record_id: Some(client_event_id.clone()),
            word_id: event.word_id,
            is_correct: event.is_correct,
            response_time_ms: event.response_time_ms,
            session_id: Some(session.id.clone()),
            is_quit: None,
            dwell_time_ms: None,
            pause_count: None,
            switch_count: None,
            retry_count: None,
            focus_loss_duration_ms: None,
            interaction_density: None,
            paused_time_ms: None,
            hint_used: None,
            confused_with: None,
            record_type: Some(RecordType::Learning),
            created_at_override: Some(clamp_event_created_at(event.client_ts_ms, session)),
        };

        amas_attempted = true;
        match process_batch_record(user_id, &req, state).await {
            // duplicate 命中早于任何 AMAS / ELO / DB 副作用，因此不计入“已提交”，
            // 也就不能用来抵消失败事件触发的全失败回滚。
            Ok(result) if result.duplicate => {
                summary.duplicates += 1;
            }
            Ok(_) => {
                summary.count += 1;
                has_committed_record = true;
            }
            Err(error) => {
                summary.errors.push(EventIngestError {
                    index,
                    client_event_id,
                    code: error.code.clone(),
                    message: error.message.clone(),
                });
            }
        }
    }

    summary.failed = summary.errors.len();

    // 全失败回滚：仅当本批次已实际触碰过 AMAS 流水线（amas_attempted）、未成功提交任何记录、
    // 且存在失败时，才撤销 user 级状态变更。纯前置验证失败不会改写 AMAS user 状态，跳过回滚
    // 可避免对并发请求写入的较新 user_state / ELO 造成 lost update。
    if amas_attempted && !has_committed_record && summary.failed > 0 {
        state
            .run_store_task("learning.events.restore_user_snapshot", {
                let user_id = user_id.to_string();
                let snapshot = user_snapshot.clone();
                move |store| restore_user_state_snapshot(&store, &user_id, &snapshot)
            })
            .await?;
    }

    Ok(summary)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SyncProgressRequest {
    session_id: String,
    total_questions: Option<u32>,
    context_shifts: Option<u32>,
    #[serde(default)]
    events: Option<Vec<SessionEvent>>,
}

async fn sync_progress(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<SyncProgressRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let SyncProgressRequest {
        session_id,
        total_questions,
        context_shifts,
        events,
    } = req;
    let user_id = auth.user_id;

    // events 缺省时保持原路径，零回归。
    let event_ingest = match events.filter(|v| !v.is_empty()) {
        Some(events) => {
            let session_for_events = state
                .run_store_task("learning.sync_progress.load_for_events", {
                    let user_id = user_id.clone();
                    let session_id = session_id.clone();
                    move |store| -> Result<_, AppError> {
                        let session = store
                            .get_learning_session(&session_id)?
                            .ok_or_else(|| AppError::not_found("学习会话不存在"))?;
                        if session.user_id != user_id {
                            return Err(AppError::forbidden("该会话属于其他用户"));
                        }
                        Ok(session)
                    }
                })
                .await??;
            Some(
                ingest_session_events(
                    &user_id,
                    &session_for_events,
                    events,
                    SYNC_PROGRESS_EVENTS_LIMIT,
                    &state,
                )
                .await?,
            )
        }
        None => None,
    };

    let user_id_for_task = user_id.clone();
    let session = state
        .run_store_task(
            "learning.sync_progress",
            move |store| -> Result<_, AppError> {
                let mut session = store
                    .get_learning_session(&session_id)?
                    .ok_or_else(|| AppError::not_found("学习会话不存在"))?;

                if session.user_id != user_id_for_task {
                    return Err(AppError::forbidden("该会话属于其他用户"));
                }

                if let Some(tq) = total_questions {
                    if tq > session.total_questions {
                        session.total_questions = tq;
                    }
                }
                if let Some(cs) = context_shifts {
                    if cs > session.context_shifts {
                        session.context_shifts = cs;
                    }
                }

                session.updated_at = Utc::now();
                store.update_learning_session(&session)?;
                Ok(session)
            },
        )
        .await??;

    Ok(ok(SessionWithEventIngest {
        session,
        event_ingest,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompleteSessionRequest {
    session_id: String,
    mastered_word_ids: Vec<String>,
    // 服务端从 records 派生，仅在 rollout 期接收用于客户端兼容
    #[allow(dead_code)]
    error_prone_word_ids: Vec<String>,
    avg_response_time_ms: i64,
    #[serde(default)]
    events: Option<Vec<SessionEvent>>,
}

async fn complete_session(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<CompleteSessionRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id;
    let events = req.events.clone();
    let mut session = state
        .run_store_task("learning.complete_session.load", {
            let session_id = req.session_id.clone();
            move |store| store.get_learning_session(&session_id)
        })
        .await??
        .ok_or_else(|| AppError::not_found("学习会话不存在"))?;

    if session.user_id != user_id {
        return Err(AppError::forbidden("该会话属于其他用户"));
    }

    let event_ingest = match events.filter(|v| !v.is_empty()) {
        Some(events) => {
            let summary = ingest_session_events(
                &user_id,
                &session,
                events,
                COMPLETE_SESSION_EVENTS_LIMIT,
                &state,
            )
            .await?;
            // ingest 已经原子更新 session 计数，重读以避免脏字段被回写。
            session = state
                .run_store_task("learning.complete_session.reload_after_events", {
                    let session_id = req.session_id.clone();
                    move |store| store.get_learning_session(&session_id)
                })
                .await??
                .ok_or_else(|| AppError::not_found("学习会话不存在"))?;
            Some(summary)
        }
        None => None,
    };

    let now = Utc::now();
    let duration_secs = (now - session.created_at).num_seconds();
    let hour_of_day = now.format("%H").to_string().parse::<u8>().unwrap_or(12);

    // 服务端真源：按 (user_id, session_id) 直接查询，不受全局 max_records_fetch 限制，
    // 避免长 session 漏算。
    let session_records = state
        .run_store_task("learning.complete_session.records", {
            let user_id = user_id.clone();
            let session_id = req.session_id.clone();
            move |store| store.list_records_by_session(&user_id, &session_id)
        })
        .await??;

    let total_in_session = session_records.len();
    let correct_in_session = session_records.iter().filter(|r| r.is_correct).count();
    let accuracy = if session.total_count > 0 {
        session.correct_count as f64 / session.total_count as f64
    } else if total_in_session > 0 {
        correct_in_session as f64 / total_in_session as f64
    } else if session.total_questions > 0 {
        session.correct_count as f64 / session.total_questions as f64
    } else {
        0.0
    };

    let mut session_word_ids: Vec<String> =
        session_records.iter().map(|r| r.word_id.clone()).collect();
    session_word_ids.sort();
    session_word_ids.dedup();

    let mut incorrect_word_ids: Vec<String> = session_records
        .iter()
        .filter(|r| !r.is_correct)
        .map(|r| r.word_id.clone())
        .collect();
    incorrect_word_ids.sort();
    incorrect_word_ids.dedup();

    let (server_mastered, server_error_prone) = state
        .run_store_task("learning.complete_session.derive_summary", {
            let user_id = user_id.clone();
            let session_word_ids = session_word_ids.clone();
            move |store| -> Result<(Vec<String>, Vec<String>), AppError> {
                let states = store.get_word_states_batch(&user_id, &session_word_ids)?;
                let mastered_set: HashSet<String> = states
                    .iter()
                    .filter(|s| s.state == WordState::Mastered)
                    .map(|s| s.word_id.clone())
                    .collect();
                let mut mastered: Vec<String> = mastered_set.iter().cloned().collect();
                mastered.sort();
                let error_prone: Vec<String> = incorrect_word_ids
                    .into_iter()
                    .filter(|w| !mastered_set.contains(w))
                    .collect();
                Ok((mastered, error_prone))
            }
        })
        .await??;

    // 对账日志：仅记录差异规模与少量样本，避免在 INFO 日志泄露用户级 word_id 全集
    let client_set: HashSet<&String> = req.mastered_word_ids.iter().collect();
    let server_set: HashSet<&String> = server_mastered.iter().collect();
    let client_only_count = client_set.difference(&server_set).count();
    let server_only_count = server_set.difference(&client_set).count();
    let agreed_count = client_set.intersection(&server_set).count();
    if client_only_count > 0 || server_only_count > 0 {
        tracing::info!(
            session_id = %req.session_id,
            client_only_count,
            server_only_count,
            agreed_count,
            "complete_session.mastered_mismatch"
        );
    }

    let amas_state = state.amas().get_user_state_async(&user_id).await?;
    let strategy = state.amas().compute_strategy_from_state(&amas_state);

    let summary = SessionSummary {
        accuracy,
        avg_response_time_ms: req.avg_response_time_ms,
        mastered_word_ids: server_mastered.clone(),
        error_prone_word_ids: server_error_prone,
        duration_secs,
        hour_of_day,
        final_difficulty: strategy.difficulty,
    };

    session.status = SessionStatus::Completed;
    session.actual_mastery_count = server_mastered.len() as u32;
    session.summary = Some(summary);
    session.updated_at = now;
    let session_for_store = session.clone();
    state
        .run_store_task("learning.complete_session.persist", move |store| {
            store.update_learning_session(&session_for_store)
        })
        .await??;

    let mastery_efficiency = if session.total_questions > 0 {
        server_mastered.len() as f64 / session.total_questions as f64
    } else {
        0.0
    };

    state
        .amas()
        .update_temporal_profile(
            &user_id,
            hour_of_day,
            accuracy,
            req.avg_response_time_ms as f64,
            mastery_efficiency,
        )
        .await?;

    Ok(ok(SessionWithEventIngest {
        session,
        event_ingest,
    }))
}

// ── pick-next-word ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PickNextWordRequest {
    active_word_ids: Vec<String>,
    error_word_ids: Vec<String>,
    last_shown_map: Option<std::collections::HashMap<String, u64>>,
    priority_map: Option<std::collections::HashMap<String, u32>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PickNextWordResponse {
    word: WordPublic,
    priority: String,
}

async fn pick_next_word(
    _auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<PickNextWordRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let max = state.config().limits.max_batch_size;
    if req.active_word_ids.is_empty() {
        return Err(AppError::bad_request(
            "LEARNING_NO_ACTIVE_WORDS",
            "activeWordIds 不能为空",
        ));
    }
    if req.active_word_ids.len() > max {
        return Err(AppError::bad_request(
            "LEARNING_TOO_MANY_IDS",
            &format!("activeWordIds 数量上限为 {max}"),
        ));
    }

    let last_shown = req.last_shown_map.unwrap_or_default();
    let priority_map = req.priority_map.unwrap_or_default();
    let response = state
        .run_store_task(
            "learning.pick_next_word",
            move |store| -> Result<_, AppError> {
                let error_set: std::collections::HashSet<&str> =
                    req.error_word_ids.iter().map(|s| s.as_str()).collect();

                let mut error_ids: Vec<&str> = req
                    .active_word_ids
                    .iter()
                    .map(|s| s.as_str())
                    .filter(|id| error_set.contains(id))
                    .collect();

                if !error_ids.is_empty() {
                    error_ids.sort_by_key(|id| last_shown.get(*id).copied().unwrap_or(0));
                    let chosen_id = error_ids[0].to_string();
                    let word = store
                        .get_word(&chosen_id)?
                        .ok_or_else(|| AppError::not_found("单词不存在"))?;
                    return Ok(PickNextWordResponse {
                        word: WordPublic::from(&word),
                        priority: "error_review".to_string(),
                    });
                }

                let mut normal_ids: Vec<&str> =
                    req.active_word_ids.iter().map(|s| s.as_str()).collect();
                normal_ids.sort_by(|a, b| {
                    let pa = priority_map.get(*a).copied().unwrap_or(0);
                    let pb = priority_map.get(*b).copied().unwrap_or(0);
                    pb.cmp(&pa).then_with(|| {
                        let ta = last_shown.get(*a).copied().unwrap_or(0);
                        let tb = last_shown.get(*b).copied().unwrap_or(0);
                        ta.cmp(&tb)
                    })
                });

                let chosen_id = normal_ids[0].to_string();
                let word = store
                    .get_word(&chosen_id)?
                    .ok_or_else(|| AppError::not_found("单词不存在"))?;

                Ok(PickNextWordResponse {
                    word: WordPublic::from(&word),
                    priority: "normal".to_string(),
                })
            },
        )
        .await??;

    Ok(ok(response))
}

// ── generate-options ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenerateOptionsRequest {
    word_id: String,
    mode: String,
    pool_word_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateOptionsResponse {
    options: Vec<String>,
    correct_index: usize,
}

async fn generate_options(
    _auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<GenerateOptionsRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let max = state.config().limits.max_batch_size;
    if req.pool_word_ids.len() > max {
        return Err(AppError::bad_request(
            "LEARNING_TOO_MANY_IDS",
            &format!("poolWordIds 数量上限为 {max}"),
        ));
    }

    let response = state
        .run_store_task(
            "learning.generate_options",
            move |store| -> Result<_, AppError> {
                let target = store
                    .get_word(&req.word_id)?
                    .ok_or_else(|| AppError::not_found("单词不存在"))?;

                // 题型语义：
                //   word-to-meaning / audio-to-meaning  → 选项展示为 meaning（前端区别在题面：文本 vs 音频）
                //   meaning-to-word / meaning-to-spelling → 选项展示为 text（前端区别在答题方式：选择 vs 拼写）
                let correct_answer = match req.mode.as_str() {
                    "word-to-meaning" | "audio-to-meaning" => target.meaning.clone(),
                    "meaning-to-word" | "meaning-to-spelling" => target.text.clone(),
                    _ => {
                        return Err(AppError::bad_request(
                            "LEARNING_INVALID_MODE",
                            "mode 仅支持 word-to-meaning / meaning-to-word / audio-to-meaning / meaning-to-spelling",
                        ));
                    }
                };
                let distractor_is_text = matches!(
                    req.mode.as_str(),
                    "meaning-to-word" | "meaning-to-spelling"
                );

                let other_ids: Vec<String> = req
                    .pool_word_ids
                    .iter()
                    .filter(|id| id.as_str() != req.word_id)
                    .cloned()
                    .collect();

                let words_map = store.get_words_by_ids(&other_ids)?;
                let mut distractors: Vec<String> = words_map
                    .values()
                    .map(|w| if distractor_is_text { w.text.clone() } else { w.meaning.clone() })
                    .filter(|s| s != &correct_answer)
                    .collect();

                let mut rng = rand::thread_rng();
                distractors.shuffle(&mut rng);
                distractors.truncate(3);

                if distractors.len() < 3 {
                    let fallback_words = store.list_words(100, 0)?;
                    let mut fallback_distractors: Vec<String> = fallback_words
                        .iter()
                        .filter(|w| w.id != req.word_id)
                        .map(|w| if distractor_is_text { w.text.clone() } else { w.meaning.clone() })
                        .filter(|s| s != &correct_answer)
                        .collect();
                    fallback_distractors.shuffle(&mut rng);
                    while distractors.len() < 3 && !fallback_distractors.is_empty() {
                        distractors.push(fallback_distractors.pop().unwrap());
                    }
                }
                while distractors.len() < 3 {
                    distractors.push("—".to_string());
                }

                let mut options = vec![correct_answer.clone()];
                options.extend(distractors);
                options.shuffle(&mut rng);

                let correct_index = options
                    .iter()
                    .position(|o| o == &correct_answer)
                    .unwrap_or(0);

                Ok(GenerateOptionsResponse {
                    options,
                    correct_index,
                })
            },
        )
        .await??;

    Ok(ok(response))
}
