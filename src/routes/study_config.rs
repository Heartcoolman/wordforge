use axum::extract::State;
use axum::routing::get;
use axum::Router;

use crate::extractors::JsonBody;
use chrono::Utc;
use serde::Deserialize;

use crate::auth::AuthUser;
use crate::response::{ok, AppError};
use crate::routes::words::WordPublic;
use crate::state::AppState;
use crate::store::operations::study_configs::StudyMode;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_config).put(update_config))
        .route("/today-words", get(get_today_words))
        .route("/progress", get(get_progress))
}

async fn get_config(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id.clone();
    let config = state
        .run_store_task("study_config.get_config", move |store| {
            store.get_study_config(&user_id)
        })
        .await??;
    Ok(ok(config))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateStudyConfigRequest {
    selected_wordbook_ids: Option<Vec<String>>,
    daily_word_count: Option<u32>,
    study_mode: Option<StudyMode>,
    daily_mastery_target: Option<u32>,
}

async fn update_config(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<UpdateStudyConfigRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id.clone();
    let (config, before_snapshot) = state
        .run_store_task(
            "study_config.update_config",
            move |store| -> Result<_, AppError> {
                // 读改写收进单个 with_user_tx(BEGIN IMMEDIATE)事务,与 update_word 一致,
                // 避免两个并发请求各自读旧快照后整行覆盖造成丢更新(last-writer-wins)。
                // 外层 Result 承载 DB 错误,内层承载应用级校验错误(保留专属 code/状态)。
                store.with_user_tx(|conn| {
                    let mut config = crate::store::Store::get_study_config_conn(conn, &user_id)?;
                    // m025:保存 before 快照用于活动日志 from/to diff
                    let before = serde_json::json!({
                        "dailyWordCount": config.daily_word_count,
                        "dailyMasteryTarget": config.daily_mastery_target,
                    });

                    if let Some(ids) = req.selected_wordbook_ids {
                        for id in &ids {
                            // 归属校验:仅 system 词书(user_id=None)或本人词书可选入,
                            // 防止把他人私有词书 id 写入 selected_wordbook_ids 越权读取(IDOR)。
                            match crate::store::Store::get_wordbook_conn(conn, id)? {
                                None => {
                                    return Ok(Err(AppError::bad_request(
                                        "WORDBOOK_NOT_FOUND",
                                        &format!("词书 '{}' 不存在", id),
                                    )));
                                }
                                Some(book) => {
                                    if book.user_id.is_some()
                                        && book.user_id.as_deref() != Some(user_id.as_str())
                                    {
                                        return Ok(Err(AppError::forbidden(&format!(
                                            "无权使用词书 '{}'",
                                            id
                                        ))));
                                    }
                                }
                            }
                        }
                        config.selected_wordbook_ids = ids;
                    }
                    if let Some(count) = req.daily_word_count {
                        config.daily_word_count = count.clamp(1, 200);
                    }
                    if let Some(mode) = req.study_mode {
                        config.study_mode = mode;
                    }
                    if let Some(target) = req.daily_mastery_target {
                        config.daily_mastery_target = target.clamp(1, 100);
                    }

                    crate::store::Store::set_study_config_conn(conn, &config)?;
                    Ok(Ok((config, before)))
                })
                .map_err(AppError::from)?
            },
        )
        .await??;

    // m025:用户活动日志 —— goal.update(失败仅 warn)
    {
        let store = state.store().clone();
        let user_id_for_log = auth.user_id.clone();
        let after = serde_json::json!({
            "dailyWordCount": config.daily_word_count,
            "dailyMasteryTarget": config.daily_mastery_target,
        });
        tokio::task::spawn_blocking(move || {
            if let Err(e) = store.insert_user_activity(
                &user_id_for_log,
                "goal.update",
                Some(&serde_json::json!({ "from": before_snapshot, "to": after })),
                None,
            ) {
                tracing::warn!(error = %e, "写 goal.update activity 失败");
            }
        });
    }

    Ok(ok(config))
}

async fn get_today_words(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id.clone();
    let payload = state
        .run_store_task(
            "study_config.get_today_words",
            move |store| -> Result<_, AppError> {
                let config = store.get_study_config(&user_id)?;
                let mut all_word_ids = Vec::new();

                // 不可在 store 层用 daily_word_count + offset 0 预切片：那样候选池永远是
                // 每本书 added_at 最靠前的 N 个词，过滤掉今日已学后无法推进到 N+1..end，
                // 用户永远学不到后续生词。这里拉取词库全部 word_id（无界），再排除「全时段
                // 已学」与「今日已学」，最后才截取 daily_word_count，使候选池随进度前移。
                for book_id in &config.selected_wordbook_ids {
                    let word_ids = store.list_wordbook_words(book_id, usize::MAX, 0)?;
                    all_word_ids.extend(word_ids);
                }

                if all_word_ids.is_empty() {
                    let words = store.list_words(usize::MAX, 0)?;
                    for w in &words {
                        all_word_ids.push(w.id.clone());
                    }
                }

                all_word_ids.sort();
                all_word_ids.dedup();

                // 跨天去重：排除用户全时段已学过的词（word_learning_states 中 state != 'NEW'），
                // 使候选池随学习进度推进，而不会反复下发每本书最靠前的 N 个词。
                let learned = store.list_learned_word_ids(&user_id)?;
                all_word_ids.retain(|wid| !learned.contains(wid.as_str()));

                // 用日期范围查询取「今日」全部记录，而非最近 500 条切片：高产用户单日
                // >500 条记录时，早些时段学过的词会被挤出窗口而被重复下发，违反每日新词契约。
                // 这是与上面「全时段已学」过滤叠加的同日守卫（覆盖刚学但状态仍为 NEW 的词）。
                let today = Utc::now().date_naive();
                let today_start = today
                    .and_hms_opt(0, 0, 0)
                    .map(|naive| naive.and_utc())
                    .unwrap_or_else(Utc::now);
                let today_end = today_start + chrono::Duration::days(1);
                let today_records =
                    store.get_user_records_between(&user_id, today_start, today_end)?;
                let studied_today: std::collections::HashSet<&str> = today_records
                    .iter()
                    .map(|r| r.word_id.as_str())
                    .collect();
                all_word_ids.retain(|wid| !studied_today.contains(wid.as_str()));
                all_word_ids.truncate(config.daily_word_count as usize);

                let mut words = Vec::new();
                for wid in &all_word_ids {
                    if let Some(word) = store.get_word(wid)? {
                        words.push(WordPublic::from(&word));
                    }
                }

                Ok(serde_json::json!({
                    "words": words,
                    "target": config.daily_word_count,
                }))
            },
        )
        .await??;

    Ok(ok(payload))
}

async fn get_progress(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id.clone();
    let payload = state
        .run_store_task(
            "study_config.get_progress",
            move |store| -> Result<_, AppError> {
                let config = store.get_study_config(&user_id)?;
                let stats = store.get_word_state_stats(&user_id)?;

                Ok(serde_json::json!({
                    "studied": stats.mastered + stats.reviewing,
                    "target": config.daily_mastery_target,
                    "new": stats.new_count,
                    "learning": stats.learning,
                    "reviewing": stats.reviewing,
                    "mastered": stats.mastered,
                }))
            },
        )
        .await??;
    Ok(ok(payload))
}
