use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};

use crate::auth::AuthUser;
use crate::response::{ok, paginated, AppError};
use crate::routes::words::WordPublic;
use crate::state::AppState;
use crate::store::operations::word_interactions::WordFavorite;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_favorites))
        .route("/status", get(favorite_status))
        .route("/:word_id", post(add_favorite).delete(remove_favorite))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListFavoritesQuery {
    page: Option<u64>,
    per_page: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FavoriteStatusQuery {
    word_ids: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FavoriteItem {
    word_id: String,
    favorited: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    word: WordPublic,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FavoriteStatusItem {
    word_id: String,
    favorited: bool,
    created_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn parse_word_ids(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

async fn list_favorites(
    auth: AuthUser,
    Query(q): Query<ListFavoritesQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let page = q.page.unwrap_or(1).max(1);
    let per_page = q
        .per_page
        .unwrap_or(state.config().pagination.default_page_size)
        .clamp(1, state.config().pagination.max_page_size);
    let limit = per_page as usize;
    let offset = ((page - 1) * per_page) as usize;
    let user_id = auth.user_id.clone();
    // P3#5: 原 handler 接受 page/per_page 却用 ok() 返回扁平数组，参数被静默忽略。
    // 改为 paginated()，与 /api/wordbooks/{id}/words 等列表端点风格一致。
    let (items, total) = state
        .run_store_task("word_favorites.list", move |store| -> Result<_, AppError> {
            let total = store.count_word_favorites(&user_id)?;
            let favorites = store.list_word_favorites(&user_id, limit, offset)?;
            let word_ids: Vec<String> = favorites.iter().map(|fav| fav.word_id.clone()).collect();
            let words = store.get_words_by_ids(&word_ids)?;
            let items = favorites
                .into_iter()
                .filter_map(|fav| {
                    words.get(&fav.word_id).map(|word| FavoriteItem {
                        word_id: fav.word_id,
                        favorited: true,
                        created_at: fav.created_at,
                        word: WordPublic::from(word),
                    })
                })
                .collect::<Vec<_>>();
            Ok((items, total))
        })
        .await??;
    Ok(paginated(items, total, page, per_page))
}

async fn favorite_status(
    auth: AuthUser,
    Query(q): Query<FavoriteStatusQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let word_ids = parse_word_ids(&q.word_ids);
    if word_ids.len() > state.config().limits.max_batch_size {
        return Err(AppError::bad_request(
            "WORD_FAVORITES_TOO_MANY_IDS",
            &format!("批量查询数量上限为{}", state.config().limits.max_batch_size),
        ));
    }
    let user_id = auth.user_id.clone();
    let statuses = state
        .run_store_task("word_favorites.status", move |store| {
            store
                .get_word_favorite_statuses(&user_id, &word_ids)
                .map(|favorites| {
                    word_ids
                        .into_iter()
                        .map(|word_id| {
                            let favorite: Option<&WordFavorite> = favorites.get(&word_id);
                            FavoriteStatusItem {
                                word_id,
                                favorited: favorite.is_some(),
                                created_at: favorite.map(|fav| fav.created_at),
                            }
                        })
                        .collect::<Vec<_>>()
                })
        })
        .await??;
    Ok(ok(statuses))
}

async fn add_favorite(
    auth: AuthUser,
    Path(word_id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id.clone();
    let favorite = state
        .run_store_task("word_favorites.add", move |store| -> Result<_, AppError> {
            if store.get_word(&word_id)?.is_none() {
                return Err(AppError::not_found("单词不存在"));
            }
            Ok(store.upsert_word_favorite(&user_id, &word_id)?)
        })
        .await??;
    Ok(ok(FavoriteStatusItem {
        word_id: favorite.word_id,
        favorited: true,
        created_at: Some(favorite.created_at),
    }))
}

async fn remove_favorite(
    auth: AuthUser,
    Path(word_id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id.clone();
    let word_id_for_delete = word_id.clone();
    let deleted = state
        .run_store_task("word_favorites.delete", move |store| {
            store.delete_word_favorite(&user_id, &word_id_for_delete)
        })
        .await??;
    Ok(ok(serde_json::json!({
        "wordId": word_id,
        "favorited": false,
        "deleted": deleted,
    })))
}
