use crate::auth::{AdminAuthUser, AuthUser};
use crate::constants::MAX_CONFUSION_PAIRS;
use crate::extractors::JsonBody;
use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};

use crate::response::{ok, AppError};
use crate::routes::words::WordPublic;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/etymology/:word_id", get(get_etymology))
        .route("/semantic/search", get(semantic_search))
        .route("/word-contexts/:word_id", get(get_word_contexts))
        .route(
            "/morphemes/:word_id",
            get(get_morphemes).post(set_morphemes),
        )
        .route("/confusion-pairs/:word_id", get(get_confusion_pairs))
}

// B52: Etymology (LLM-generated, cached in sled)
async fn get_etymology(
    _user: AuthUser,
    Path(word_id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let etymology = state
        .run_store_task(
            "content.get_etymology",
            move |store| -> Result<_, AppError> {
                if let Some(cached) = store.get_etymology(&word_id)? {
                    let is_pending_llm = cached
                        .get("status")
                        .and_then(|status| status.as_str())
                        .map(|status| status == "pending_llm")
                        .unwrap_or(false);

                    if !is_pending_llm {
                        return Ok(cached);
                    }

                    store.delete_etymology(&word_id)?;
                }

                let word = store
                    .get_word(&word_id)?
                    .ok_or_else(|| AppError::not_found("单词不存在"))?;

                let roots = match store.get_word_morphemes(&word_id)? {
                    Some(serde_json::Value::Array(arr)) => arr
                        .iter()
                        .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                        .map(|s| s.to_string())
                        .filter(|s| !s.trim().is_empty())
                        .collect::<Vec<String>>(),
                    _ => Vec::new(),
                };

                let etymology_text = if !roots.is_empty() {
                    format!(
                        "'{}' 可拆分为 {}。该解释基于词素规则推断，后续可由 LLM 结果覆盖。",
                        word.text,
                        roots.join(" + ")
                    )
                } else if !word.examples.is_empty() {
                    format!(
                        "'{}' 当前缺少词素拆分，已使用词条示例生成基础词源说明（非 LLM）。",
                        word.text
                    )
                } else {
                    format!(
                        "'{}' 当前暂无可用词素数据，已返回基础词源说明（非 LLM）。",
                        word.text
                    )
                };

                let etymology = serde_json::json!({
                    "wordId": word_id,
                    "word": word.text,
                    "etymology": etymology_text,
                    "roots": roots,
                    "generated": false,
                    "source": "rule_based_fallback",
                });

                store.set_etymology(&word_id, &etymology)?;
                Ok(etymology)
            },
        )
        .await??;
    Ok(ok(etymology))
}

// B53: Semantic search (requires embeddings)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SemanticSearchQuery {
    query: String,
    limit: Option<usize>,
}

async fn semantic_search(
    _user: AuthUser,
    Query(q): Query<SemanticSearchQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let query = q.query.trim().to_string();
    if query.is_empty() {
        return Err(AppError::bad_request("INVALID_QUERY", "搜索内容不能为空"));
    }
    let limit = q.limit.unwrap_or(10).clamp(1, 50);
    let query_for_search = query.clone();
    let (items, total) = state
        .run_store_task("content.semantic_search", move |store| {
            store.search_words(&query_for_search, limit, 0)
        })
        .await??;
    let items: Vec<WordPublic> = items.iter().map(WordPublic::from).collect();

    Ok(ok(serde_json::json!({
        "query": query,
        "results": items,
        "total": total,
        "method": "keyword_fallback",
        "degraded": true,
    })))
}

// B54: Word contexts
async fn get_word_contexts(
    _user: AuthUser,
    Path(word_id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let lookup_word_id = word_id.clone();
    let word = state
        .run_store_task("content.get_word_contexts", move |store| {
            store.get_word(&lookup_word_id)
        })
        .await??
        .ok_or_else(|| AppError::not_found("单词不存在"))?;

    // 从 word.examples 生成基本上下文数据
    let contexts: Vec<serde_json::Value> = word
        .examples
        .iter()
        .enumerate()
        .map(|(i, example)| {
            serde_json::json!({
                "id": format!("{}-ctx-{}", word_id, i),
                "sentence": example,
                "source": "word_examples",
            })
        })
        .collect();

    Ok(ok(serde_json::json!({
        "wordId": word_id,
        "word": word.text,
        "examples": word.examples,
        "contexts": contexts,
    })))
}

// B55: Word morphemes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WordMorphemes {
    word_id: String,
    morphemes: Vec<Morpheme>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Morpheme {
    text: String,
    #[serde(rename = "type")]
    morpheme_type: String, // prefix, root, suffix
    meaning: String,
}

async fn get_morphemes(
    _user: AuthUser,
    Path(word_id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let lookup_word_id = word_id.clone();
    let morphemes = match state
        .run_store_task("content.get_morphemes", move |store| {
            store.get_word_morphemes(&lookup_word_id)
        })
        .await??
    {
        Some(serde_json::Value::Array(arr)) => {
            let items: Vec<Morpheme> = arr
                .iter()
                .filter_map(|v| serde_json::from_value::<Morpheme>(v.clone()).ok())
                .collect();
            WordMorphemes {
                word_id: word_id.clone(),
                morphemes: items,
            }
        }
        _ => WordMorphemes {
            word_id: word_id.clone(),
            morphemes: Vec::new(),
        },
    };
    Ok(ok(morphemes))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetMorphemesRequest {
    morphemes: Vec<Morpheme>,
}

async fn set_morphemes(
    _admin: AdminAuthUser,
    Path(word_id): Path<String>,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<SetMorphemesRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let morphemes_json: Vec<serde_json::Value> = req
        .morphemes
        .iter()
        .map(|m| serde_json::to_value(m).unwrap_or_default())
        .collect();
    let word_id_for_update = word_id.clone();
    state
        .run_store_task("content.set_morphemes", move |store| {
            store.set_word_morphemes(&word_id_for_update, &morphemes_json)
        })
        .await??;
    let data = WordMorphemes {
        word_id,
        morphemes: req.morphemes,
    };
    Ok(ok(data))
}

// B56: Confusion pairs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfusionPair {
    word_id: String,
    word: String,
    meaning: String,
    similarity: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfusionPairsQuery {
    limit: Option<usize>,
}

async fn get_confusion_pairs(
    _user: AuthUser,
    Path(word_id): Path<String>,
    Query(q): Query<ConfusionPairsQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(20).clamp(1, MAX_CONFUSION_PAIRS);
    let response_word_id = word_id.clone();
    let pairs = state
        .run_store_task(
            "content.get_confusion_pairs",
            move |store| -> Result<_, AppError> {
                let raw_pairs = store.get_confusion_pairs_for_word(&word_id, limit)?;
                let mut pairs = Vec::new();
                for (other_word_id, score) in raw_pairs {
                    if let Some(word) = store.get_word(&other_word_id)? {
                        pairs.push(ConfusionPair {
                            word_id: other_word_id,
                            word: word.text.clone(),
                            meaning: word.meaning.clone(),
                            similarity: score.clamp(0.0, 1.0),
                        });
                    }
                }
                Ok(pairs)
            },
        )
        .await??;

    Ok(ok(serde_json::json!({
        "wordId": response_word_id,
        "confusionPairs": pairs,
    })))
}
