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
    capture_user_state_snapshot, restore_engine_algo_state, restore_user_state_snapshot,
    CreateRecordRequest, CreateRecordResponse,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/batch", post(batch_create_records))
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
            format!("批量上报 {} 条中 {} 条处理失败", req.records.len(), errors.len()),
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

    // S6: 只捕获 word 级状态
    let mastery_key = format!("mastery:{word_id}");
    let (prev_mastery, prev_word_elo, prev_user_elo) = state
        .run_store_task("records.batch.snapshot", {
            let user_id = user_id_owned.clone();
            let word_id = word_id.clone();
            let mastery_key = mastery_key.clone();
            move |store| {
                Ok::<_, crate::store::StoreError>((
                    store.get_engine_algo_state(&user_id, &mastery_key)?,
                    store.get_word_elo(&word_id)?,
                    store.get_user_elo(&user_id)?,
                ))
            }
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
                        restore_engine_algo_state(
                            &store,
                            &user_id_owned,
                            &mastery_key,
                            &prev_mastery,
                        );
                        if let Err(e) = store.set_word_elo(&word_id, &prev_word_elo) {
                            tracing::warn!(error = %e, "Failed to rollback word ELO in batch");
                        }
                        if let Err(e) = store.set_user_elo(&user_id_owned, &prev_user_elo) {
                            tracing::warn!(error = %e, "Failed to rollback user ELO in batch");
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
