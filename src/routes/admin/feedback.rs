use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use crate::auth::AdminAuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::operations::feedback::{UpdateAnnouncementRequest, UpdateFeedbackRequest};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_feedback))
        .route("/stats", get(feedback_stats))
        .route("/mark-all-read", post(mark_all_read))
        .route("/export.csv", get(export_csv))
        // m036:公告 / FAQ CRUD（静态段，置于 /:id 动态段之前）
        .route(
            "/announcements",
            get(list_announcements).post(create_announcement),
        )
        .route(
            "/announcements/:id",
            patch(update_announcement).delete(delete_announcement),
        )
        .route("/:id", get(get_detail).patch(update_feedback))
        .route("/:id/replies", post(add_reply))
        // m036:回复草稿持久化（每工单一份，存为草稿 / 取回）
        .route("/:id/draft", get(get_reply_draft).post(save_reply_draft))
        .route("/:id/assign", post(assign_feedback))
        .route("/:id/resolve", post(resolve_feedback))
        .route("/:id/merge", post(merge_feedback))
        .route("/:id/github-issue", post(create_github_issue))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListFeedbackQuery {
    page: Option<u64>,
    per_page: Option<u64>,
    category: Option<String>,
    status: Option<String>,
    #[serde(default)]
    unread: bool,
    #[serde(default)]
    assigned: bool,
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
    let (unread, assigned) = (q.unread, q.assigned);
    let (items, total) = state
        .run_store_task("admin.feedback.list", move |store| {
            store.list_feedback_filtered(
                page,
                per_page,
                category.as_deref(),
                status.as_deref(),
                unread,
                assigned,
            )
        })
        .await??;

    Ok(crate::response::paginated(items, total, page, per_page))
}

async fn feedback_stats(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let stats = state
        .run_store_task("admin.feedback.stats", move |store| store.feedback_stats())
        .await??;
    Ok(ok(stats))
}

