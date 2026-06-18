use axum::extract::State;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::amas::word_selector::{self, SessionSelectionContext};
use crate::auth::AuthUser;
use crate::extractors::JsonBody;
use crate::response::{ok, AppError};
use crate::routes::words::WordPublic;
use crate::state::AppState;
use crate::store::operations::learning_sessions::SessionStatus;

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

pub(super) async fn get_study_words(
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
                        multi_trace_enabled: amas_config.feature_flags.multi_trace_enabled,
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
pub(super) struct NextWordsRequest {
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
pub(super) struct SessionPerformanceData {
    recent_accuracy: f64,
    mastered_count: u32,
    target_mastery_count: u32,
    error_prone_word_ids: Vec<String>,
}

pub(super) async fn next_words(
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
            // ② 混淆隔离对端在下方 store task 内按已出现词查 confusion_pairs 填充（需 DB 访问）。
            confusion_exclude_word_ids: Vec::new(),
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
                let mut shown_ids: Vec<String> = Vec::new();
                if let Some(ref sid) = session_id_for_select {
                    for wid in store.get_session_shown_word_ids(sid)? {
                        shown_ids.push(wid.clone());
                        exclude_set.insert(wid);
                    }
                }
                let filtered: Vec<String> = candidate_word_ids
                    .into_iter()
                    .filter(|wid| !exclude_set.contains(wid))
                    .collect();

                // ② 混淆隔离（Phase 1b）：flag 开启时，把本 session 已出现词的高分混淆对端注入
                // context（候选池里命中者评分被 dampen）。flag 关 → 不查、不填充 → 选词 bit-exact legacy。
                let mut session_context = session_context;
                if amas_config.feature_flags.confusion_isolation_enabled {
                    if let Some(ctx) = session_context.as_mut() {
                        let min_score = amas_config.word_selector.confusion_min_score;
                        let mut conf: HashSet<String> = HashSet::new();
                        for wid in &shown_ids {
                            for (other, score) in store.get_confusion_pairs_for_word(wid, 20)? {
                                if score >= min_score {
                                    conf.insert(other);
                                }
                            }
                        }
                        ctx.confusion_exclude_word_ids = conf.into_iter().collect();
                    }
                }

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
                        multi_trace_enabled: amas_config.feature_flags.multi_trace_enabled,
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
pub(super) struct AdjustWordsRequest {
    recent_performance: Option<f64>,
    user_state: Option<String>,
}

pub(super) async fn adjust_words(
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
