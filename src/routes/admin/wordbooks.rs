//! admin 词库中心:本地系统+用户词库管理控制台。挂载 /api/admin/wordbooks。
//! 全部 AdminAuthUser 鉴权;统计为跨用户聚合(admin 全局视角)。
//! 远程目录浏览/导入仍由 wordbook_center.rs 提供(/api/admin/wordbook-center/*)。

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::Router;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::AdminAuthUser;
use crate::extractors::JsonBody;
use crate::response::{created, ok, paginated, AppError};
use crate::state::AppState;
use crate::store::operations::admin_wordbooks::{
    WordEntryFilter, WordEntryRow, WordbookListFilter,
};
use crate::store::operations::wordbooks::{Wordbook, WordbookType};
use crate::store::operations::words::Word;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_wordbooks).post(create_wordbook))
        .route(
            "/:id",
            axum::routing::patch(patch_wordbook).delete(delete_wordbook),
        )
        .route("/:id/stats", get(wordbook_stats))
        .route("/:id/words", get(list_words).post(add_word))
        .route("/:id/words/:word_id", axum::routing::delete(remove_word))
        .route("/:id/heatmap", get(wordbook_heatmap))
        .route("/:id/user-distribution", get(user_distribution))
        .route("/:id/history", get(wordbook_history))
        .route("/:id/export", get(export_wordbook))
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn clamp_page(page: Option<u64>) -> u64 {
    page.unwrap_or(1).max(1)
}
fn clamp_per_page(per_page: Option<u64>) -> u64 {
    per_page.unwrap_or(20).clamp(1, 200)
}

/// 写词库审计;失败仅 warn,不影响主流程。
fn write_audit(
    state: &AppState,
    wordbook_id: &str,
    action: &str,
    detail: &str,
    admin_id: Option<&str>,
) {
    if let Err(e) = state
        .store()
        .insert_wordbook_audit(wordbook_id, action, detail, admin_id)
    {
        tracing::warn!(error=%e, action=%action, "写 wordbook audit 失败(不影响主流程)");
    }
}

/// 把 store 的 Wordbook 序列化为前端契约 shape(含 type/userId/wordCount/createdAt)。
fn wordbook_to_json(wb: &Wordbook) -> serde_json::Value {
    serde_json::to_value(wb).unwrap_or(serde_json::Value::Null)
}

// ---------------------------------------------------------------------------
// GET /  列表 + counts
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    #[serde(default)]
    r#type: Option<String>,
    search: Option<String>,
    sort: Option<String>,
    page: Option<u64>,
    per_page: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ListResponse {
    items: Vec<crate::store::operations::admin_wordbooks::WordbookSummaryRow>,
    total: u64,
    page: u64,
    per_page: u64,
    total_pages: u64,
    counts: crate::store::operations::admin_wordbooks::WordbookCounts,
}

async fn list_wordbooks(
    _admin: AdminAuthUser,
    Query(q): Query<ListQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let page = clamp_page(q.page);
    let per_page = clamp_per_page(q.per_page);
    let type_filter = match q.r#type.as_deref() {
        Some("system") => "system",
        Some("user") => "user",
        _ => "all",
    }
    .to_string();
    let sort = match q.sort.as_deref() {
        Some("hottest") => "hottest",
        Some("newest") => "newest",
        _ => "default",
    }
    .to_string();
    let search = q
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let filter = WordbookListFilter {
        type_filter,
        search,
        sort,
        limit: per_page as i64,
        offset: ((page - 1) * per_page) as i64,
    };

    let (items, total, counts) = state
        .run_store_task("admin.wordbooks.list", move |store| {
            store.list_all_wordbooks(&filter)
        })
        .await??;

    let total = total as u64;
    let total_pages = if per_page > 0 {
        total.div_ceil(per_page)
    } else {
        0
    };
    Ok(ok(ListResponse {
        items,
        total,
        page,
        per_page,
        total_pages,
        counts,
    }))
}

// ---------------------------------------------------------------------------
// GET /:id/stats
// ---------------------------------------------------------------------------

async fn wordbook_stats(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let cutoff = Utc::now() - Duration::days(7);
    let id_for_task = id.clone();
    let stats = state
        .run_store_task("admin.wordbooks.stats", move |store| {
            store.get_wordbook_admin_stats(&id_for_task, cutoff)
        })
        .await??;
    Ok(ok(stats))
}

// ---------------------------------------------------------------------------
// GET /:id/words
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WordsQuery {
    page: Option<u64>,
    per_page: Option<u64>,
    search: Option<String>,
    pos: Option<String>,
    sort: Option<String>,
}

