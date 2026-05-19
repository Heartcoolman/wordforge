use axum::extract::{DefaultBodyLimit, State};
use axum::routing::post;
use axum::Router;
use chrono::Utc;
use serde::Deserialize;

use crate::auth::AuthUser;
use crate::extractors::JsonBody;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::operations::feedback::FeedbackItem;

const MAX_FEEDBACK_BODY_CHARS: usize = 5_000;
const MAX_FEEDBACK_CATEGORY_CHARS: usize = 64;
const MAX_FEEDBACK_ROUTE_CHARS: usize = 200;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_feedback))
        .layer(DefaultBodyLimit::max(32 * 1024))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateFeedbackRequest {
    category: Option<String>,
    body: String,
    route: Option<String>,
}

async fn create_feedback(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<CreateFeedbackRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let body = req.body.trim();
    if body.is_empty() {
        return Err(AppError::bad_request(
            "INVALID_FEEDBACK",
            "反馈内容不能为空",
        ));
    }
    if body.chars().count() > MAX_FEEDBACK_BODY_CHARS {
        return Err(AppError::bad_request("INVALID_FEEDBACK", "反馈内容过长"));
    }

    let category = trim_optional(req.category, MAX_FEEDBACK_CATEGORY_CHARS, "反馈类型过长")?;
    let route = trim_optional(req.route, MAX_FEEDBACK_ROUTE_CHARS, "页面路径过长")?;
    let item = FeedbackItem {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: auth.user_id,
        category,
        body: body.to_string(),
        route,
        created_at: Utc::now(),
    };
    let response = item.clone();
    state
        .run_store_task("feedback.create", move |store| store.create_feedback(&item))
        .await??;

    Ok(ok(response))
}

fn trim_optional(
    value: Option<String>,
    max_chars: usize,
    too_long_message: &str,
) -> Result<Option<String>, AppError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > max_chars {
        return Err(AppError::bad_request("INVALID_FEEDBACK", too_long_message));
    }
    Ok(Some(trimmed.to_string()))
}
