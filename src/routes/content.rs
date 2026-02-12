use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::Router;

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

    // TODO: 接入 LLM API 生成真实词源数据，当前返回 pending_llm 占位信息
    let etymology = serde_json::json!({
        "wordId": word_id,
        "word": word.text,
        "etymology": format!("'{}' 的词源分析尚未生成，需要 LLM 服务支持", word.text),
        "roots": [],
        "generated": false,
        "status": "pending_llm",
    });

    // 避免长期缓存 pending_llm 占位数据
    let is_pending_llm = etymology
        .get("status")
        .and_then(|status| status.as_str())
        .map(|status| status == "pending_llm")
        .unwrap_or(false);

    if !is_pending_llm {
        state
            .store()
            .etymologies
            .insert(
                key.as_bytes(),
                serde_json::to_vec(&etymology).map_err(|e| AppError::internal(&e.to_string()))?,
            )
            .map_err(|e| AppError::internal(&e.to_string()))?;
    }

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
    let limit = q.limit.unwrap_or(10).clamp(1, 50);

    // TODO: 接入向量数据库实现真正的语义搜索，当前 fallback 到文本匹配
    let (items, total) = state.store().search_words(&q.query, limit, 0)?;
    let items: Vec<WordPublic> = items.iter().map(WordPublic::from).collect();

    Ok(ok(serde_json::json!({
        "query": q.query,
        "results": items,
        "total": total,
        "method": "text_search",
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

    let prefix = format!("{}:", word_id);
    for item in state.store().confusion_pairs.scan_prefix(prefix.as_bytes()) {
        if pairs.len() >= limit {
            break;
        }
        let (_k, v) = match item {
            Ok(kv) => kv,
            Err(_) => continue,
        };
        if let Ok(val) = serde_json::from_slice::<ConfusionPair>(&v) {
            pairs.push(val);
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
                if let Ok(val) = serde_json::from_slice::<ConfusionPair>(&v) {
                    pairs.push(val);
                }
            }
        }
    }

    Ok(ok(serde_json::json!({
        "wordId": word_id,
        "confusionPairs": pairs,
    })))
}
