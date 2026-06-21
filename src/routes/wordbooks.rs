use axum::extract::{Path, Query, State};
use axum::routing::{delete, get, post};
use axum::Router;

use crate::extractors::JsonBody;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::constants::MAX_PAGE_NUMBER;
use crate::response::{created, ok, paginated, AppError};
use crate::routes::words::WordPublic;
use crate::state::AppState;
use crate::store::operations::wordbooks::{Wordbook, WordbookType};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/system", get(list_system_wordbooks))
        .route("/user", get(list_user_wordbooks))
        .route("/progress", get(list_wordbook_progress))
        .route("/", post(create_wordbook))
        .route("/:id/words", get(list_wordbook_words).post(add_words))
        .route("/:id/words/:word_id", delete(remove_word))
}

async fn list_system_wordbooks(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let books = state
        .run_store_task("wordbooks.list_system", |store| {
            store.list_system_wordbooks()
        })
        .await??;
    Ok(ok(books))
}

async fn list_user_wordbooks(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id.clone();
    let books = state
        .run_store_task("wordbooks.list_user", move |store| {
            store.list_user_wordbooks(&user_id)
        })
        .await??;
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
    JsonBody(req): JsonBody<CreateWordbookRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if req.name.trim().is_empty() {
        return Err(AppError::bad_request(
            "WORDBOOK_INVALID_NAME",
            "名称不能为空",
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

    let book_for_save = book.clone();
    state
        .run_store_task("wordbooks.create", move |store| {
            store.upsert_wordbook(&book_for_save)
        })
        .await??;
    Ok(created(book))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListWordbookWordsQuery {
    page: Option<u64>,
    per_page: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WordbookProgressQuery {
    wordbook_ids: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WordbookProgressResponse {
    wordbook_id: String,
    word_count: u64,
    studied_words: u64,
    mastered_words: u64,
    reviewing_words: u64,
    forgotten_words: u64,
    progress: f64,
    mastery_progress: f64,
    last_studied_at: Option<chrono::DateTime<Utc>>,
}

fn parse_wordbook_ids(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

async fn list_wordbook_progress(
    auth: AuthUser,
    Query(q): Query<WordbookProgressQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let requested_ids = parse_wordbook_ids(q.wordbook_ids.as_deref());
    if requested_ids.len() > state.config().limits.max_batch_size {
        return Err(AppError::bad_request(
            "WORDBOOK_TOO_MANY_IDS",
            &format!("词书数量上限为{}", state.config().limits.max_batch_size),
        ));
    }
    let user_id = auth.user_id.clone();
    let progress = state
        .run_store_task("wordbooks.progress", move |store| -> Result<_, AppError> {
            let mut books = Vec::new();
            if requested_ids.is_empty() {
                books.extend(store.list_system_wordbooks()?);
                books.extend(store.list_user_wordbooks(&user_id)?);
            } else {
                let mut seen = std::collections::HashSet::new();
                for id in requested_ids {
                    if !seen.insert(id.clone()) {
                        continue;
                    }
                    let book = store
                        .get_wordbook(&id)?
                        .ok_or_else(|| AppError::not_found("词书不存在"))?;
                    if book.user_id.is_some() && book.user_id.as_deref() != Some(&user_id) {
                        return Err(AppError::forbidden("您没有该词书的访问权限"));
                    }
                    books.push(book);
                }
            }

            let mut out = Vec::with_capacity(books.len());
            for book in books {
                let stats = store.get_wordbook_progress_stats(&user_id, &book.id)?;
                let word_count = stats.word_count.max(book.word_count);
                let progress = if word_count > 0 {
                    stats.studied_words as f64 / word_count as f64
                } else {
                    0.0
                };
                let mastery_progress = if word_count > 0 {
                    stats.mastered_words as f64 / word_count as f64
                } else {
                    0.0
                };
                out.push(WordbookProgressResponse {
                    wordbook_id: stats.wordbook_id,
                    word_count,
                    studied_words: stats.studied_words,
                    mastered_words: stats.mastered_words,
                    reviewing_words: stats.reviewing_words,
                    forgotten_words: stats.forgotten_words,
                    progress,
                    mastery_progress,
                    last_studied_at: stats.last_studied_at,
                });
            }
            Ok(out)
        })
        .await??;
    Ok(ok(progress))
}

async fn list_wordbook_words(
    auth: AuthUser,
    Path(id): Path<String>,
    Query(q): Query<ListWordbookWordsQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // 上界封顶防 (page-1)*per_page 在 release(无 overflow-checks)下溢出回绕成乱序 offset。
    let page = q.page.unwrap_or(1).clamp(1, MAX_PAGE_NUMBER);
    let per_page = q
        .per_page
        .unwrap_or(state.config().pagination.default_page_size)
        .clamp(1, state.config().pagination.max_page_size);
    let limit = per_page as usize;
    let offset = ((page - 1) * per_page) as usize;
    let user_id = auth.user_id.clone();
    let id_for_lookup = id.clone();
    let (total, word_ids, words_by_id) = state
        .run_store_task(
            "wordbooks.list_words",
            move |store| -> Result<_, AppError> {
                let book = store
                    .get_wordbook(&id_for_lookup)?
                    .ok_or_else(|| AppError::not_found("词书不存在"))?;
                if book.user_id.is_some() && book.user_id.as_deref() != Some(&user_id) {
                    return Err(AppError::forbidden("您没有该词书的操作权限"));
                }

                // count + list 收进同一事务快照，避免并发增删词导致 total 与列表不一致。
                let (total, word_ids) =
                    store.count_and_list_wordbook_words(&id_for_lookup, limit, offset)?;
                let words_by_id = store.get_words_by_ids(&word_ids)?;
                Ok((total, word_ids, words_by_id))
            },
        )
        .await??;
    let items: Vec<WordPublic> = word_ids
        .iter()
        .filter_map(|wid| words_by_id.get(wid).map(WordPublic::from))
        .collect();

    Ok(paginated(items, total, page, per_page))
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
    JsonBody(req): JsonBody<AddWordsRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if req.word_ids.len() > state.config().limits.max_batch_size {
        return Err(AppError::bad_request(
            "WORDBOOK_TOO_MANY_WORDS",
            &format!(
                "单次添加单词数量不能超过{}",
                state.config().limits.max_batch_size
            ),
        ));
    }
    let user_id = auth.user_id.clone();
    let id_for_update = id.clone();
    let word_ids = req.word_ids;
    let added = state
        .run_store_task("wordbooks.add_words", move |store| -> Result<_, AppError> {
            let book = store
                .get_wordbook(&id_for_update)?
                .ok_or_else(|| AppError::not_found("词书不存在"))?;
            if book.user_id.is_none() {
                return Err(AppError::forbidden("无法修改系统词书"));
            }
            if book.user_id.as_deref() != Some(&user_id) {
                return Err(AppError::forbidden("您没有该词书的操作权限"));
            }

            // 校验 word_id 真实存在,避免幻影 id 灌入 wordbook_words 虚增 word_count
            //（wordbook_words 无指向 words 的外键,DB 不会拒绝不存在的词）。
            let existing = store.get_words_by_ids(&word_ids)?;
            let mut added = 0usize;
            for word_id in &word_ids {
                if !existing.contains_key(word_id) {
                    continue;
                }
                if store.add_word_to_wordbook(&id_for_update, word_id)? {
                    added += 1;
                }
            }
            Ok(added)
        })
        .await??;

    Ok(ok(serde_json::json!({"added": added})))
}

async fn remove_word(
    auth: AuthUser,
    Path((id, word_id)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id.clone();
    let id_for_update = id.clone();
    let word_id_for_update = word_id.clone();
    let removed = state
        .run_store_task(
            "wordbooks.remove_word",
            move |store| -> Result<_, AppError> {
                let book = store
                    .get_wordbook(&id_for_update)?
                    .ok_or_else(|| AppError::not_found("词书不存在"))?;
                if book.user_id.is_none() {
                    return Err(AppError::forbidden("无法修改系统词书"));
                }
                if book.user_id.as_deref() != Some(&user_id) {
                    return Err(AppError::forbidden("您没有该词书的操作权限"));
                }
                store
                    .remove_word_from_wordbook(&id_for_update, &word_id_for_update)
                    .map_err(AppError::from)
            },
        )
        .await??;
    Ok(ok(serde_json::json!({"removed": removed})))
}
