use axum::extract::State;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::extractors::JsonBody;
use crate::response::{ok, AppError};
use crate::routes::words::WordPublic;
use crate::state::AppState;

// ── pick-next-word ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PickNextWordRequest {
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

pub(super) async fn pick_next_word(
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
pub(super) struct GenerateOptionsRequest {
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

pub(super) async fn generate_options(
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

                // 题型语义：
                //   word-to-meaning / audio-to-meaning  → 选项展示为 meaning（前端区别在题面：文本 vs 音频）
                //   meaning-to-word / meaning-to-spelling → 选项展示为 text（前端区别在答题方式：选择 vs 拼写）
                let correct_answer = match req.mode.as_str() {
                    "word-to-meaning" | "audio-to-meaning" => target.meaning.clone(),
                    "meaning-to-word" | "meaning-to-spelling" => target.text.clone(),
                    _ => {
                        return Err(AppError::bad_request(
                            "LEARNING_INVALID_MODE",
                            "mode 仅支持 word-to-meaning / meaning-to-word / audio-to-meaning / meaning-to-spelling",
                        ));
                    }
                };
                let distractor_is_text = matches!(
                    req.mode.as_str(),
                    "meaning-to-word" | "meaning-to-spelling"
                );

                let other_ids: Vec<String> = req
                    .pool_word_ids
                    .iter()
                    .filter(|id| id.as_str() != req.word_id)
                    .cloned()
                    .collect();

                let words_map = store.get_words_by_ids(&other_ids)?;
                let mut distractors: Vec<String> = words_map
                    .values()
                    .map(|w| if distractor_is_text { w.text.clone() } else { w.meaning.clone() })
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
                        .map(|w| if distractor_is_text { w.text.clone() } else { w.meaning.clone() })
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
