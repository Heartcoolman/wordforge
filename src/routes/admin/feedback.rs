use axum::extract::{Path, Query, State};
use axum::routing::{get, patch};
use axum::{Json, Router};
use serde::Deserialize;

use crate::auth::AdminAuthUser;
use crate::response::AppError;
use crate::state::AppState;
use crate::store::operations::feedback::UpdateFeedbackRequest;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_feedback))
        .route("/:id", patch(update_feedback))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListFeedbackQuery {
    page: Option<u64>,
    per_page: Option<u64>,
    category: Option<String>,
    status: Option<String>,
}

async fn list_feedback(
    _admin: AdminAuthUser,
    Query(q): Query<ListFeedbackQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let page = q.page.unwrap_or(1).clamp(1, u64::MAX);
    let per_page = q
        .per_page
        .unwrap_or(state.config().pagination.default_page_size)
        .clamp(1, state.config().pagination.max_page_size);
    let category = q.category.clone();
    let status = q.status.clone();
    let (items, total) = state
        .run_store_task("admin.feedback.list", move |store| {
            store.list_feedback_filtered(
                page,
                per_page,
                category.as_deref(),
                status.as_deref(),
            )
        })
        .await??;

    Ok(crate::response::paginated(items, total, page, per_page))
}

async fn update_feedback(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<UpdateFeedbackRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let item = state
        .run_store_task("admin.feedback.update", move |store| {
            store.update_feedback(&id, &req)
        })
        .await??;

    match item {
        Some(f) => Ok(axum::Json(f).into_response()),
        None => Err(AppError::not_found("反馈记录不存在")),
    }
}
