use axum::extract::State;
use axum::routing::{get, post};
use axum::Router;

use crate::extractors::JsonBody;
use chrono::Utc;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::amas::word_selector::{self, SessionSelectionContext};
use crate::auth::AuthUser;
use crate::response::{ok, AppError};
use crate::routes::words::WordPublic;
use crate::state::AppState;
use crate::store::operations::learning_sessions::{LearningSession, SessionStatus, SessionSummary};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/session", post(create_or_resume_session))
        .route("/study-words", get(get_study_words))
        .route("/next-words", post(next_words))
        .route("/adjust-words", post(adjust_words))
        .route("/sync-progress", post(sync_progress))
        .route("/complete-session", post(complete_session))
        .route("/pick-next-word", post(pick_next_word))
        .route("/generate-options", post(generate_options))
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

        let amas_config = state.amas().get_config().await;
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
    let amas_config = state.amas().get_config().await;
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
    let config = state
        .run_store_task("learning.next_words.config", {
            let user_id = user_id.clone();
            move |store| store.get_study_config(&user_id)
        })
        .await??;
    let amas_state = state.amas().get_user_state_async(&user_id).await?;
    let mut strategy_params = state.amas().compute_strategy_from_state(&amas_state);

    // 获取 AMAS 配置用于动态调整和选词
    let amas_config = state.amas().get_config().await;
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
    let mastered_word_ids = req.mastered_word_ids;
    let words = state
        .run_store_task(
            "learning.next_words.select",
            move |store| -> Result<_, AppError> {
                if let Some(ref mastered_ids) = mastered_word_ids {
                    for wid in mastered_ids {
                        if let Some(mut wls) = store.get_word_learning_state(&user_id, wid)? {
                            wls.state = crate::store::operations::word_states::WordState::Mastered;
                            wls.updated_at = Utc::now();
                            store.set_word_learning_state(&wls)?;
                        }
                    }
                }

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

                let exclude_set: std::collections::HashSet<&str> =
                    exclude_word_ids.iter().map(|s| s.as_str()).collect();
                let filtered: Vec<String> = candidate_word_ids
                    .into_iter()
                    .filter(|wid| !exclude_set.contains(wid.as_str()))
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
                let words_by_id = store.get_words_by_ids(&scored_word_ids)?;
                Ok(scored
                    .iter()
                    .filter_map(|sw| words_by_id.get(&sw.word_id).map(WordPublic::from))
                    .collect::<Vec<_>>())
            },
        )
        .await??;

    Ok(ok(serde_json::json!({
        "words": words,
        "batchSize": batch_size,
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
    let amas_config = state.amas().get_config().await;
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SyncProgressRequest {
    session_id: String,
    total_questions: Option<u32>,
    context_shifts: Option<u32>,
}

async fn sync_progress(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<SyncProgressRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let session = state
        .run_store_task(
            "learning.sync_progress",
            move |store| -> Result<_, AppError> {
                let mut session = store
                    .get_learning_session(&req.session_id)?
                    .ok_or_else(|| AppError::not_found("学习会话不存在"))?;

                if session.user_id != auth.user_id {
                    return Err(AppError::forbidden("该会话属于其他用户"));
                }

                if let Some(tq) = req.total_questions {
                    if tq > session.total_questions {
                        session.total_questions = tq;
                    }
                }
                if let Some(cs) = req.context_shifts {
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

    Ok(ok(session))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompleteSessionRequest {
    session_id: String,
    mastered_word_ids: Vec<String>,
    error_prone_word_ids: Vec<String>,
    avg_response_time_ms: i64,
}

async fn complete_session(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<CompleteSessionRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id;
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

    let now = Utc::now();
    let duration_secs = (now - session.created_at).num_seconds();
    let hour_of_day = now.format("%H").to_string().parse::<u8>().unwrap_or(12);

    // Compute accuracy from incremental counters on the session
    let accuracy = if session.total_count > 0 {
        session.correct_count as f64 / session.total_count as f64
    } else if session.total_questions > 0 {
        // Fallback for sessions created before incremental counters:
        // filter by session_id and compute from matching records
        let session_records = state
            .run_store_task("learning.complete_session.records", {
                let user_id = user_id.clone();
                move |store| store.get_user_records(&user_id, 10000)
            })
            .await??;
        let total_in_session = session_records
            .iter()
            .filter(|r| r.session_id.as_deref() == Some(&req.session_id))
            .count();
        if total_in_session > 0 {
            let correct_in_session = session_records
                .iter()
                .filter(|r| r.session_id.as_deref() == Some(&req.session_id) && r.is_correct)
                .count();
            correct_in_session as f64 / total_in_session as f64
        } else {
            session.correct_count as f64 / session.total_questions as f64
        }
    } else {
        0.0
    };

    let amas_state = state.amas().get_user_state_async(&user_id).await?;
    let strategy = state.amas().compute_strategy_from_state(&amas_state);

    let summary = SessionSummary {
        accuracy,
        avg_response_time_ms: req.avg_response_time_ms,
        mastered_word_ids: req.mastered_word_ids.clone(),
        error_prone_word_ids: req.error_prone_word_ids.clone(),
        duration_secs,
        hour_of_day,
        final_difficulty: strategy.difficulty,
    };

    session.status = SessionStatus::Completed;
    session.actual_mastery_count = req.mastered_word_ids.len() as u32;
    session.summary = Some(summary);
    session.updated_at = now;
    let session_for_store = session.clone();
    state
        .run_store_task("learning.complete_session.persist", move |store| {
            store.update_learning_session(&session_for_store)
        })
        .await??;

    // 更新 HabitProfile.temporal_performance
    let mastery_efficiency = if session.total_questions > 0 {
        req.mastered_word_ids.len() as f64 / session.total_questions as f64
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

    Ok(ok(session))
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

                let correct_answer = match req.mode.as_str() {
                    "word-to-meaning" => target.meaning.clone(),
                    "meaning-to-word" => target.text.clone(),
                    _ => {
                        return Err(AppError::bad_request(
                            "LEARNING_INVALID_MODE",
                            "mode 仅支持 word-to-meaning 或 meaning-to-word",
                        ));
                    }
                };

                let other_ids: Vec<String> = req
                    .pool_word_ids
                    .iter()
                    .filter(|id| id.as_str() != req.word_id)
                    .cloned()
                    .collect();

                let words_map = store.get_words_by_ids(&other_ids)?;
                let mut distractors: Vec<String> = words_map
                    .values()
                    .map(|w| match req.mode.as_str() {
                        "meaning-to-word" => w.text.clone(),
                        _ => w.meaning.clone(),
                    })
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
                        .map(|w| match req.mode.as_str() {
                            "meaning-to-word" => w.text.clone(),
                            _ => w.meaning.clone(),
                        })
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
