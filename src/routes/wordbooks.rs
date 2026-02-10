use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::response::{created, ok, AppError};
use crate::state::AppState;
use crate::store::operations::wordbooks::{Wordbook, WordbookType};
use crate::store::operations::words::Word;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/system", get(list_system_wordbooks))
        .route("/user", get(list_user_wordbooks))
        .route("/", post(create_wordbook))
        .route("/:id/words", get(list_wordbook_words).post(add_words))
        .route("/:id/words/:word_id", delete(remove_word))
}

async fn list_system_wordbooks(
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let books = state.store().list_system_wordbooks()?;
    Ok(ok(books))
}

async fn list_user_wordbooks(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let books = state.store().list_user_wordbooks(&auth.user_id)?;
    Ok(ok(books))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateWordbookRequest {
    name: String,
    description: Option<String>,
}

async fn create_wordbook(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateWordbookRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if req.name.trim().is_empty() {
        return Err(AppError::bad_request(
            "WORDBOOK_INVALID_NAME",
            "Name is required",
        ));
    }

    let book = Wordbook {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.name.trim().to_string(),
        description: req.description.unwrap_or_default(),
        book_type: WordbookType::User,
        user_id: Some(auth.user_id),
        word_count: 0,
        created_at: Utc::now(),
    };

    state.store().upsert_wordbook(&book)?;
    Ok(created(book))
}

#[derive(Debug, Deserialize)]
struct ListWordbookWordsQuery {
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WordbookWordsResponse {
    items: Vec<Word>,
    total: u64,
    limit: usize,
    offset: usize,
}

async fn list_wordbook_words(
    auth: AuthUser,
    Path(id): Path<String>,
    Query(q): Query<ListWordbookWordsQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let book = state
        .store()
        .get_wordbook(&id)?
        .ok_or_else(|| AppError::not_found("Wordbook not found"))?;

    // User wordbooks require ownership; system wordbooks are readable by anyone
    if book.user_id.is_some() && book.user_id.as_deref() != Some(&auth.user_id) {
        return Err(AppError::forbidden("You do not own this wordbook"));
    }

    let limit = q.limit.unwrap_or(20).clamp(1, 100);
    let offset = q.offset.unwrap_or(0);
    let total = state.store().count_wordbook_words(&id)?;
    let word_ids = state.store().list_wordbook_words(&id, limit, offset)?;

    let mut items = Vec::new();
    for wid in &word_ids {
        if let Some(word) = state.store().get_word(wid)? {
            items.push(word);
        }
    }

    Ok(ok(WordbookWordsResponse {
        items,
        total,
        limit,
        offset,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddWordsRequest {
    word_ids: Vec<String>,
}

async fn add_words(
    auth: AuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<AddWordsRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let book = state
        .store()
        .get_wordbook(&id)?
        .ok_or_else(|| AppError::not_found("Wordbook not found"))?;

    // System wordbooks (user_id is None) cannot be modified by regular users
    if book.user_id.is_none() {
        return Err(AppError::forbidden("Cannot modify a system wordbook"));
    }
    if book.user_id.as_deref() != Some(&auth.user_id) {
        return Err(AppError::forbidden("You do not own this wordbook"));
    }

    let mut added = 0usize;
    for word_id in &req.word_ids {
        state.store().add_word_to_wordbook(&id, word_id)?;
        added += 1;
    }

    Ok(ok(serde_json::json!({"added": added})))
}

async fn remove_word(
    auth: AuthUser,
    Path((id, word_id)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let book = state
        .store()
        .get_wordbook(&id)?
        .ok_or_else(|| AppError::not_found("Wordbook not found"))?;

    // System wordbooks (user_id is None) cannot be modified by regular users
    if book.user_id.is_none() {
        return Err(AppError::forbidden("Cannot modify a system wordbook"));
    }
    if book.user_id.as_deref() != Some(&auth.user_id) {
        return Err(AppError::forbidden("You do not own this wordbook"));
    }

    state.store().remove_word_from_wordbook(&id, &word_id)?;
    Ok(ok(serde_json::json!({"removed": true})))
}