async fn get_detail(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let read_id = id.clone();
    let detail = state
        .run_store_task("admin.feedback.detail", move |store| {
            store.get_feedback_detail(&read_id)
        })
        .await??;
    match detail {
        Some(d) => {
            // 打开详情即标记该工单已读（首次置 read_at，幂等）
            let mark_id = id.clone();
            let _ = state
                .run_store_task("admin.feedback.mark_read", move |store| {
                    store.set_feedback_read(&mark_id)
                })
                .await;
            Ok(ok(d).into_response())
        }
        None => Err(AppError::not_found("反馈记录不存在")),
    }
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
        // 保持 HEAD 既有裸 JSON 返回(集成测试直接读 status 字段);其余新端点用 ok() 信封。
        Some(f) => Ok(axum::Json(f).into_response()),
        None => Err(AppError::not_found("反馈记录不存在")),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddReplyRequest {
    body: String,
    #[serde(default)]
    push_inapp: bool,
    #[serde(default)]
    cc_email: bool,
}

async fn add_reply(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<AddReplyRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let body = req.body.trim().to_string();
    if body.is_empty() {
        return Err(AppError::bad_request("INVALID_REPLY", "回复内容不能为空"));
    }
    let author_id = admin.admin_id.clone();
    let (push_inapp, cc_email) = (req.push_inapp, req.cc_email);
    let reply_id = id.clone();
    let reply_body = body.clone();
    let reply = state
        .run_store_task("admin.feedback.reply", move |store| {
            store.add_reply(
                &reply_id,
                &reply_body,
                push_inapp,
                cc_email,
                Some(author_id.as_str()),
            )
        })
        .await??;

    let reply = match reply {
        Some(r) => r,
        None => return Err(AppError::not_found("反馈记录不存在")),
    };

    // pushInapp 时尽力给该工单的提交用户发应用内通知，失败仅 warn 不阻断
    if push_inapp {
        let detail_id = id.clone();
        if let Ok(Ok(Some(detail))) = state
            .run_store_task("admin.feedback.reply.lookup_user", move |store| {
                store.get_feedback_detail(&detail_id)
            })
            .await
        {
            let user_id = detail.item.user_id.clone();
            let notif_body = body.clone();
            let result = state
                .run_store_task("admin.feedback.reply.notify", move |store| {
                    let entry = (
                        user_id.clone(),
                        String::new(),
                        json!({
                            "id": uuid::Uuid::new_v4().to_string(),
                            "userId": user_id,
                            "type": "system",
                            "title": "你的反馈有了新回复",
                            "message": notif_body,
                            "read": false,
                            "createdAt": chrono::Utc::now().to_rfc3339(),
                        }),
                    );
                    store.batch_create_notifications(&[entry])
                })
                .await;
            if let Err(e) = result {
                tracing::warn!(error=%e, feedback_id=%id, "反馈回复应用内通知发送失败（不阻断）");
            }
        }
    }

    Ok(ok(reply))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssignRequest {
    assignee_admin_id: Option<String>,
}

async fn assign_feedback(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<AssignRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let assignee = req.assignee_admin_id.clone();
    let item = state
        .run_store_task("admin.feedback.assign", move |store| {
            store.assign_feedback(&id, assignee.as_deref())
        })
        .await??;
    match item {
        Some(f) => Ok(ok(f)),
        None => Err(AppError::not_found("反馈记录不存在")),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResolveRequest {
    resolution: Option<String>,
}

async fn resolve_feedback(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<ResolveRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let update = UpdateFeedbackRequest {
        priority: None,
        status: Some("resolved".to_string()),
        assignee_admin_id: None,
        resolution: req.resolution.clone(),
    };
    let item = state
        .run_store_task("admin.feedback.resolve", move |store| {
            store.update_feedback(&id, &update)
        })
        .await??;
    match item {
        Some(f) => Ok(ok(f)),
        None => Err(AppError::not_found("反馈记录不存在")),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MergeRequest {
    target_id: String,
}

async fn merge_feedback(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<MergeRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let target_id = req.target_id.clone();
    let merged = state
        .run_store_task("admin.feedback.merge", move |store| {
            store.merge_feedback(&id, &target_id)
        })
        .await??;
    if merged {
        Ok(ok(json!({ "merged": true })))
    } else {
        Err(AppError::not_found("源或目标反馈记录不存在"))
    }
}

async fn mark_all_read(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let updated = state
        .run_store_task("admin.feedback.mark_all_read", move |store| {
            store.mark_all_feedback_read()
        })
        .await??;
    Ok(ok(json!({ "updated": updated })))
}

// ============================================================================
// m036:公告 / FAQ CRUD + 回复草稿
// ============================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListAnnouncementsQuery {
    /// 'announcement' | 'faq'；缺省返回全部。
    kind: Option<String>,
    /// 仅看已发布 / 仅看草稿。
    published: Option<bool>,
}

/// GET /announcements?kind=&published= —— 列出公告 / FAQ（created_at DESC）。
async fn list_announcements(
    _admin: AdminAuthUser,
    Query(q): Query<ListAnnouncementsQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let kind = normalize_announcement_kind(q.kind.as_deref())?;
    let published = q.published;
    let items = state
        .run_store_task("admin.feedback.announcements.list", move |store| {
            store.list_announcements(kind.as_deref(), published)
        })
        .await??;
    Ok(ok(json!({ "data": items })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateAnnouncementRequest {
    title: String,
    body: String,
    /// 'announcement' | 'faq'；缺省 'announcement'。
    #[serde(default)]
    kind: Option<String>,
    /// 缺省 false（存为草稿）。
    #[serde(default)]
    published: bool,
}

/// POST /announcements —— 新建公告 / FAQ（默认存为草稿）。
async fn create_announcement(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    Json(req): Json<CreateAnnouncementRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let title = req.title.trim().to_string();
    let body = req.body.trim().to_string();
    if title.is_empty() {
        return Err(AppError::bad_request("INVALID_TITLE", "标题不能为空"));
    }
    if body.is_empty() {
        return Err(AppError::bad_request("INVALID_BODY", "正文不能为空"));
    }
    let kind = normalize_announcement_kind(req.kind.as_deref())?
        .unwrap_or_else(|| "announcement".to_string());
    let published = req.published;
    let author_id = admin.admin_id.clone();
    let item = state
        .run_store_task("admin.feedback.announcements.create", move |store| {
            store.create_announcement(&title, &body, &kind, published, Some(author_id.as_str()))
        })
        .await??;
    Ok(crate::response::created(item))
}

/// PATCH /announcements/:id —— 部分更新（含发布/撤回草稿）。
async fn update_announcement(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<UpdateAnnouncementRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if let Some(ref t) = req.title {
        if t.trim().is_empty() {
            return Err(AppError::bad_request("INVALID_TITLE", "标题不能为空"));
        }
    }
    if let Some(ref b) = req.body {
        if b.trim().is_empty() {
            return Err(AppError::bad_request("INVALID_BODY", "正文不能为空"));
        }
    }
    let item = state
        .run_store_task("admin.feedback.announcements.update", move |store| {
            store.update_announcement(&id, &req)
        })
        .await??;
    match item {
        Some(a) => Ok(ok(a)),
        None => Err(AppError::not_found("公告 / FAQ 不存在")),
    }
}

/// DELETE /announcements/:id
async fn delete_announcement(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let deleted = state
        .run_store_task("admin.feedback.announcements.delete", move |store| {
            store.delete_announcement(&id)
        })
        .await??;
    if deleted {
        Ok(ok(json!({ "deleted": true })))
    } else {
        Err(AppError::not_found("公告 / FAQ 不存在"))
    }
}

/// 校验并归一化 kind；None 透传 None，非法值报 400。
fn normalize_announcement_kind(raw: Option<&str>) -> Result<Option<String>, AppError> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(k @ ("announcement" | "faq")) => Ok(Some(k.to_string())),
        Some(_) => Err(AppError::bad_request(
            "INVALID_KIND",
            "kind 必须是 'announcement' 或 'faq'",
        )),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveDraftRequest {
    body: String,
    #[serde(default)]
    push_inapp: bool,
    #[serde(default)]
    cc_email: bool,
}

/// POST /:id/draft —— 保存（覆盖）该工单的回复草稿。
async fn save_reply_draft(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<SaveDraftRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let body = req.body;
    let (push_inapp, cc_email) = (req.push_inapp, req.cc_email);
    let author_id = admin.admin_id.clone();
    let draft = state
        .run_store_task("admin.feedback.draft.save", move |store| {
            store.save_reply_draft(&id, &body, push_inapp, cc_email, Some(author_id.as_str()))
        })
        .await??;
    match draft {
        Some(d) => Ok(ok(d)),
        None => Err(AppError::not_found("反馈记录不存在")),
    }
}

/// GET /:id/draft —— 取该工单的回复草稿（无则 draft=null）。
async fn get_reply_draft(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let draft = state
        .run_store_task("admin.feedback.draft.get", move |store| {
            store.get_reply_draft(&id)
        })
        .await??;
    Ok(ok(json!({ "draft": draft })))
}

/// POST /:id/github-issue —— 把工单转为 GitHub Issue。
/// 读 env GITHUB_TOKEN + FEEDBACK_GITHUB_REPO(owner/repo)；未配置返回 409 GITHUB_NOT_CONFIGURED。
/// reqwest 风格对齐 src/services/updater.rs（User-Agent + Authorization: Bearer）。
async fn create_github_issue(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let token = std::env::var("GITHUB_TOKEN").ok().filter(|s| !s.is_empty());
    let repo = std::env::var("FEEDBACK_GITHUB_REPO")
        .ok()
        .filter(|s| !s.is_empty());
    let (token, repo) = match (token, repo) {
        (Some(t), Some(r)) => (t, r),
        _ => {
            return Err(AppError::conflict(
                "GITHUB_NOT_CONFIGURED",
                "未配置 GITHUB_TOKEN / FEEDBACK_GITHUB_REPO，无法创建 GitHub Issue",
            ))
        }
    };

    // 取工单详情构造 issue 标题/正文
    let detail_id = id.clone();
    let detail = state
        .run_store_task("admin.feedback.github.detail", move |store| {
            store.get_feedback_detail(&detail_id)
        })
        .await??
        .ok_or_else(|| AppError::not_found("反馈记录不存在"))?;
    let item = &detail.item;

    let title = {
        let first_line = item.body.lines().next().unwrap_or("").trim();
        let snippet: String = first_line.chars().take(80).collect();
        let snippet = if snippet.is_empty() {
            "用户反馈".to_string()
        } else {
            snippet
        };
        format!("[反馈] {snippet}")
    };
    let category = item.category.as_deref().unwrap_or("unknown");
    let body = format!(
        "来自 WordForge 反馈中心\n\n- 工单 ID: {}\n- 用户: {}\n- 分类: {}\n- 优先级: {}\n- 提交时间: {}\n\n---\n\n{}",
        item.id,
        item.user_id,
        category,
        item.priority,
        item.created_at.to_rfc3339(),
        item.body,
    );

    let client = reqwest::Client::builder()
        .user_agent("wordforge-feedback")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());
    let api_url = format!("https://api.github.com/repos/{repo}/issues");
    let resp = client
        .post(&api_url)
        .header(header::ACCEPT, "application/vnd.github+json")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .json(&json!({ "title": title, "body": body }))
        .send()
        .await
        .map_err(|e| AppError::service_unavailable("GITHUB_REQUEST_FAILED", &e.to_string()))?;

    let status = resp.status();
    if !status.is_success() {
        let detail = resp.text().await.unwrap_or_default();
        return Err(AppError {
            status: StatusCode::BAD_GATEWAY,
            code: format!("GITHUB_API_{}", status.as_u16()),
            message: format!("GitHub API 返回错误：{detail}"),
            is_operational: true,
        });
    }
    let payload: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::internal(&e.to_string()))?;
    let issue_url = payload
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    let write_url = issue_url.clone();
    let write_id = id.clone();
    state
        .run_store_task("admin.feedback.github.persist", move |store| {
            store.set_github_issue(&write_id, &write_url)
        })
        .await??;

    Ok(ok(json!({ "issueUrl": issue_url })))
}

/// GET /export.csv —— 导出全部反馈为 CSV（列：id,user_id,category,priority,status,created_at,body）。
/// body 去换行避免破坏行结构。
async fn export_csv(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // 拉全量（按 created_at DESC）；per_page 取 u64::MAX 等效全量
    let (items, _total) = state
        .run_store_task("admin.feedback.export", move |store| {
            store.list_feedback_filtered(1, u64::MAX, None, None, false, false)
        })
        .await??;

    let mut csv = String::from("id,user_id,category,priority,status,created_at,body\n");
    for it in &items {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            csv_field(&it.id),
            csv_field(&it.user_id),
            csv_field(it.category.as_deref().unwrap_or("")),
            csv_field(&it.priority),
            csv_field(&it.status),
            csv_field(&it.created_at.to_rfc3339()),
            csv_field(&it.body.replace(['\n', '\r'], " ")),
        ));
    }

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"feedback.csv\"",
        )
        .body(Body::from(csv))
        .map_err(|e| AppError::internal(&e.to_string()))
}

/// CSV 字段转义：①公式注入中和——首字符为电子表格公式触发符时前置单引号；
/// ②含逗号/引号/换行的值用双引号包裹并转义内部双引号。
fn csv_field(s: &str) -> String {
    let neutralized = if s
        .chars()
        .next()
        .is_some_and(|c| matches!(c, '=' | '+' | '-' | '@' | '\t' | '\r'))
    {
        format!("'{s}")
    } else {
        s.to_string()
    };
    if neutralized.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", neutralized.replace('"', "\"\""))
    } else {
        neutralized
    }
}
