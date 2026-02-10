use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::amas::types::{MasteryLevel, RawEvent};
use crate::auth::AuthUser;
use crate::response::{created, ok, AppError};
use crate::state::AppState;
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
struct ListRecordsQuery {
    limit: Option<usize>,
    offset: Option<usize>,
}

impl ListRecordsQuery {
    fn limit(&self) -> usize {
        self.limit.unwrap_or(50)
    }
    fn offset(&self) -> usize {
        self.offset.unwrap_or(0)
    }
}

async fn list_records(
    auth: AuthUser,
    Query(q): Query<ListRecordsQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit().clamp(1, 200);
    let offset = q.offset();
    let records = state
        .store()
        .get_user_records_with_offset(&auth.user_id, limit, offset)?;
    Ok(ok(records))
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CreateRecordRequest {
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

async fn process_single_record(
    user_id: &str,
    req: &CreateRecordRequest,
    state: &AppState,
) -> Result<(LearningRecord, serde_json::Value), AppError> {
    let record = LearningRecord {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user_id.to_string(),
        word_id: req.word_id.clone(),
        is_correct: req.is_correct,
        response_time_ms: req.response_time_ms,
        session_id: req.session_id.clone(),
        created_at: Utc::now(),
    };

    state.store().create_record(&record)?;

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
            },
        )
        .await?;

    // B11: Update word_learning_states from AMAS result
    if let Some(ref wm) = amas_result.word_mastery {
        let new_state = match wm.mastery_level {
            MasteryLevel::New => WordState::New,
            MasteryLevel::Learning => WordState::Learning,
            MasteryLevel::Reviewing => WordState::Reviewing,
            MasteryLevel::Mastered => WordState::Mastered,
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
                half_life: 24.0,
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
            wls.next_review_date = Some(
                Utc::now()
                    + chrono::Duration::seconds(wm.next_review_interval_secs),
            );
        }
        wls.updated_at = Utc::now();
        state.store().set_word_learning_state(&wls)?;
    }

    // B11: Update learning session counters
    if let Some(ref sid) = req.session_id {
        if let Some(mut session) = state.store().get_learning_session(sid)? {
            session.total_questions += 1;
            if let Some(ref wm) = amas_result.word_mastery {
                if wm.mastery_level == MasteryLevel::Mastered {
                    session.actual_mastery_count += 1;
                }
            }
            session.updated_at = Utc::now();
            state.store().update_learning_session(&session)?;
        }
    }

    let amas_json = serde_json::to_value(&amas_result)
        .map_err(|e| AppError::internal(&e.to_string()))?;

    Ok((record, amas_json))
}

async fn create_record(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateRecordRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let (record, amas_result) = process_single_record(&auth.user_id, &req, &state).await?;

    Ok(created(serde_json::json!({
        "record": record,
        "amasResult": amas_result,
    })))
}

// B33: Batch submit records
#[derive(Debug, Deserialize)]
struct BatchCreateRecordsRequest {
    records: Vec<CreateRecordRequest>,
}

async fn batch_create_records(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<BatchCreateRecordsRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if req.records.len() > 500 {
        return Err(AppError::bad_request(
            "BATCH_TOO_LARGE",
            "batch_create_records accepts at most 500 records",
        ));
    }
    let mut results = Vec::new();
    for item in &req.records {
        let (record, amas_result) =
            process_single_record(&auth.user_id, item, &state).await?;
        results.push(serde_json::json!({
            "record": record,
            "amasResult": amas_result,
        }));
    }

    Ok(created(serde_json::json!({
        "count": results.len(),
        "items": results,
    })))
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
    let records = state.store().get_user_records(&auth.user_id, 10_000)?;
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

    // Current streak (consecutive days) - matches compute_streak_days logic
    let streak = {
        let today = Utc::now().date_naive();
        let dates: std::collections::BTreeSet<chrono::NaiveDate> = by_day
            .keys()
            .filter_map(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
            .collect();

        let mut s = 0u32;
        let mut current = today;

        // If no activity today, check if yesterday counts
        if !dates.contains(&current) {
            match current.pred_opt() {
                Some(yesterday) if dates.contains(&yesterday) => current = yesterday,
                _ => { /* streak stays 0 */ }
            }
        }

        if dates.contains(&current) {
            while dates.contains(&current) {
                s += 1;
                current = match current.pred_opt() {
                    Some(d) => d,
                    None => break,
                };
            }
        }
        s
    };

    Ok(ok(serde_json::json!({
        "total": total,
        "correct": correct,
        "accuracy": accuracy,
        "streak": streak,
        "daily": daily,
    })))
}
