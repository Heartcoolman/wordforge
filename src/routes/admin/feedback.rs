use axum::extract::{Query, State};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use crate::auth::AdminAuthUser;
use crate::response::AppError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list_feedback))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListFeedbackQuery {
    page: Option<u64>,
    per_page: Option<u64>,
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
    let (items, total) = state
        .run_store_task("admin.feedback.list", move |store| {
            store.list_feedback(page, per_page)
        })
        .await??;

    Ok(crate::response::paginated(items, total, page, per_page))
}
