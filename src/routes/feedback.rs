use axum::extract::{DefaultBodyLimit, State};
use axum::routing::post;
use axum::Router;
use chrono::Utc;
use serde::Deserialize;

use crate::auth::AuthUser;
use crate::extractors::JsonBody;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::operations::feedback::{FeedbackItem, NewAttachment};

const MAX_FEEDBACK_BODY_CHARS: usize = 5_000;
const MAX_FEEDBACK_CATEGORY_CHARS: usize = 64;
const MAX_FEEDBACK_ROUTE_CHARS: usize = 200;
const MAX_FEEDBACK_ATTACHMENTS: usize = 10;
const MAX_FEEDBACK_ATTACHMENT_NAME_CHARS: usize = 200;
const MAX_FEEDBACK_ATTACHMENT_URL_CHARS: usize = 2_000;
const MAX_FEEDBACK_DEVICE_PROFILE_BYTES: usize = 8 * 1024;
const MAX_FEEDBACK_ANSWER_SNAPSHOT_BYTES: usize = 8 * 1024;

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
    /// m022:可选设备指纹快照 JSON。客户端可填 platform / appVersion / osName / browser / locale 等。
    #[serde(default)]
    device_profile: Option<serde_json::Value>,
    /// m022:可选答题上下文快照 JSON。客户端可附最近 N 步答题事件、当前题目 ID 等任意 shape。
    #[serde(default)]
    answer_snapshot: Option<serde_json::Value>,
    /// m030:可选 CSAT 评分（1-5）与评语。
    #[serde(default)]
    csat_score: Option<i64>,
    #[serde(default)]
    csat_comment: Option<String>,
    /// m030:可选附件列表（name/url/kind）。
    #[serde(default)]
    attachments: Option<Vec<NewAttachment>>,
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

    if let Some(score) = req.csat_score {
        if !(1..=5).contains(&score) {
            return Err(AppError::bad_request("INVALID_FEEDBACK", "CSAT 评分须在 1-5 之间"));
        }
    }

    let attachments = req.attachments.unwrap_or_default();
    if attachments.len() > MAX_FEEDBACK_ATTACHMENTS {
        return Err(AppError::bad_request("INVALID_FEEDBACK", "附件数量过多"));
    }
    for attachment in &attachments {
        if attachment.name.chars().count() > MAX_FEEDBACK_ATTACHMENT_NAME_CHARS {
            return Err(AppError::bad_request("INVALID_FEEDBACK", "附件名称过长"));
        }
        if attachment.url.chars().count() > MAX_FEEDBACK_ATTACHMENT_URL_CHARS {
            return Err(AppError::bad_request("INVALID_FEEDBACK", "附件链接过长"));
        }
    }

    check_json_size(
        &req.device_profile,
        MAX_FEEDBACK_DEVICE_PROFILE_BYTES,
        "设备指纹快照过大",
    )?;
    check_json_size(
        &req.answer_snapshot,
        MAX_FEEDBACK_ANSWER_SNAPSHOT_BYTES,
        "答题上下文快照过大",
    )?;

    let item = FeedbackItem {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: auth.user_id,
        category,
        body: body.to_string(),
        route,
        created_at: Utc::now(),
        priority: "normal".to_string(),
        status: "open".to_string(),
        assignee_admin_id: None,
        resolved_at: None,
        resolution: None,
        device_profile: req.device_profile,
        answer_snapshot: req.answer_snapshot,
        read_at: None,
        first_response_at: None,
        csat_score: req.csat_score,
        csat_comment: req.csat_comment,
        dedup_count: 0,
        github_issue_url: None,
        user_name: None,
        platform: None,
        app_version: None,
    };
    let response = item.clone();
    state
        .run_store_task("feedback.create", move |store| {
            store.create_feedback(&item, &attachments)
        })
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

fn check_json_size(
    value: &Option<serde_json::Value>,
    max_bytes: usize,
    too_large_message: &str,
) -> Result<(), AppError> {
    if let Some(value) = value {
        if serde_json::to_vec(value).map(|v| v.len()).unwrap_or(0) > max_bytes {
            return Err(AppError::bad_request("INVALID_FEEDBACK", too_large_message));
        }
    }
    Ok(())
}
