use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;

use crate::extractors::JsonBody;
use chrono::Utc;
use serde::Deserialize;

use crate::amas::types::{MasteryLevel, RawEvent};
use crate::auth::AuthUser;
use crate::constants::DEFAULT_HALF_LIFE_HOURS;
use crate::response::{created, ok, AppError};
use crate::services::event_bus::DomainEvent;
use crate::state::AppState;
use crate::store::operations::learning_sessions::LearningSession;
use crate::store::operations::records::LearningRecord;
use crate::store::operations::word_states::{WordLearningState, WordState};

use super::single::{acquire_user_pipeline_lock, CreateRecordRequest, CreateRecordResponse};

pub fn router() -> Router<AppState> {
    Router::new().route("/batch", post(batch_create_records))
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
    request_id: Option<axum::Extension<crate::middleware::request_id::RequestId>>,
    JsonBody(req): JsonBody<BatchCreateRecordsRequest>,
) -> Result<axum::response::Response, AppError> {
    // 任务C:从请求扩展取 request_id（中间件注入），批内逐条透传到 AMAS 决策。
    let request_id = request_id.map(|axum::Extension(rid)| rid.0);
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
        match process_batch_record(&user_id, item, &state, request_id.clone()).await {
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

    // 批级"全部失败回滚"已废除：per-record 快照/回滚现与 single 同款、含 user 级键
    //（user_state/ige/trust/swd/user_elo+trend），每条失败已在自身临界区内完整还原。
    // 保留批级回滚反而有害——批前快照在 per-user 锁外捕获，回滚会把批处理期间并发请求
    // 已提交的用户状态一并覆盖（与 per-record 快照竞态同类）。
    let has_new_records = results.iter().any(|r| !r.duplicate);

    // m037 软拦截:部分/全部失败时告警(admin 监控 + 给该 user 应用内通知),不阻断响应。
    if !errors.is_empty() {
        let severity = if has_new_records { "warning" } else { "error" };
        crate::services::alerting::raise_data_alert(
            &state,
            "amas.learning_record",
            "batch_process_failed",
            severity,
            "学习数据上报处理失败".to_string(),
            format!(
                "批量上报 {} 条中 {} 条处理失败",
                req.records.len(),
                errors.len()
            ),
            Some(user_id.clone()),
        )
        .await;
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

/// S5: 批量场景下的单条记录处理。快照/回滚与 single 同款（word 级 + user 级全量，
/// 见 capture_engine_state_snapshot），per-record 失败在自身临界区内完整还原。
pub(crate) async fn process_batch_record(
    user_id: &str,
    req: &CreateRecordRequest,
    state: &AppState,
    request_id: Option<String>,
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
        created_at: req.created_at_override.unwrap_or_else(Utc::now),
        record_type: req.record_type_or_default(),
        self_rating: req.self_rating,
        question_mode: req.question_mode.clone(),
    };
    let word_id = req.word_id.clone();
    let record_for_store = record.clone();
    let req_for_store = req.clone();

    // W1-1：幂等账本预检（语义同 single：挡 AMAS 已应用但记录未落库的崩溃窗口重放）。
    let already_processed = state
        .run_store_task("records.batch.check_processed", {
            let user_id = user_id_owned.clone();
            let record_id = record.id.clone();
            move |store| store.is_event_processed(&user_id, &record_id)
        })
        .await??;
    if already_processed {
        let record_for_replay = record.clone();
        state
            .run_store_task("records.batch.persist_replayed", move |store| {
                store
                    .create_record_with_updates(&record_for_replay, None, None, false)
                    .map_err(|e| AppError::internal(&e.to_string()))
            })
            .await??;
        return Ok(CreateRecordResponse {
            record,
            amas_result: None,
            duplicate: true,
        });
    }

    // per-user 流水线互斥（与 single 同款，见 USER_PIPELINE_LOCKS）：串行化同用户
    // 「AMAS → tx2」流水线，保持事件应用顺序与落库顺序一致。
    let _pipeline_guard = acquire_user_pipeline_lock(user_id).await;

    let amas_result = state
        .amas()
        .process_event_idempotent(
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
                question_mode: req.question_mode.clone(),
            },
            &record.id,
            request_id,
            // 正规学习流：计入全局 ELO 与 mastery 边沿流水（amas 诊断端点传 false）。
            true,
        )
        .await?;
    // W1-1 并发收口：None 表示并发同 client_record_id 请求抢先写入幂等标记、本次 AMAS 已整笔回滚，
    // 走与 already_processed 一致的裸记录回放（不重复累加 ELO/mastery/trust）。
    let Some((amas_result, swd_appended_seq)) = amas_result else {
        let record_for_replay = record.clone();
        state
            .run_store_task("records.batch.persist_replayed", move |store| {
                store
                    .create_record_with_updates(&record_for_replay, None, None, false)
                    .map_err(|e| AppError::internal(&e.to_string()))
            })
            .await??;
        return Ok(CreateRecordResponse {
            record,
            amas_result: None,
            duplicate: true,
        });
    };
    // S2 收尾：快照回滚已删除，swd append 句柄不再需要（重试恢复统一走幂等账本短路）。
    let _ = swd_appended_seq;
    let amas_result_for_store = amas_result.clone();

    state
        .run_store_task(
            "records.batch.persist",
            move |store| -> Result<_, AppError> {
                // ELO 现由 AMAS 引擎在 process_event_blocking → persist_engine_state_atomic
                // 的锁定原子事务内应用，路由侧不再重复 RMW（否则 ELO 双重累加）。
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
                    // total_attempts/correct_streak 不在此累加：store 层以 SQL 相对自增落库，
                    // struct 字段仅占位、写入时被忽略（同 single.rs 注释）。
                    if wm.next_review_interval_secs > 0 {
                        wls.next_review_date = Some(
                            Utc::now() + chrono::Duration::seconds(wm.next_review_interval_secs),
                        );
                    }
                    wls.updated_at = Utc::now();
                    next_word_state = Some(wls);
                }
                // Passed to create_record_with_updates as a delta — see single.rs's identical
                // comment / store.rs's create_record_with_updates learning_sessions comment.
                // 语义为「处于 Mastered 级」的水平判定而非进入边沿（同 single.rs 注释）。
                let just_mastered = amas_result_for_store
                    .word_mastery
                    .as_ref()
                    .is_some_and(|wm| wm.mastery_level == MasteryLevel::Mastered);

                let mut next_session: Option<LearningSession> = None;
                if let Some(ref sid) = req_for_store.session_id {
                    // 归属校验：仅累加调用者本人的会话，防止跨用户篡改他人会话统计 (IDOR)。
                    if let Some(mut session) = store
                        .get_learning_session(sid)?
                        .filter(|s| s.user_id == user_id_owned)
                    {
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
                        just_mastered,
                    )
                    .map_err(|error| {
                        // S2 收尾：手动快照回滚已删除（与 single 同款语义）。tx2 失败时 AMAS 状态
                        // 与幂等标记原样保留，客户端离线队列同 clientRecordId 重发命中幂等账本
                        // 短路、仅补落裸记录行——无 AMAS 双重累加，无陈旧快照覆盖并发已提交状态。
                        tracing::warn!(
                            user_id = %user_id_owned,
                            word_id = %word_id,
                            record_id = %record_for_store.id,
                            error = %error,
                            "批内记录落库失败（AMAS 已应用、标记保留），等待客户端重试补落记录行"
                        );
                        AppError::internal(&error.to_string())
                    })?;
                Ok(())
            },
        )
        .await??;

    // v1.1-P1 S2：与 single 路径对齐，写库成功后旁路 emit RecordCreated。
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
