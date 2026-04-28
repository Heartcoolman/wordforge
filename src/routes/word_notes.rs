use axum::extract::{Path, Query, State};
use axum::routing::{get, put};
use axum::Router;
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::extractors::JsonBody;
use crate::response::{created, ok, AppError};
use crate::state::AppState;
use crate::store::operations::word_interactions::WordNote;

const MAX_NOTE_CONTENT_LEN: usize = 5000;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_notes).post(create_note))
        .route("/:id", put(update_note).delete(delete_note))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListNotesQuery {
    word_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateNoteRequest {
    word_id: String,
    content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateNoteRequest {
    content: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WordNotePublic {
    id: String,
    word_id: String,
    content: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<WordNote> for WordNotePublic {
    fn from(note: WordNote) -> Self {
        Self {
            id: note.id,
            word_id: note.word_id,
            content: note.content,
            created_at: note.created_at,
            updated_at: note.updated_at,
        }
    }
}

fn clean_note_content(content: &str) -> Result<String, AppError> {
    let cleaned = content.trim();
    if cleaned.is_empty() {
        return Err(AppError::bad_request(
            "WORD_NOTE_EMPTY_CONTENT",
            "笔记内容不能为空",
        ));
    }
    if cleaned.chars().count() > MAX_NOTE_CONTENT_LEN {
        return Err(AppError::bad_request(
            "WORD_NOTE_TOO_LONG",
            "笔记内容不能超过5000个字符",
        ));
    }
    Ok(cleaned.to_string())
}

async fn list_notes(
    auth: AuthUser,
    Query(q): Query<ListNotesQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id.clone();
    let notes = state
        .run_store_task("word_notes.list", move |store| {
            store
                .list_word_notes(&user_id, q.word_id.as_deref())
                .map(|items| {
                    items
                        .into_iter()
                        .map(WordNotePublic::from)
                        .collect::<Vec<_>>()
                })
        })
        .await??;
    Ok(ok(notes))
}

async fn create_note(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<CreateNoteRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let content = clean_note_content(&req.content)?;
    let user_id = auth.user_id.clone();
    let word_id = req.word_id;
    let note = state
        .run_store_task("word_notes.create", move |store| -> Result<_, AppError> {
            if store.get_word(&word_id)?.is_none() {
                return Err(AppError::not_found("单词不存在"));
            }
            Ok(store.create_word_note(&user_id, &word_id, &content)?)
        })
        .await??;
    Ok(created(WordNotePublic::from(note)))
}

async fn update_note(
    auth: AuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<UpdateNoteRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let content = clean_note_content(&req.content)?;
    let user_id = auth.user_id.clone();
    let note = state
        .run_store_task("word_notes.update", move |store| {
            store.update_word_note(&user_id, &id, &content)
        })
        .await??
        .ok_or_else(|| AppError::not_found("笔记不存在"))?;
    Ok(ok(WordNotePublic::from(note)))
}

async fn delete_note(
    auth: AuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id.clone();
    let deleted = state
        .run_store_task("word_notes.delete", move |store| {
            store.delete_word_note(&user_id, &id)
        })
        .await??;
    if !deleted {
        return Err(AppError::not_found("笔记不存在"));
    }
    Ok(ok(serde_json::json!({ "deleted": true })))
}
