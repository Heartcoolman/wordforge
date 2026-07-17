use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::Router;

use crate::extractors::JsonBody;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::constants::DEFAULT_HALF_LIFE_HOURS;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::operations::word_states::{WordLearningState, WordState};
use crate::store::{Store, StoreError};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WordLearningStateResponse {
    #[serde(flatten)]
    base: WordLearningState,
    bookmarked: bool,
}

fn enrich_one(
    store: &Store,
    user_id: &str,
    state: WordLearningState,
) -> Result<WordLearningStateResponse, StoreError> {
    let word_ids = vec![state.word_id.clone()];
    let favorites = store.get_word_favorite_statuses(user_id, &word_ids)?;
    Ok(WordLearningStateResponse {
        bookmarked: favorites.contains_key(&state.word_id),
        base: state,
    })
}

fn enrich_many(
    store: &Store,
    user_id: &str,
    states: Vec<WordLearningState>,
) -> Result<Vec<WordLearningStateResponse>, StoreError> {
    if states.is_empty() {
        return Ok(Vec::new());
    }
    let word_ids: Vec<String> = states.iter().map(|s| s.word_id.clone()).collect();
    let favorites = store.get_word_favorite_statuses(user_id, &word_ids)?;
    Ok(states
        .into_iter()
        .map(|s| WordLearningStateResponse {
            bookmarked: favorites.contains_key(&s.word_id),
            base: s,
        })
        .collect())
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/batch", post(batch_query))
        .route("/due/list", get(due_list))
        .route("/stats/overview", get(stats_overview))
        .route("/batch-update", post(batch_update))
        .route("/:word_id", get(get_word_state))
        .route("/:word_id/mark-mastered", post(mark_mastered))
        .route("/:word_id/reset", post(reset_word))
}