async fn list_words(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    Query(q): Query<WordsQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let page = clamp_page(q.page);
    let per_page = clamp_per_page(q.per_page);
    let sort = match q.sort.as_deref() {
        Some("alpha") => "alpha",
        Some("difficulty") => "difficulty",
        _ => "frequency",
    }
    .to_string();
    let filter = WordEntryFilter {
        search: q
            .search
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        pos: q
            .pos
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        sort,
        limit: per_page as i64,
        offset: ((page - 1) * per_page) as i64,
    };
    let id_for_task = id.clone();
    let (rows, total) = state
        .run_store_task("admin.wordbooks.words", move |store| {
            store.list_wordbook_word_entries(&id_for_task, &filter)
        })
        .await??;
    Ok(paginated(rows, total as u64, page, per_page))
}

// ---------------------------------------------------------------------------
// GET /:id/heatmap
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct HeatmapQuery {
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HeatmapResponse {
    wordbook_id: String,
    max_count: i64,
    cells: Vec<crate::store::operations::admin_wordbooks::HeatmapCell>,
}

async fn wordbook_heatmap(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    Query(q): Query<HeatmapQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let limit = q.limit.unwrap_or(600).clamp(1, 2000);
    let id_for_task = id.clone();
    let (max_count, cells) = state
        .run_store_task("admin.wordbooks.heatmap", move |store| {
            store.wordbook_heatmap(&id_for_task, limit)
        })
        .await??;
    Ok(ok(HeatmapResponse {
        wordbook_id: id,
        max_count,
        cells,
    }))
}

// ---------------------------------------------------------------------------
// GET /:id/user-distribution
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DistributionResponse {
    wordbook_id: String,
    total_users: i64,
    buckets: Vec<crate::store::operations::admin_wordbooks::DistributionBucket>,
}

async fn user_distribution(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let id_for_task = id.clone();
    let (total_users, buckets) = state
        .run_store_task("admin.wordbooks.user_distribution", move |store| {
            store.wordbook_user_distribution(&id_for_task)
        })
        .await??;
    Ok(ok(DistributionResponse {
        wordbook_id: id,
        total_users,
        buckets,
    }))
}

// ---------------------------------------------------------------------------
// GET /:id/history
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryQuery {
    page: Option<u64>,
    per_page: Option<u64>,
}

async fn wordbook_history(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    Query(q): Query<HistoryQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let page = clamp_page(q.page);
    let per_page = clamp_per_page(q.per_page);
    let id_for_task = id.clone();
    let (entries, total) = state
        .run_store_task("admin.wordbooks.history", move |store| {
            store.list_wordbook_audit(
                &id_for_task,
                per_page as i64,
                ((page - 1) * per_page) as i64,
            )
        })
        .await??;
    Ok(paginated(entries, total as u64, page, per_page))
}

// ---------------------------------------------------------------------------
// POST /  create
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateWordbookRequest {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    r#type: Option<String>,
}

async fn create_wordbook(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<CreateWordbookRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::bad_request("WB_NAME_REQUIRED", "name 不能为空"));
    }
    let book_type = match req.r#type.as_deref() {
        Some("user") => WordbookType::User,
        _ => WordbookType::System,
    };
    let wb = Wordbook {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.clone(),
        description: req.description.unwrap_or_default(),
        book_type,
        user_id: None,
        word_count: 0,
        created_at: Utc::now(),
    };
    let wb_for_task = wb.clone();
    state
        .run_store_task("admin.wordbooks.create", move |store| {
            store.upsert_wordbook(&wb_for_task)
        })
        .await??;

    write_audit(&state, &wb.id, "create", &name, Some(&admin.admin_id));
    Ok(created(wordbook_to_json(&wb)))
}

// ---------------------------------------------------------------------------
// PATCH /:id  update
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatchWordbookRequest {
    name: Option<String>,
    description: Option<String>,
}

async fn patch_wordbook(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<PatchWordbookRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let name = req.name.map(|s| s.trim().to_string());
    if name.is_none() && req.description.is_none() {
        return Err(AppError::bad_request(
            "WB_PATCH_EMPTY",
            "至少提供 name 或 description",
        ));
    }
    if let Some(n) = name.as_deref() {
        if n.is_empty() {
            return Err(AppError::bad_request("WB_NAME_REQUIRED", "name 不能为空"));
        }
    }
    let id_for_task = id.clone();
    let desc = req.description.clone();
    let name_for_task = name.clone();
    let updated: Wordbook = state
        .run_store_task(
            "admin.wordbooks.patch",
            move |store| -> Result<Wordbook, AppError> {
                let mut wb = store
                    .get_wordbook(&id_for_task)?
                    .ok_or_else(|| AppError::not_found("词库不存在"))?;
                if let Some(n) = name_for_task {
                    wb.name = n;
                }
                if let Some(d) = desc {
                    wb.description = d;
                }
                // 仅字段级更新 name/description,绝不回写 word_count,避免覆盖并发增删词维护的计数。
                store.update_wordbook_meta(&id_for_task, &wb.name, &wb.description)?;
                // word_count 可能在读取后被并发增删词修改,返回前重读以反映真实值。
                let wb = store
                    .get_wordbook(&id_for_task)?
                    .ok_or_else(|| AppError::not_found("词库不存在"))?;
                Ok(wb)
            },
        )
        .await??;

    write_audit(&state, &id, "update", &updated.name, Some(&admin.admin_id));
    Ok(ok(wordbook_to_json(&updated)))
}

