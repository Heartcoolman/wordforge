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

use super::single::{
    capture_user_state_snapshot, restore_user_state_snapshot, CreateRecordRequest,
    CreateRecordResponse,
};

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
    let user_id = auth.user_id;
    let user_snapshot = state
        .run_store_task("records.batch.snapshot", {
            let user_id = user_id.clone();
            move |store| capture_user_state_snapshot(&store, &user_id)
        })
        .await??;

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

    // 如果全部失败（无新记录写入），回滚到初始用户状态
    let has_new_records = results.iter().any(|r| !r.duplicate);
    if !has_new_records && !errors.is_empty() {
        state
            .run_store_task("records.batch.restore_user_snapshot", {
                let user_id = user_id.clone();
                let user_snapshot = user_snapshot.clone();
                move |store| restore_user_state_snapshot(&store, &user_id, &user_snapshot)
            })
            .await?;
    }

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

/// S5: 批量场景下的单条记录处理，只捕获 word 级快照（mastery + word_elo）
pub(crate) async fn process_batch_record(
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
                    .create_record_with_updates(&record_for_replay, None, None)
                    .map_err(|e| AppError::internal(&e.to_string()))
            })
            .await??;
        return Ok(CreateRecordResponse {
            record,
            amas_result: None,
            duplicate: true,
        });
    }

    // S6: 只捕获 word 级状态
    let mastery_key = format!("mastery:{word_id}");
    let (prev_mastery, prev_word_elo, prev_user_elo, prev_word_contrib) = state
        .run_store_task("records.batch.snapshot", {
            let user_id = user_id_owned.clone();
            let word_id = word_id.clone();
            let mastery_key = mastery_key.clone();
            move |store| {
                Ok::<_, crate::store::StoreError>((
                    store.get_engine_algo_state(&user_id, &mastery_key)?,
                    store.get_word_elo(&word_id)?,
                    store.get_user_elo(&user_id)?,
                    store.get_word_elo_user_contrib(&user_id, &word_id)?,
                ))
            }
        })
        .await??;

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
            },
            &record.id,
        )
        .await?;
    // W1-1 并发收口：None 表示并发同 client_record_id 请求抢先写入幂等标记、本次 AMAS 已整笔回滚，
    // 走与 already_processed 一致的裸记录回放（不重复累加 ELO/mastery/trust）。
    let Some(amas_result) = amas_result else {
        let record_for_replay = record.clone();
        state
            .run_store_task("records.batch.persist_replayed", move |store| {
                store
                    .create_record_with_updates(&record_for_replay, None, None)
                    .map_err(|e| AppError::internal(&e.to_string()))
            })
            .await??;
        return Ok(CreateRecordResponse {
            record,
            amas_result: None,
            duplicate: true,
        });
    };
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
                    )
                    .map_err(|error| {
                        // W1-1：原子回滚 word 级状态（mastery + ELO）+ 清幂等标记（同一 tx）。
                        // 守「标记存在 ⟺ AMAS 已应用」不变式，消除原多步非原子写之间的崩溃窗口
                        // （重试丢 AMAS）。batch 不碰 user_state（user 级快照在批级单独处理）。
                        let restore = crate::store::operations::engine::EngineStateRestore {
                            user_id: &user_id_owned,
                            user_state: None,
                            algo_states: &[(mastery_key.as_str(), &prev_mastery)],
                            user_elo: Some(&prev_user_elo),
                            word_elo: Some((&word_id, &prev_word_elo)),
                            word_elo_contrib: Some((&word_id, prev_word_contrib)),
                            clear_marker_record_id: Some(&record_for_store.id),
                        };
                        if let Err(e) = store.restore_engine_state_atomic(&restore) {
                            tracing::warn!(error = %e, "batch 原子回滚 word 状态+清标记失败");
                        }
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
