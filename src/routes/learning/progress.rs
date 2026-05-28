use axum::extract::State;
use chrono::Utc;
use serde::Deserialize;
use std::collections::HashSet;

use crate::auth::AuthUser;
use crate::extractors::JsonBody;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::operations::learning_sessions::{SessionStatus, SessionSummary};
use crate::store::operations::word_states::WordState;

use super::session::{
    ingest_session_events, SessionEvent, SessionWithEventIngest,
};
use super::{COMPLETE_SESSION_EVENTS_LIMIT, SYNC_PROGRESS_EVENTS_LIMIT};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SyncProgressRequest {
    session_id: String,
    total_questions: Option<u32>,
    context_shifts: Option<u32>,
    #[serde(default)]
    events: Option<Vec<SessionEvent>>,
}

pub(super) async fn sync_progress(
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
pub(super) struct CompleteSessionRequest {
    session_id: String,
    mastered_word_ids: Vec<String>,
    // 服务端从 records 派生，仅在 rollout 期接收用于客户端兼容
    #[allow(dead_code)]
    error_prone_word_ids: Vec<String>,
    avg_response_time_ms: i64,
    #[serde(default)]
    events: Option<Vec<SessionEvent>>,
}

pub(super) async fn complete_session(
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

    // m025:用户活动日志 —— session.complete(失败仅 warn)
    {
        let store = state.store().clone();
        let user_id_for_log = user_id.clone();
        let session_id_for_log = req.session_id.clone();
        let total = total_in_session;
        let correct = correct_in_session;
        let acc = accuracy;
        tokio::task::spawn_blocking(move || {
            if let Err(e) = store.insert_user_activity(
                &user_id_for_log,
                "session.complete",
                Some(&serde_json::json!({
                    "sessionId": session_id_for_log,
                    "total": total,
                    "correct": correct,
                    "accuracy": acc,
                })),
                None,
            ) {
                tracing::warn!(error = %e, "写 session.complete activity 失败");
            }
        });
    }

    Ok(ok(SessionWithEventIngest {
        session,
        event_ingest,
    }))
}