// ---------------------------------------------------------------------------
// DELETE /:id
// ---------------------------------------------------------------------------

async fn delete_wordbook(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // 删除前先记审计(契约要求 delete 在删之前记)
    let detail = {
        let id_for_lookup = id.clone();
        state
            .run_store_task("admin.wordbooks.delete.lookup", move |store| {
                store.get_wordbook(&id_for_lookup)
            })
            .await??
            .map(|wb| wb.name)
            .unwrap_or_default()
    };
    write_audit(&state, &id, "delete", &detail, Some(&admin.admin_id));

    let id_for_task = id.clone();
    let existed = state
        .run_store_task("admin.wordbooks.delete", move |store| {
            store.delete_wordbook_cascade(&id_for_task)
        })
        .await??;
    if !existed {
        return Err(AppError::not_found("词库不存在"));
    }
    Ok(ok(serde_json::json!({ "deleted": true })))
}

// ---------------------------------------------------------------------------
// POST /:id/words  add word
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddWordRequest {
    text: String,
    #[serde(default)]
    pronunciation: Option<String>,
    #[serde(default)]
    part_of_speech: Option<String>,
    meaning: String,
    #[serde(default)]
    examples: Vec<String>,
}

async fn add_word(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<AddWordRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let text = req.text.trim().to_string();
    let meaning = req.meaning.trim().to_string();
    if text.is_empty() || meaning.is_empty() {
        return Err(AppError::bad_request(
            "WB_WORD_INVALID",
            "text 和 meaning 不能为空",
        ));
    }
    let word = Word {
        id: uuid::Uuid::new_v4().to_string(),
        text: text.clone(),
        meaning: meaning.clone(),
        pronunciation: req.pronunciation.clone(),
        part_of_speech: req.part_of_speech.clone(),
        difficulty: 0.5,
        examples: req.examples.clone(),
        tags: vec!["admin-added".to_string()],
        created_at: Utc::now(),
    };
    let id_for_task = id.clone();
    let word_for_task = word.clone();
    state
        .run_store_task(
            "admin.wordbooks.add_word",
            move |store| -> Result<(), AppError> {
                if store.get_wordbook(&id_for_task)?.is_none() {
                    return Err(AppError::not_found("词库不存在"));
                }
                store.upsert_word(&word_for_task)?;
                store.add_word_to_wordbook(&id_for_task, &word_for_task.id)?;
                Ok(())
            },
        )
        .await??;

    write_audit(&state, &id, "add_word", &text, Some(&admin.admin_id));

    let row = WordEntryRow {
        id: word.id,
        text: word.text,
        pronunciation: word.pronunciation,
        part_of_speech: word.part_of_speech,
        meaning: word.meaning,
        examples: word.examples,
        appear_count: 0,
        accuracy: None,
    };
    Ok(created(row))
}

// ---------------------------------------------------------------------------
// DELETE /:id/words/:word_id
// ---------------------------------------------------------------------------

async fn remove_word(
    admin: AdminAuthUser,
    Path((id, word_id)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let id_for_task = id.clone();
    let word_for_task = word_id.clone();
    let removed = state
        .run_store_task("admin.wordbooks.remove_word", move |store| {
            store.remove_word_from_wordbook(&id_for_task, &word_for_task)
        })
        .await??;

    write_audit(&state, &id, "remove_word", &word_id, Some(&admin.admin_id));
    Ok(ok(serde_json::json!({ "removed": removed })))
}

// ---------------------------------------------------------------------------
// GET /:id/export
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportWord {
    spelling: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    phonetic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    part_of_speech: Option<String>,
    meaning: String,
    examples: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportResponse {
    id: String,
    name: String,
    description: String,
    #[serde(rename = "type")]
    book_type: String,
    version: String,
    words: Vec<ExportWord>,
}

const EXPORT_LIMIT: i64 = 50000;

async fn export_wordbook(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let id_for_task = id.clone();
    let exported = state
        .run_store_task("admin.wordbooks.export", move |store| {
            store.export_wordbook(&id_for_task, EXPORT_LIMIT)
        })
        .await??;
    let (wb, words) = exported.ok_or_else(|| AppError::not_found("词库不存在"))?;
    let book_type = match wb.book_type {
        WordbookType::System => "system",
        WordbookType::User => "user",
    }
    .to_string();
    let words = words
        .into_iter()
        .map(|w| ExportWord {
            spelling: w.text,
            phonetic: w.pronunciation,
            part_of_speech: w.part_of_speech,
            meaning: w.meaning,
            examples: w.examples,
        })
        .collect();
    Ok(ok(ExportResponse {
        id: wb.id,
        name: wb.name,
        description: wb.description,
        book_type,
        version: String::new(),
        words,
    }))
}
