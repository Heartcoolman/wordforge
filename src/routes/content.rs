use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::Router;
use std::collections::HashSet;

use crate::auth::{AdminAuthUser, AuthUser};
use crate::constants::MAX_CONFUSION_PAIRS;
use crate::extractors::JsonBody;
use serde::{Deserialize, Serialize};

use crate::response::{ok, AppError};
use crate::routes::words::WordPublic;
use crate::state::AppState;
use crate::store::keys;

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
    let key = keys::etymology_key(&word_id)?;

    // Check cache first
    if let Some(raw) = state
        .store()
        .etymologies
        .get(key.as_bytes())
        .map_err(|e| AppError::internal(&e.to_string()))?
    {
        let cached: serde_json::Value =
            serde_json::from_slice(&raw).unwrap_or(serde_json::json!({}));
        let is_pending_llm = cached
            .get("status")
            .and_then(|status| status.as_str())
            .map(|status| status == "pending_llm")
            .unwrap_or(false);

        if !is_pending_llm {
            return Ok(ok(cached));
        }

        state
            .store()
            .etymologies
            .remove(key.as_bytes())
            .map_err(|e| AppError::internal(&e.to_string()))?;
    }

    // Look up the word
    let word = state
        .store()
        .get_word(&word_id)?
        .ok_or_else(|| AppError::not_found("Word not found"))?;

    // 优先读取词素缓存，生成可用的规则化词源说明，避免返回 pending 占位信息。
    let roots = {
        let morpheme_key = keys::word_morpheme_key(&word_id)?;
        match state
            .store()
            .word_morphemes
            .get(morpheme_key.as_bytes())
            .map_err(|e| AppError::internal(&e.to_string()))?
        {
            Some(raw) => serde_json::from_slice::<WordMorphemes>(&raw)
                .map(|m| {
                    m.morphemes
                        .into_iter()
                        .map(|item| item.text)
                        .filter(|item| !item.trim().is_empty())
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default(),
            None => Vec::new(),
        }
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

    state
        .store()
        .etymologies
        .insert(
            key.as_bytes(),
            serde_json::to_vec(&etymology).map_err(|e| AppError::internal(&e.to_string()))?,
        )
        .map_err(|e| AppError::internal(&e.to_string()))?;

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
    let query = q.query.trim();
    let limit = q.limit.unwrap_or(10).clamp(1, 50);

    // TODO: 接入向量数据库实现真正的语义搜索，当前 fallback 到文本匹配
    let (items, total) = state.store().search_words(query, limit, 0)?;
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
    let word = state
        .store()
        .get_word(&word_id)?
        .ok_or_else(|| AppError::not_found("Word not found"))?;

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
    let key = keys::word_morpheme_key(&word_id)?;
    let morphemes = match state
        .store()
        .word_morphemes
        .get(key.as_bytes())
        .map_err(|e| AppError::internal(&e.to_string()))?
    {
        Some(raw) => serde_json::from_slice::<WordMorphemes>(&raw).unwrap_or(WordMorphemes {
            word_id: word_id.clone(),
            morphemes: Vec::new(),
        }),
        None => WordMorphemes {
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
    let key = keys::word_morpheme_key(&word_id)?;
    let data = WordMorphemes {
        word_id,
        morphemes: req.morphemes,
    };
    state
        .store()
        .word_morphemes
        .insert(
            key.as_bytes(),
            serde_json::to_vec(&data).map_err(|e| AppError::internal(&e.to_string()))?,
        )
        .map_err(|e| AppError::internal(&e.to_string()))?;
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedConfusionPair {
    word_a: String,
    word_b: String,
    score: f64,
}

fn decode_confusion_pair(
    raw: &[u8],
    target_word_id: &str,
    state: &AppState,
) -> Option<ConfusionPair> {
    if let Ok(pair) = serde_json::from_slice::<ConfusionPair>(raw) {
        if pair.word_id == target_word_id {
            return None;
        }
        return Some(ConfusionPair {
            similarity: pair.similarity.clamp(0.0, 1.0),
            ..pair
        });
    }

    let cached = serde_json::from_slice::<CachedConfusionPair>(raw).ok()?;
    let other_word_id = if cached.word_a == target_word_id {
        cached.word_b
    } else if cached.word_b == target_word_id {
        cached.word_a
    } else {
        return None;
    };

    let other_word = state.store().get_word(&other_word_id).ok().flatten()?;
    Some(ConfusionPair {
        word_id: other_word.id,
        word: other_word.text,
        meaning: other_word.meaning,
        similarity: cached.score.clamp(0.0, 1.0),
    })
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

    let mut pairs = Vec::new();
    let mut seen = HashSet::new();

    let prefix = format!("{}:", word_id);
    for item in state.store().confusion_pairs.scan_prefix(prefix.as_bytes()) {
        if pairs.len() >= limit {
            break;
        }
        let (_k, v) = match item {
            Ok(kv) => kv,
            Err(_) => continue,
        };
        if let Some(val) = decode_confusion_pair(&v, &word_id, &state) {
            if seen.insert(val.word_id.clone()) {
                pairs.push(val);
            }
        }
    }

    if pairs.len() < limit {
        let suffix = format!(":{}", word_id);
        for item in state.store().confusion_pairs.iter() {
            if pairs.len() >= limit {
                break;
            }
            let (k, v) = match item {
                Ok(kv) => kv,
                Err(_) => continue,
            };
            let key_str = String::from_utf8_lossy(&k);
            if key_str.ends_with(&suffix) {
                if let Some(val) = decode_confusion_pair(&v, &word_id, &state) {
                    if seen.insert(val.word_id.clone()) {
                        pairs.push(val);
                    }
                }
            }
        }
    }

    Ok(ok(serde_json::json!({
        "wordId": word_id,
        "confusionPairs": pairs,
    })))
}
