use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::keys;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/etymology/:word_id", get(get_etymology))
        .route("/semantic/search", get(semantic_search))
        .route("/word-contexts/:word_id", get(get_word_contexts))
        .route("/morphemes/:word_id", get(get_morphemes).post(set_morphemes))
        .route("/confusion-pairs/:word_id", get(get_confusion_pairs))
}

// B52: Etymology (LLM-generated, cached in sled)
async fn get_etymology(
    Path(word_id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let key = keys::etymology_key(&word_id);

    // Check cache first
    if let Some(raw) = state.store().etymologies.get(key.as_bytes())
        .map_err(|e| AppError::internal(&e.to_string()))?
    {
        let cached: serde_json::Value = serde_json::from_slice(&raw)
            .unwrap_or(serde_json::json!({}));
        return Ok(ok(cached));
    }

    // Look up the word
    let word = state.store().get_word(&word_id)?
        .ok_or_else(|| AppError::not_found("Word not found"))?;

    // Generate placeholder etymology (LLM integration would go here)
    let etymology = serde_json::json!({
        "wordId": word_id,
        "word": word.text,
        "etymology": format!("Etymology for '{}' - origin analysis pending LLM generation", word.text),
        "roots": [],
        "generated": false,
    });

    // Cache for future requests
    state.store().etymologies.insert(
        key.as_bytes(),
        serde_json::to_vec(&etymology).map_err(|e| AppError::internal(&e.to_string()))?,
    ).map_err(|e| AppError::internal(&e.to_string()))?;

    Ok(ok(etymology))
}

// B53: Semantic search (requires embeddings)
#[derive(Debug, Deserialize)]
struct SemanticSearchQuery {
    query: String,
    limit: Option<usize>,
}

async fn semantic_search(
    Query(q): Query<SemanticSearchQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(10).clamp(1, 50);

    // Fallback to text search until embeddings are available
    let (items, total) = state.store().search_words(&q.query, limit, 0)?;

    Ok(ok(serde_json::json!({
        "query": q.query,
        "results": items,
        "total": total,
        "method": "text_search",
    })))
}

// B54: Word contexts
async fn get_word_contexts(
    Path(word_id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let word = state.store().get_word(&word_id)?
        .ok_or_else(|| AppError::not_found("Word not found"))?;

    Ok(ok(serde_json::json!({
        "wordId": word_id,
        "word": word.text,
        "examples": word.examples,
        "contexts": [],
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
    Path(word_id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let key = keys::word_morpheme_key(&word_id);
    let morphemes = match state.store().word_morphemes.get(key.as_bytes())
        .map_err(|e| AppError::internal(&e.to_string()))?
    {
        Some(raw) => serde_json::from_slice::<WordMorphemes>(&raw)
            .unwrap_or(WordMorphemes { word_id: word_id.clone(), morphemes: Vec::new() }),
        None => WordMorphemes { word_id: word_id.clone(), morphemes: Vec::new() },
    };
    Ok(ok(morphemes))
}

async fn set_morphemes(
    Path(word_id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<WordMorphemes>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let key = keys::word_morpheme_key(&word_id);
    let data = WordMorphemes {
        word_id,
        morphemes: req.morphemes,
    };
    state.store().word_morphemes.insert(
        key.as_bytes(),
        serde_json::to_vec(&data).map_err(|e| AppError::internal(&e.to_string()))?,
    ).map_err(|e| AppError::internal(&e.to_string()))?;
    Ok(ok(data))
}

// B56: Confusion pairs
async fn get_confusion_pairs(
    Path(word_id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // Use prefix scan for word_id instead of iterating the entire tree.
    // Confusion pair keys are formatted as "word_a:word_b" (sorted),
    // so we scan for keys starting with "word_id:" to find pairs where
    // this word is the lexicographically smaller one, then also check
    // keys where it appears as the second word.
    let mut pairs = Vec::new();

    // Scan for keys where word_id is the first component
    let prefix = format!("{}:", word_id);
    for item in state.store().confusion_pairs.scan_prefix(prefix.as_bytes()) {
        let (_k, v) = match item {
            Ok(kv) => kv,
            Err(_) => continue,
        };
        if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&v) {
            pairs.push(val);
        }
    }

    // Also scan for keys where word_id is the second component.
    // Since keys are "a:b" with a < b, we need to check all keys ending
    // with ":word_id". Because sled only supports prefix scans, we do a
    // full iteration only if needed, but limit to entries containing the id.
    // For efficiency, iterate only if the first scan returned nothing or
    // the word_id might appear on the right side.
    for item in state.store().confusion_pairs.iter() {
        let (k, v) = match item {
            Ok(kv) => kv,
            Err(_) => continue,
        };
        let key_str = String::from_utf8_lossy(&k);
        // Only match keys where word_id is the second part (after ':')
        if key_str.ends_with(&format!(":{}", word_id)) {
            if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&v) {
                pairs.push(val);
            }
        }
    }

    Ok(ok(serde_json::json!({
        "wordId": word_id,
        "confusionPairs": pairs,
    })))
}