async fn get_word_state(
    auth: AuthUser,
    Path(word_id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let resp = state
        .run_store_task("word_states.get", move |store| -> Result<_, AppError> {
            let wls = store.get_word_learning_state(&auth.user_id, &word_id)?;
            match wls {
                Some(s) => Ok(Some(enrich_one(&store, &auth.user_id, s)?)),
                None => Ok(None),
            }
        })
        .await??;

    match resp {
        Some(r) => Ok(ok(r)),
        None => Err(AppError::not_found("单词学习状态不存在")),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchQueryRequest {
    word_ids: Vec<String>,
}

async fn batch_query(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<BatchQueryRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if req.word_ids.len() > state.config().limits.max_batch_size {
        return Err(AppError::bad_request(
            "BATCH_TOO_LARGE",
            &format!(
                "批量查询单词数量上限为{}",
                state.config().limits.max_batch_size
            ),
        ));
    }
    let states = state
        .run_store_task(
            "word_states.batch_query",
            move |store| -> Result<_, AppError> {
                let raw = store.get_word_states_batch(&auth.user_id, &req.word_ids)?;
                Ok(enrich_many(&store, &auth.user_id, raw)?)
            },
        )
        .await??;
    Ok(ok(states))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DueListQuery {
    limit: Option<usize>,
}

async fn due_list(
    auth: AuthUser,
    Query(q): Query<DueListQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let due = state
        .run_store_task(
            "word_states.due_list",
            move |store| -> Result<_, AppError> {
                let raw = store.get_due_words(&auth.user_id, limit)?;
                Ok(enrich_many(&store, &auth.user_id, raw)?)
            },
        )
        .await??;
    Ok(ok(due))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatsOverviewQuery {
    category: Option<String>,
}

async fn stats_overview(
    auth: AuthUser,
    Query(q): Query<StatsOverviewQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let category = match q.category.as_deref().unwrap_or("all") {
        "all" => None,
        "learning" => Some(crate::store::operations::records::RecordType::Learning),
        "review" => Some(crate::store::operations::records::RecordType::Review),
        other => {
            return Err(AppError::bad_request(
                "INVALID_CATEGORY",
                &format!("category 必须是 all、learning 或 review，收到 {other}"),
            ))
        }
    };
    let stats = state
        .run_store_task(
            "word_states.stats_overview",
            move |store| -> Result<_, AppError> {
                let mut s = store.get_word_state_stats_filtered(&auth.user_id, category)?;
                let (due_count, eta_minutes) = store.get_due_review_estimated_minutes(&auth.user_id)?;
                s.due_count = Some(due_count);
                s.due_review_estimated_minutes = Some(eta_minutes);
                Ok(s)
            },
        )
        .await??;
    Ok(ok(stats))
}

async fn mark_mastered(
    auth: AuthUser,
    Path(word_id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let wls = state
        .run_store_task(
            "word_states.mark_mastered",
            move |store| -> Result<_, AppError> {
                if store.get_word(&word_id)?.is_none() {
                    return Err(AppError::not_found("单词不存在"));
                }

                // read-modify-write 收进 BEGIN IMMEDIATE 事务，避免与并发学习记录写同一行时丢更新。
                let wls = store.with_user_tx(|conn| {
                    let mut wls = Store::get_word_learning_state_conn(conn, &auth.user_id, &word_id)?
                        .unwrap_or_else(|| WordLearningState {
                            user_id: auth.user_id.clone(),
                            word_id: word_id.clone(),
                            state: WordState::New,
                            mastery_level: 0.0,
                            next_review_date: None,
                            half_life: DEFAULT_HALF_LIFE_HOURS,
                            correct_streak: 0,
                            total_attempts: 0,
                            updated_at: Utc::now(),
                        });

                    wls.state = WordState::Mastered;
                    wls.mastery_level = 1.0;
                    // 掌握后清除复习排程，与 reset_word 一致，避免已掌握词残留在 due 队列/ETA 统计。
                    wls.next_review_date = None;
                    wls.updated_at = Utc::now();
                    Store::set_word_learning_state_conn(conn, &wls)?;
                    Ok(wls)
                })?;
                Ok(enrich_one(&store, &auth.user_id, wls)?)
            },
        )
        .await??;

    Ok(ok(wls))
}

async fn reset_word(
    auth: AuthUser,
    Path(word_id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let wls = state
        .run_store_task("word_states.reset", move |store| -> Result<_, AppError> {
            if store.get_word(&word_id)?.is_none() {
                return Err(AppError::not_found("单词不存在"));
            }

            let wls = WordLearningState {
                user_id: auth.user_id.clone(),
                word_id,
                state: WordState::New,
                mastery_level: 0.0,
                next_review_date: None,
                half_life: 24.0,
                correct_streak: 0,
                total_attempts: 0,
                updated_at: Utc::now(),
            };

            // 全列覆写仍可能与并发写者交错，收进 BEGIN IMMEDIATE 事务串行化。
            store.with_user_tx(|conn| Store::set_word_learning_state_conn(conn, &wls))?;
            Ok(enrich_one(&store, &auth.user_id, wls)?)
        })
        .await??;
    Ok(ok(wls))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchUpdateItem {
    word_id: String,
    state: Option<WordState>,
    mastery_level: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchUpdateRequest {
    updates: Vec<BatchUpdateItem>,
}

async fn batch_update(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<BatchUpdateRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if req.updates.len() > state.config().limits.max_batch_size {
        return Err(AppError::bad_request(
            "BATCH_TOO_LARGE",
            &format!("批量更新数量上限为{}", state.config().limits.max_batch_size),
        ));
    }
    let result = state
        .run_store_task(
            "word_states.batch_update",
            move |store| -> Result<_, AppError> {
                let word_ids: Vec<String> = req.updates.iter().map(|u| u.word_id.clone()).collect();
                let existing_words = store.get_words_by_ids(&word_ids)?;
                let missing: Vec<&str> = word_ids
                    .iter()
                    .filter(|id| !existing_words.contains_key(id.as_str()))
                    .map(|id| id.as_str())
                    .collect();
                if !missing.is_empty() {
                    return Err(AppError::bad_request(
                        "WORD_NOT_FOUND",
                        &format!("以下单词不存在：{}", missing.join(", ")),
                    ));
                }

                let result = store.with_transaction(|conn| {
                    let mut updated = 0usize;
                    for item in &req.updates {
                        let mut wls = Store::get_word_learning_state_conn(
                            conn,
                            &auth.user_id,
                            &item.word_id,
                        )?
                        .unwrap_or_else(|| WordLearningState {
                            user_id: auth.user_id.clone(),
                            word_id: item.word_id.clone(),
                            state: WordState::New,
                            mastery_level: 0.0,
                            next_review_date: None,
                            half_life: DEFAULT_HALF_LIFE_HOURS,
                            correct_streak: 0,
                            total_attempts: 0,
                            updated_at: Utc::now(),
                        });

                        if let Some(ref s) = item.state {
                            wls.state = s.clone();
                            // 状态切到 Mastered/New 时同步清除复习排程，保持 state 与 due 队列一致。
                            if matches!(s, WordState::Mastered | WordState::New) {
                                wls.next_review_date = None;
                            }
                            // 客户端只传 state 未传 mastery_level 时，强制 mastery_level 与终态对齐
                            // （Mastered→1.0、New→0.0），与 mark_mastered/reset_word 口径一致，
                            // 避免持久化出 state=MASTERED 但 mastery_level=0.0 的不一致进度。
                            if item.mastery_level.is_none() {
                                match s {
                                    WordState::Mastered => wls.mastery_level = 1.0,
                                    WordState::New => wls.mastery_level = 0.0,
                                    _ => {}
                                }
                            }
                            // 切到 Reviewing/Forgotten/Learning 但无排程日期时，默认立即可复习；否则该词
                            // 被标为需复习却因 next_review_date IS NULL 永不进入 due 队列而搁浅。
                            if matches!(
                                s,
                                WordState::Reviewing | WordState::Forgotten | WordState::Learning
                            ) && wls.next_review_date.is_none()
                            {
                                wls.next_review_date = Some(Utc::now());
                            }
                        }
                        if let Some(level) = item.mastery_level {
                            wls.mastery_level = level.clamp(0.0, 1.0);
                        }
                        wls.updated_at = Utc::now();
                        Store::set_word_learning_state_conn(conn, &wls)?;
                        updated += 1;
                    }
                    Ok(updated)
                })?;

                Ok(result)
            },
        )
        .await??;

    Ok(ok(serde_json::json!({"updated": result})))
}
