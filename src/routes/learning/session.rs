use axum::extract::{Path, Query, State};
use axum::http::StatusCode;

use crate::extractors::JsonBody;
use chrono::{DateTime, Duration, LocalResult, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::auth::AuthUser;
use crate::response::{ok, paginated, AppError};
use crate::routes::records::{
    capture_user_state_snapshot, process_batch_record, restore_user_state_snapshot,
    CreateRecordRequest,
};
use crate::state::AppState;
use crate::store::operations::learning_sessions::{LearningSession, SessionStatus};
use crate::store::operations::records::RecordType;

use super::{
    EVENT_CLIENT_EVENT_ID_MAX_LEN, EVENT_CLIENT_TS_BACKFILL_LIMIT_MIN,
    EVENT_CLIENT_TS_FUTURE_TOLERANCE_MIN, EVENT_RESPONSE_TIME_MAX_MS,
};

// ── session list helpers ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ListSessionsQuery {
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

pub(super) async fn list_sessions(
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

// 客户端 validateDraftSession 用：按 id 校验 session 是否仍可恢复。
// 找不到 / 属于他人均返回 404 + LEARNING_SESSION_NOT_FOUND（避免信息泄漏）。
pub(super) async fn get_session(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id;
    let session = state
        .run_store_task("learning.get_session", {
            let session_id = session_id.clone();
            move |store| store.get_learning_session(&session_id)
        })
        .await??
        .filter(|s| s.user_id == user_id)
        .ok_or_else(|| AppError {
            status: StatusCode::NOT_FOUND,
            code: "LEARNING_SESSION_NOT_FOUND".into(),
            message: "学习会话不存在".into(),
            is_operational: true,
        })?;

    Ok(ok(session))
}

// ── create / resume session ──

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(super) struct CreateSessionRequest {
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

pub(super) async fn create_or_resume_session(
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

// ── shared event ingestion (used by progress.rs) ──

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionEvent {
    pub(crate) client_event_id: String,
    pub(crate) word_id: String,
    pub(crate) is_correct: bool,
    pub(crate) response_time_ms: i64,
    pub(crate) client_ts_ms: i64,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EventIngestSummary {
    pub(crate) count: usize,
    pub(crate) duplicates: usize,
    pub(crate) failed: usize,
    pub(crate) errors: Vec<EventIngestError>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EventIngestError {
    pub(crate) index: usize,
    pub(crate) client_event_id: String,
    pub(crate) code: String,
    pub(crate) message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionWithEventIngest {
    #[serde(flatten)]
    pub(crate) session: LearningSession,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) event_ingest: Option<EventIngestSummary>,
}

pub(super) fn clamp_event_created_at(
    client_ts_ms: i64,
    session: &LearningSession,
) -> DateTime<Utc> {
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

pub(super) async fn ingest_session_events(
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
            push_err(
                &mut summary,
                "LEARNING_INVALID_EVENT_ID",
                "clientEventId 不能为空",
            );
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
            push_err(
                &mut summary,
                "LEARNING_INVALID_CLIENT_TS",
                "clientTsMs 必须大于 0",
            );
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
            self_rating: None,
            question_mode: None,
            created_at_override: Some(clamp_event_created_at(event.client_ts_ms, session)),
        };

        amas_attempted = true;
        match process_batch_record(user_id, &req, state).await {
            // duplicate 命中早于任何 AMAS / ELO / DB 副作用，因此不计入"已提交"，
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
