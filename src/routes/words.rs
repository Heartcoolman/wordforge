use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::response::{created, ok, AppError};
use crate::state::AppState;
use crate::store::operations::words::Word;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_words).post(create_word))
        .route("/count", get(count_words))
        .route("/batch", post(batch_create_words))
        .route("/import-url", post(import_from_url))
        .route("/:id", get(get_word).put(update_word).delete(delete_word))
}

#[derive(Debug, Deserialize)]
struct ListWordsQuery {
    limit: Option<usize>,
    offset: Option<usize>,
    search: Option<String>,
}

impl ListWordsQuery {
    fn limit(&self) -> usize {
        self.limit.unwrap_or(20)
    }

    fn offset(&self) -> usize {
        self.offset.unwrap_or(0)
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ListWordsResponse {
    items: Vec<Word>,
    total: u64,
    limit: usize,
    offset: usize,
}

async fn list_words(
    Query(query): Query<ListWordsQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = query.limit();
    if limit == 0 || limit > 100 {
        return Err(AppError::bad_request(
            "WORDS_INVALID_LIMIT",
            "limit must be between 1 and 100",
        ));
    }

    let offset = query.offset();

    // B15: search support
    if let Some(ref search) = query.search {
        if !search.trim().is_empty() {
            let (items, total) = state.store().search_words(search, limit, offset)?;
            return Ok(ok(ListWordsResponse {
                items,
                total,
                limit,
                offset,
            }));
        }
    }

    let total = state.store().count_words()?;
    let items = state.store().list_words(limit, offset)?;
    Ok(ok(ListWordsResponse {
        items,
        total,
        limit,
        offset,
    }))
}

// B17: Count all words
async fn count_words(
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let total = state.store().count_words()?;
    Ok(ok(serde_json::json!({"total": total})))
}

// B14: Delete word
async fn delete_word(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let _ = state
        .store()
        .get_word(&id)?
        .ok_or_else(|| AppError::not_found("Word not found"))?;
    state.store().delete_word(&id)?;
    Ok(ok(serde_json::json!({"deleted": true, "id": id})))
}

async fn get_word(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let word = state
        .store()
        .get_word(&id)?
        .ok_or_else(|| AppError::not_found("Word not found"))?;
    Ok(ok(word))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpsertWordRequest {
    id: Option<String>,
    text: String,
    meaning: String,
    pronunciation: Option<String>,
    part_of_speech: Option<String>,
    difficulty: Option<f64>,
    examples: Option<Vec<String>>,
    tags: Option<Vec<String>>,
}

async fn create_word(
    State(state): State<AppState>,
    Json(req): Json<UpsertWordRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if req.text.trim().is_empty() || req.meaning.trim().is_empty() {
        return Err(AppError::bad_request(
            "WORDS_INVALID_PAYLOAD",
            "text and meaning are required",
        ));
    }

    let word = Word {
        id: req.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        text: req.text.trim().to_string(),
        meaning: req.meaning.trim().to_string(),
        pronunciation: req.pronunciation,
        part_of_speech: req.part_of_speech,
        difficulty: req.difficulty.unwrap_or(0.5).clamp(0.0, 1.0),
        examples: req.examples.unwrap_or_default(),
        tags: req.tags.unwrap_or_default(),
        embedding: None,
        created_at: Utc::now(),
    };

    state.store().upsert_word(&word)?;
    Ok(created(word))
}

async fn update_word(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<UpsertWordRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let existing = state
        .store()
        .get_word(&id)?
        .ok_or_else(|| AppError::not_found("Word not found"))?;

    let word = Word {
        id: existing.id,
        text: if req.text.trim().is_empty() {
            existing.text
        } else {
            req.text.trim().to_string()
        },
        meaning: if req.meaning.trim().is_empty() {
            existing.meaning
        } else {
            req.meaning.trim().to_string()
        },
        pronunciation: req.pronunciation.or(existing.pronunciation),
        part_of_speech: req.part_of_speech.or(existing.part_of_speech),
        difficulty: req
            .difficulty
            .unwrap_or(existing.difficulty)
            .clamp(0.0, 1.0),
        examples: req.examples.unwrap_or(existing.examples),
        tags: req.tags.unwrap_or(existing.tags),
        embedding: existing.embedding,
        created_at: existing.created_at,
    };

    state.store().upsert_word(&word)?;
    Ok(ok(word))
}

#[derive(Debug, Deserialize)]
struct BatchCreateWordsRequest {
    words: Vec<UpsertWordRequest>,
}

async fn batch_create_words(
    State(state): State<AppState>,
    Json(req): Json<BatchCreateWordsRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if req.words.len() > 500 {
        return Err(AppError::bad_request(
            "BATCH_TOO_LARGE",
            "batch_create_words accepts at most 500 words",
        ));
    }
    let mut created_words = Vec::new();
    let mut skipped_indices = Vec::new();

    for (i, item) in req.words.into_iter().enumerate() {
        if item.text.trim().is_empty() || item.meaning.trim().is_empty() {
            skipped_indices.push(i);
            continue;
        }
        let word = Word {
            id: item.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            text: item.text.trim().to_string(),
            meaning: item.meaning.trim().to_string(),
            pronunciation: item.pronunciation,
            part_of_speech: item.part_of_speech,
            difficulty: item.difficulty.unwrap_or(0.5).clamp(0.0, 1.0),
            examples: item.examples.unwrap_or_default(),
            tags: item.tags.unwrap_or_default(),
            embedding: None,
            created_at: Utc::now(),
        };
        state.store().upsert_word(&word)?;
        created_words.push(word);
    }

    Ok(created(serde_json::json!({
        "count": created_words.len(),
        "skipped": skipped_indices,
        "items": created_words
    })))
}

// B30: Import words from URL
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportUrlRequest {
    url: String,
}

async fn import_from_url(
    State(state): State<AppState>,
    Json(req): Json<ImportUrlRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let content = reqwest::get(&req.url)
        .await
        .map_err(|e| AppError::bad_request("IMPORT_FETCH_FAILED", &format!("Failed to fetch URL: {e}")))?
        .text()
        .await
        .map_err(|e| AppError::bad_request("IMPORT_READ_FAILED", &format!("Failed to read content: {e}")))?;

    // Parse lines as "word\tmeaning" or "word - meaning"
    let mut imported = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let (text, meaning) = if line.contains('\t') {
            let parts: Vec<&str> = line.splitn(2, '\t').collect();
            (parts[0].trim().to_string(), parts.get(1).unwrap_or(&"").trim().to_string())
        } else if line.contains(" - ") {
            let parts: Vec<&str> = line.splitn(2, " - ").collect();
            (parts[0].trim().to_string(), parts.get(1).unwrap_or(&"").trim().to_string())
        } else {
            (line.to_string(), String::new())
        };

        if text.is_empty() {
            continue;
        }

        let word = Word {
            id: uuid::Uuid::new_v4().to_string(),
            text,
            meaning: if meaning.is_empty() { "imported".to_string() } else { meaning },
            pronunciation: None,
            part_of_speech: None,
            difficulty: 0.5,
            examples: Vec::new(),
            tags: vec!["imported".to_string()],
            embedding: None,
            created_at: Utc::now(),
        };
        state.store().upsert_word(&word)?;
        imported.push(word);
    }

    Ok(created(serde_json::json!({
        "imported": imported.len(),
        "items": imported,
    })))
}
