use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::Router;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;

use crate::auth::{AdminAuthUser, AuthUser};
use crate::constants::{DEFAULT_PAGE_SIZE, MAX_PAGE_NUMBER, MAX_PAGE_SIZE};
use crate::extractors::JsonBody;
use crate::response::{created, ok, paginated, AppError};
use crate::routes::words::{resolve_import_url_addrs, validate_import_url};
use crate::state::AppState;
use crate::store::operations::wb_center::{WordbookCenterImport, WordbookImportHistory};
use crate::store::operations::wordbooks::{Wordbook, WordbookType};
use crate::store::operations::words::Word;

// ── Remote data models ──

#[derive(Debug, Deserialize)]
struct RemoteCatalog {
    data: Vec<RemoteWordbookMeta>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteWordbookMeta {
    id: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    word_count: u64,
    #[serde(default)]
    cover_image: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    version: String,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    download_count: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteWordbook {
    id: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    word_count: u64,
    #[serde(default)]
    cover_image: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    version: String,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    download_count: Option<u64>,
    #[serde(default)]
    words: Vec<RemoteWord>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteWord {
    spelling: String,
    #[serde(default)]
    phonetic: Option<String>,
    #[serde(default)]
    meanings: Vec<String>,
    #[serde(default)]
    examples: Vec<String>,
    #[serde(default)]
    audio_url: Option<String>,
}

// ── Response models ──

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowseItem {
    #[serde(flatten)]
    meta: RemoteWordbookMeta,
    imported: bool,
    local_wordbook_id: Option<String>,
    local_version: Option<String>,
    has_update: bool,
    /// 本地标签(wordbook_local_tags),与远端 meta.tags 区分;未导入则空。
    /// 标签编辑器用此预填(而非远端 tags),避免 replace 保存污染本地表。
    local_tags: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInfo {
    remote_id: String,
    name: String,
    local_version: String,
    remote_version: String,
    local_wordbook_id: String,
}

// ── Query params ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewQuery {
    page: Option<u64>,
    per_page: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportHistoryQuery {
    page: Option<u64>,
    per_page: Option<u64>,
}

// ── Admin routes ──

pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/browse", get(admin_browse))
        .route("/browse/:id", get(admin_preview))
        .route("/import/:id", post(admin_import))
        .route("/updates", get(admin_updates))
        .route("/updates/:id/sync", post(admin_sync))
        // m022:本地 JSON 上传(CSV 在前端用 PapaParse 转 JSON 再 POST 避免后端引 csv crate)
        .route("/upload", post(admin_upload))
        // m022:本地标签覆盖层(远端 metadata 不可改,这是 admin 自定义的 wordbook 标签)
        .route("/:wordbook_id/tags", axum::routing::patch(admin_patch_tags))
}

// ── User routes ──

pub fn user_router() -> Router<AppState> {
    Router::new()
        .route("/browse", get(user_browse))
        .route("/browse/:id", get(user_preview))
        .route("/import-history", get(user_import_history))
        .route("/import/:id", post(user_import))
        .route("/import-url", post(user_import_url))
        .route("/updates", get(user_updates))
        .route("/updates/:id/sync", post(user_sync))
        .route("/settings", get(user_get_settings).put(user_set_settings))
}

// ── Shared HTTP helpers ──

async fn fetch_remote_json<T: serde::de::DeserializeOwned>(
    base_url: &str,
    path: &str,
) -> Result<T, AppError> {
    let full_url = format!("{}/{}", base_url.trim_end_matches('/'), path);
    let url_parsed = validate_import_url(&full_url)?;
    let (resolved_host, resolved_addrs) = resolve_import_url_addrs(&url_parsed).await?;

    let mut client_builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none());

    if url_parsed
        .host_str()
        .and_then(|host| host.parse::<IpAddr>().ok())
        .is_none()
    {
        client_builder = client_builder.resolve_to_addrs(&resolved_host, &resolved_addrs);
    }

    let client = client_builder
        .build()
        .map_err(|e| AppError::internal(&format!("HTTP client error: {e}")))?;

    let response = client.get(url_parsed).send().await.map_err(|e| {
        AppError::bad_request("WB_CENTER_FETCH_FAILED", &format!("获取远程数据失败：{e}"))
    })?;

    if !response.status().is_success() {
        return Err(AppError::bad_request(
            "WB_CENTER_FETCH_FAILED",
            &format!("远程服务返回状态码 {}", response.status()),
        ));
    }

    const MAX_SIZE: usize = 50 * 1_024 * 1_024;
    if let Some(len) = response.content_length() {
        if len > MAX_SIZE as u64 {
            return Err(AppError::bad_request(
                "WB_CENTER_TOO_LARGE",
                "响应内容过大（上限50MB）",
            ));
        }
    }

    let mut body_bytes = Vec::new();
    let mut stream = response.bytes_stream();
    use futures::StreamExt;
    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| {
            AppError::bad_request("WB_CENTER_READ_FAILED", &format!("读取内容失败：{e}"))
        })?;
        body_bytes.extend_from_slice(&chunk);
        if body_bytes.len() > MAX_SIZE {
            return Err(AppError::bad_request(
                "WB_CENTER_TOO_LARGE",
                "响应内容过大（上限50MB）",
            ));
        }
    }

    serde_json::from_slice(&body_bytes).map_err(|e| {
        AppError::bad_request("WB_CENTER_PARSE_FAILED", &format!("解析远程数据失败：{e}"))
    })
}

fn build_browse_items(
    catalog: Vec<RemoteWordbookMeta>,
    imports: &[WordbookCenterImport],
    local_tags: &HashMap<String, Vec<String>>,
) -> Vec<BrowseItem> {
    let import_map: HashMap<&str, &WordbookCenterImport> =
        imports.iter().map(|i| (i.remote_id.as_str(), i)).collect();

    catalog
        .into_iter()
        .map(|meta| {
            let imp = import_map.get(meta.id.as_str());
            let local_tags = imp
                .and_then(|i| local_tags.get(&i.local_wordbook_id).cloned())
                .unwrap_or_default();
            BrowseItem {
                imported: imp.is_some(),
                local_wordbook_id: imp.map(|i| i.local_wordbook_id.clone()),
                local_version: imp.map(|i| i.version.clone()),
                has_update: imp
                    .map(|i| !meta.version.is_empty() && i.version != meta.version)
                    .unwrap_or(false),
                local_tags,
                meta,
            }
        })
        .collect()
}

/// 批量取若干已导入词书的本地标签,key 为 local_wordbook_id。
fn local_tags_map(
    store: &crate::store::Store,
    imports: &[WordbookCenterImport],
) -> HashMap<String, Vec<String>> {
    let mut m = HashMap::new();
    for imp in imports {
        let tags = store
            .list_wordbook_local_tags(&imp.local_wordbook_id)
            .unwrap_or_default();
        m.insert(imp.local_wordbook_id.clone(), tags);
    }
    m
}

fn map_remote_word(rw: &RemoteWord, remote_id: &str) -> Word {
    Word {
        id: uuid::Uuid::new_v4().to_string(),
        text: rw.spelling.clone(),
        meaning: rw.meanings.join("; "),
        pronunciation: rw.phonetic.clone(),
        part_of_speech: None,
        difficulty: 0.5,
        examples: rw.examples.clone(),
        tags: vec![
            "imported".to_string(),
            "wb-center".to_string(),
            remote_id.to_string(),
        ],
        created_at: Utc::now(),
    }
}

fn persist_remote_wordbook_import(
    store: &crate::store::Store,
    source_url: &str,
    remote: RemoteWordbook,
    book_type: WordbookType,
    user_id: Option<String>,
) -> Result<serde_json::Value, AppError> {
    // 409 预检按导入者 user_id 收口:多个 end-user 可各自导入同一 center 词书,互不阻断
    //（此前缺 user_id 致第二用户被永久 409 挡住)。admin/system(user_id=None)沿用空串命名空间。
    if store
        .get_wb_center_import(source_url, &remote.id, user_id.as_deref())?
        .is_some()
    {
        return Err(AppError::conflict(
            "WB_CENTER_ALREADY_IMPORTED",
            "该词书已被导入",
        ));
    }

    let wordbook_id = uuid::Uuid::new_v4().to_string();
    let mut words = Vec::with_capacity(remote.words.len());
    let mut skipped = 0u64;
    for rw in &remote.words {
        if rw.spelling.trim().is_empty() {
            skipped += 1;
            continue;
        }
        words.push(map_remote_word(rw, &remote.id));
    }
    let imported = words.len() as u64;

    let book = Wordbook {
        id: wordbook_id.clone(),
        name: remote.name.clone(),
        description: remote.description.clone(),
        book_type,
        user_id: user_id.clone(),
        word_count: imported,
        created_at: Utc::now(),
    };
    let import_record = WordbookCenterImport {
        remote_id: remote.id.clone(),
        local_wordbook_id: wordbook_id.clone(),
        source_url: source_url.to_string(),
        version: remote.version,
        user_id,
        imported_at: Utc::now(),
        updated_at: Utc::now(),
        word_count: imported,
    };
    // 单事务原子写:建词书 + 批量词条/挂接 + 导入记录,任一步失败整笔回滚不留孤儿。
    store.import_remote_wordbook_atomic(&book, &words, &import_record)?;

    let wb = store.get_wordbook(&wordbook_id)?;
    Ok(serde_json::json!({
        "wordbook": wb,
        "wordsImported": imported,
        "wordsSkipped": skipped,
    }))
}

fn sync_remote_wordbook_import(
    store: &crate::store::Store,
    import_record: &WordbookCenterImport,
    remote: RemoteWordbook,
) -> Result<serde_json::Value, AppError> {
    let wb_id = import_record.local_wordbook_id.clone();

    let local_word_ids = store.list_wordbook_words(&wb_id, 100_000, 0)?;
    let local_words = store.get_words_by_ids(&local_word_ids)?;
    let mut text_to_word: HashMap<String, Word> = HashMap::new();
    for w in local_words.values() {
        text_to_word.insert(w.text.to_lowercase(), w.clone());
    }

    // 收集本次写入,交由 sync_remote_wordbook_atomic 单事务落库(消除多写无事务孤儿)。
    let mut upserts: Vec<Word> = Vec::new();
    let mut add_word_ids: Vec<String> = Vec::new();
    let mut words_added = 0u64;
    let mut words_updated = 0u64;
    let mut remote_texts = std::collections::HashSet::new();

    for rw in &remote.words {
        let text_lower = rw.spelling.trim().to_lowercase();
        if text_lower.is_empty() {
            continue;
        }
        remote_texts.insert(text_lower.clone());

        if let Some(existing) = text_to_word.get(&text_lower) {
            let new_meaning = rw.meanings.join("; ");
            let meaning_changed = existing.meaning != new_meaning;
            let pron_changed = existing.pronunciation != rw.phonetic;
            if meaning_changed || pron_changed {
                let mut w = existing.clone();
                w.meaning = new_meaning;
                w.pronunciation = rw.phonetic.clone();
                upserts.push(w);
                words_updated += 1;
            }
        } else {
            let word = map_remote_word(rw, &import_record.remote_id);
            add_word_ids.push(word.id.clone());
            upserts.push(word);
            words_added += 1;
        }
    }

    // 仅移除"本远程来源"且已从远程消失的词:按 map_remote_word 写入的 wb-center + remote_id tag
    // 判定来源,绝不删用户手动加入的词(此前无差别删除会静默销毁用户自加词条)。
    let remote_id = &import_record.remote_id;
    let mut remove_word_ids: Vec<String> = Vec::new();
    for (text_lower, word) in &text_to_word {
        if remote_texts.contains(text_lower) {
            continue;
        }
        let from_this_remote = word.tags.iter().any(|t| t == "wb-center")
            && word.tags.iter().any(|t| t == remote_id);
        if from_this_remote {
            remove_word_ids.push(word.id.clone());
        }
    }
    let words_removed = remove_word_ids.len() as u64;

    let mut updated_import = import_record.clone();
    updated_import.version = remote.version;
    updated_import.updated_at = Utc::now();
    // word_count 由 sync_remote_wordbook_atomic 在事务内按实际挂接数权威回写。
    store.sync_remote_wordbook_atomic(&wb_id, &upserts, &add_word_ids, &remove_word_ids, &updated_import)?;

    let wb = store.get_wordbook(&wb_id)?;
    Ok(serde_json::json!({
        "wordbook": wb,
        "wordsAdded": words_added,
        "wordsUpdated": words_updated,
        "wordsRemoved": words_removed,
    }))
}

async fn do_import(
    state: &AppState,
    base_url: &str,
    remote_id: &str,
    book_type: WordbookType,
    user_id: Option<String>,
) -> Result<serde_json::Value, AppError> {
    let remote: RemoteWordbook =
        fetch_remote_json(base_url, &format!("wordbooks/{}.json", remote_id)).await?;
    // 导入记录按 remote.id 入库,而 sync/更新检测按请求的 path id 查找。若 JSON 内部 id 与 path id
    // 不一致,导入会成功却永远无法同步("导入记录不存在")。故此处强制一致,不一致即拒绝。
    if remote.id != remote_id {
        return Err(AppError::bad_request(
            "WB_CENTER_ID_MISMATCH",
            "远程词书内部 id 与请求 id 不一致",
        ));
    }
    let downloaded_remote_id = remote.id.clone();
    let source_url = base_url.to_string();
    let store = state.store().clone();

    let result = crate::blocking::run_blocking("wordbook_center.import", move || {
        persist_remote_wordbook_import(&store, &source_url, remote, book_type, user_id)
    })
    .await??;

    // Fire-and-forget download counter —— 必须走与主拉取相同的 SSRF 校验 + 地址 pinning +
    // 禁重定向，否则用户可控的 base_url 可借 DNS rebinding / 3xx 重定向把这条 POST 打向内网。
    let counter_url = format!(
        "{}/wordbooks/{}/download",
        base_url.trim_end_matches('/'),
        downloaded_remote_id
    );
    if let Ok(url_parsed) = validate_import_url(&counter_url) {
        if let Ok((resolved_host, resolved_addrs)) = resolve_import_url_addrs(&url_parsed).await {
            let mut client_builder = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .redirect(reqwest::redirect::Policy::none());
            if url_parsed
                .host_str()
                .and_then(|host| host.parse::<IpAddr>().ok())
                .is_none()
            {
                client_builder =
                    client_builder.resolve_to_addrs(&resolved_host, &resolved_addrs);
            }
            if let Ok(client) = client_builder.build() {
                tokio::spawn(async move {
                    let _ = client.post(url_parsed).send().await;
                });
            }
        }
    }

    Ok(result)
}

async fn do_sync(
    state: &AppState,
    base_url: &str,
    import_record: &WordbookCenterImport,
) -> Result<serde_json::Value, AppError> {
    let remote: RemoteWordbook = fetch_remote_json(
        base_url,
        &format!("wordbooks/{}.json", import_record.remote_id),
    )
    .await?;
    let store = state.store().clone();
    let import_record = import_record.clone();
    crate::blocking::run_blocking("wordbook_center.sync", move || {
        sync_remote_wordbook_import(&store, &import_record, remote)
    })
    .await?
}

/// best-effort 写词库审计:从 do_import/do_sync/admin_upload 的结果里取
/// wordbook.id + name,失败仅 tracing::warn 不影响主流程。
fn write_wb_audit_from_result(
    state: &AppState,
    action: &str,
    admin_id: &str,
    result: &serde_json::Value,
) {
    let wb = result.get("wordbook");
    let wordbook_id = wb
        .and_then(|w| w.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if wordbook_id.is_empty() {
        return;
    }
    let detail = wb
        .and_then(|w| w.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if let Err(e) =
        state
            .store()
            .insert_wordbook_audit(wordbook_id, action, &detail, Some(admin_id))
    {
        tracing::warn!(error=%e, action=%action, "写 wordbook 远端导入审计失败(不影响主流程)");
    }
}

fn source_name_from_url(url: &str) -> Option<String> {
    url.rsplit('/')
        .next()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
}

fn truncate_error_message(message: &str) -> String {
    message.chars().take(1000).collect()
}

fn success_import_history(
    user_id: &str,
    source_type: &str,
    source_name: Option<String>,
    source_url: Option<String>,
    result: &serde_json::Value,
) -> WordbookImportHistory {
    let wordbook = result.get("wordbook");
    WordbookImportHistory {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user_id.to_string(),
        source_type: source_type.to_string(),
        source_name,
        source_url,
        status: "success".to_string(),
        wordbook_id: wordbook
            .and_then(|value| value.get("id"))
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        wordbook_name: wordbook
            .and_then(|value| value.get("name"))
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        words_imported: result.get("wordsImported").and_then(|value| value.as_u64()),
        words_skipped: result.get("wordsSkipped").and_then(|value| value.as_u64()),
        error_message: None,
        created_at: Utc::now(),
    }
}

fn failed_import_history(
    user_id: &str,
    source_type: &str,
    source_name: Option<String>,
    source_url: Option<String>,
    error: &AppError,
) -> WordbookImportHistory {
    WordbookImportHistory {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user_id.to_string(),
        source_type: source_type.to_string(),
        source_name,
        source_url,
        status: "failed".to_string(),
        wordbook_id: None,
        wordbook_name: None,
        words_imported: None,
        words_skipped: None,
        error_message: Some(truncate_error_message(&error.message)),
        created_at: Utc::now(),
    }
}

async fn record_import_history(state: &AppState, history: WordbookImportHistory) {
    match state
        .run_store_task("wordbook_center.import_history.persist", move |store| {
            store.insert_wordbook_import_history(&history)
        })
        .await
    {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            tracing::warn!(error = %error, "Failed to persist wordbook import history");
        }
        Err(error) => {
            tracing::warn!(error = %error, "Failed to persist wordbook import history");
        }
    }
}

fn paginated_words(
    words: &[&RemoteWord],
    total: u64,
    page: u64,
    per_page: u64,
) -> serde_json::Value {
    let total_pages = if per_page > 0 {
        total.div_ceil(per_page)
    } else {
        0
    };
    serde_json::json!({
        "data": words.iter().map(|w| serde_json::json!({
            "spelling": w.spelling,
            "phonetic": w.phonetic,
            "meanings": w.meanings,
            "examples": w.examples,
        })).collect::<Vec<_>>(),
        "total": total,
        "page": page,
        "perPage": per_page,
        "totalPages": total_pages,
    })
}

// ════════════════════ Admin endpoints ════════════════════

async fn admin_browse(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let settings = state
        .run_store_task("wordbook_center.admin_browse.settings", |store| {
            store.get_system_settings()
        })
        .await??;
    let base_url = settings
        .wordbook_center_url
        .ok_or_else(|| AppError::bad_request("WB_CENTER_NOT_CONFIGURED", "词书中心URL未配置"))?;

    let catalog: RemoteCatalog = fetch_remote_json(&base_url, "index.json").await?;
    let (imports, tags_map) = state
        .run_store_task("wordbook_center.admin_browse.imports", {
            let base_url = base_url.clone();
            move |store| {
                // 过滤到 admin/system 命名空间（user_id=None/空串），与 list_wb_center_imports_by_user(None)
                // 语义一致，避免 build_browse_items 的 remote_id HashMap key 与普通用户导入冲突而泄露其私有词书。
                let imports: Vec<_> = store
                    .list_wb_center_imports_by_source(&base_url)?
                    .into_iter()
                    .filter(|i| i.user_id.is_none() || i.user_id.as_deref() == Some(""))
                    .collect();
                let tags = local_tags_map(&store, &imports);
                Ok::<_, crate::store::StoreError>((imports, tags))
            }
        })
        .await??;
    let items = build_browse_items(catalog.data, &imports, &tags_map);
    Ok(ok(items))
}

async fn admin_preview(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    Query(q): Query<PreviewQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let settings = state
        .run_store_task("wordbook_center.admin_preview.settings", |store| {
            store.get_system_settings()
        })
        .await??;
    let base_url = settings
        .wordbook_center_url
        .ok_or_else(|| AppError::bad_request("WB_CENTER_NOT_CONFIGURED", "词书中心URL未配置"))?;

    let remote: RemoteWordbook =
        fetch_remote_json(&base_url, &format!("wordbooks/{}.json", id)).await?;

    // 上界封顶防 (page-1)*per_page 在 release(无 overflow-checks)下溢出回绕成乱序 offset。
    let page = q.page.unwrap_or(1).clamp(1, MAX_PAGE_NUMBER);
    let per_page = q
        .per_page
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    let total = remote.words.len() as u64;
    let offset = ((page - 1) * per_page) as usize;
    let words: Vec<&RemoteWord> = remote
        .words
        .iter()
        .skip(offset)
        .take(per_page as usize)
        .collect();

    Ok(ok(serde_json::json!({
        "id": remote.id,
        "name": remote.name,
        "description": remote.description,
        "wordCount": remote.word_count,
        "coverImage": remote.cover_image,
        "tags": remote.tags,
        "version": remote.version,
        "author": remote.author,
        "downloadCount": remote.download_count,
        "words": paginated_words(&words, total, page, per_page),
    })))
}

async fn admin_import(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let settings = state
        .run_store_task("wordbook_center.admin_import.settings", |store| {
            store.get_system_settings()
        })
        .await??;
    let base_url = settings
        .wordbook_center_url
        .ok_or_else(|| AppError::bad_request("WB_CENTER_NOT_CONFIGURED", "词书中心URL未配置"))?;

    let result = do_import(&state, &base_url, &id, WordbookType::System, None).await?;
    write_wb_audit_from_result(&state, "import", &admin.admin_id, &result);
    Ok(created(result))
}

async fn admin_updates(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let settings = state
        .run_store_task("wordbook_center.admin_updates.settings", |store| {
            store.get_system_settings()
        })
        .await??;
    let base_url = match settings.wordbook_center_url {
        Some(url) => url,
        None => return Ok(ok(Vec::<UpdateInfo>::new())),
    };

    let imports = state
        .run_store_task("wordbook_center.admin_updates.imports", |store| {
            store.list_wb_center_imports_by_user(None)
        })
        .await??;
    if imports.is_empty() {
        return Ok(ok(Vec::<UpdateInfo>::new()));
    }

    let catalog: RemoteCatalog = fetch_remote_json(&base_url, "index.json").await?;
    let remote_map: HashMap<&str, &RemoteWordbookMeta> =
        catalog.data.iter().map(|m| (m.id.as_str(), m)).collect();

    let updates: Vec<UpdateInfo> = imports
        .iter()
        .filter_map(|imp| {
            let remote = remote_map.get(imp.remote_id.as_str())?;
            if !remote.version.is_empty() && imp.version != remote.version {
                Some(UpdateInfo {
                    remote_id: imp.remote_id.clone(),
                    name: remote.name.clone(),
                    local_version: imp.version.clone(),
                    remote_version: remote.version.clone(),
                    local_wordbook_id: imp.local_wordbook_id.clone(),
                })
            } else {
                None
            }
        })
        .collect();

    Ok(ok(updates))
}

/// m022:POST /api/admin/wordbook-center/upload —— 上传本地 JSON 词书,绕过远端 center。
///
/// body 与 RemoteWordbook 同 shape:`{id, name, description, version, tags, words: [{spelling, phonetic, meanings, examples}]}`。
/// `id` 必须由调用方提供(用于去重 + 防止重复导入)。
/// 默认 book_type=System(admin 上传作为系统词书),user_id=None。
/// 复用 persist_remote_wordbook_import 走完整 store 写入流程。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadWordbookRequest {
    id: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    words: Vec<RemoteWord>,
}

async fn admin_upload(
    admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<UploadWordbookRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if req.id.is_empty() || req.name.is_empty() {
        return Err(AppError::bad_request(
            "WB_UPLOAD_INVALID",
            "id 和 name 必填",
        ));
    }
    if req.words.is_empty() {
        return Err(AppError::bad_request(
            "WB_UPLOAD_EMPTY",
            "words 列表为空,拒绝创建空词书",
        ));
    }
    // 用 `local:upload:<timestamp>` 作 source_url,与 wb_center 的远端 source_url 隔离
    let source_url = format!("local:upload:{}", Utc::now().to_rfc3339());
    let initial_tags = req.tags.clone();
    let wordbook_id_for_tags = req.id.clone();
    let remote = RemoteWordbook {
        id: req.id,
        name: req.name,
        description: req.description,
        word_count: req.words.len() as u64,
        cover_image: None,
        tags: req.tags,
        version: req.version,
        author: Some("admin-upload".to_string()),
        download_count: None,
        words: req.words,
    };
    let store = state.store().clone();
    let result = crate::blocking::run_blocking("wordbook_center.admin_upload", move || {
        persist_remote_wordbook_import(&store, &source_url, remote, WordbookType::System, None)
    })
    .await??;

    write_wb_audit_from_result(&state, "import", &admin.admin_id, &result);

    // 把 upload 时携带的 tags 直接落 wordbook_local_tags(免去前端再调 PATCH)
    if !initial_tags.is_empty() {
        // 从 result 里取 wordbook.id(persist_remote_wordbook_import 返回 wordbook 对象)
        let local_id = result
            .get("wordbook")
            .and_then(|w| w.get("id"))
            .and_then(|v| v.as_str())
            .unwrap_or(&wordbook_id_for_tags)
            .to_string();
        let store2 = state.store().clone();
        let local_id_for_log = local_id.clone();
        let tag_write =
            crate::blocking::run_blocking("wordbook_center.admin_upload.tags", move || {
                store2.set_wordbook_local_tags(&local_id, &initial_tags, Some("admin-upload"))
            })
            .await;
        // 失败仅 warn 不阻断:词书已入库,本地标签缺失可后续 PATCH 补写
        match tag_write {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!(
                wordbook_id = %local_id_for_log, error = %e,
                "upload 携带标签落 wordbook_local_tags 失败"
            ),
            Err(e) => tracing::warn!(error = %e, "upload 标签写入任务 join 失败"),
        }
    }

    Ok(created(result))
}

/// m022:PATCH /api/admin/wordbook-center/:wordbook_id/tags —— 本地标签覆盖。
///
/// body: `{add?: string[], remove?: string[]}` 或 `{replace: string[]}`(三选一,replace 优先)。
/// 写 wordbook_local_tags 表(m022 新建),不影响远端 metadata 中的 tags。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatchTagsRequest {
    #[serde(default)]
    add: Vec<String>,
    #[serde(default)]
    remove: Vec<String>,
    /// 给定时整体替换为这个列表,忽略 add/remove。前端"标签编辑器"提交全集时使用。
    #[serde(default)]
    replace: Option<Vec<String>>,
}

async fn admin_patch_tags(
    admin: AdminAuthUser,
    Path(wordbook_id): Path<String>,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<PatchTagsRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let admin_id = admin.admin_id.clone();
    let tags = state
        .run_store_task("wordbook_center.admin_patch_tags", move |store| {
            if let Some(replace) = req.replace {
                store.set_wordbook_local_tags(&wordbook_id, &replace, Some(&admin_id))?;
            } else {
                if !req.add.is_empty() {
                    store.add_wordbook_local_tags(&wordbook_id, &req.add, Some(&admin_id))?;
                }
                if !req.remove.is_empty() {
                    store.remove_wordbook_local_tags(&wordbook_id, &req.remove)?;
                }
            }
            store.list_wordbook_local_tags(&wordbook_id)
        })
        .await??;
    Ok(ok(serde_json::json!({ "tags": tags })))
}

async fn admin_sync(
    admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let settings = state
        .run_store_task("wordbook_center.admin_sync.settings", |store| {
            store.get_system_settings()
        })
        .await??;
    let base_url = settings
        .wordbook_center_url
        .ok_or_else(|| AppError::bad_request("WB_CENTER_NOT_CONFIGURED", "词书中心URL未配置"))?;

    let import_record = state
        .run_store_task("wordbook_center.admin_sync.import_record", {
            let base_url = base_url.clone();
            let id = id.clone();
            // admin 同步走 admin/system 命名空间(user_id=None → 空串)。
            move |store| store.get_wb_center_import(&base_url, &id, None)
        })
        .await??
        .ok_or_else(|| AppError::not_found("导入记录不存在"))?;

    let result = do_sync(&state, &base_url, &import_record).await?;
    write_wb_audit_from_result(&state, "sync", &admin.admin_id, &result);
    Ok(ok(result))
}

// ════════════════════ User endpoints ════════════════════

async fn get_user_wb_center_url(
    state: &AppState,
    user_id: &str,
) -> Result<Option<String>, AppError> {
    let store = state.store().clone();
    let user_id = user_id.to_string();

    crate::blocking::run_blocking(
        "wordbook_center.user_center_url",
        move || -> Result<_, AppError> {
            match store.get_user_preferences(&user_id)? {
                Some(prefs) => Ok(prefs
                    .get("wordbook_center_url")
                    .or_else(|| prefs.get("wordbookCenterUrl"))
                    .and_then(|value| value.as_str())
                    .filter(|url| !url.is_empty())
                    .map(str::to_string)),
                None => Ok(None),
            }
        },
    )
    .await?
}

async fn set_user_wb_center_url(
    state: &AppState,
    user_id: &str,
    url: Option<&str>,
) -> Result<(), AppError> {
    let store = state.store().clone();
    let user_id = user_id.to_string();
    let url = url.map(str::to_string);

    crate::blocking::run_blocking(
        "wordbook_center.set_user_center_url",
        move || -> Result<_, AppError> {
            let mut prefs = store
                .get_user_preferences(&user_id)?
                .unwrap_or(serde_json::json!({}));

            if let Some(obj) = prefs.as_object_mut() {
                match url.as_deref() {
                    Some(url) if !url.is_empty() => {
                        obj.insert(
                            "wordbook_center_url".to_string(),
                            serde_json::Value::String(url.to_string()),
                        );
                    }
                    _ => {
                        obj.remove("wordbook_center_url");
                    }
                }
            }

            store.set_user_preferences(&user_id, &prefs)?;
            Ok(())
        },
    )
    .await?
}

async fn user_get_settings(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let url = get_user_wb_center_url(&state, &auth.user_id).await?;
    Ok(ok(serde_json::json!({ "wordbookCenterUrl": url })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateUserWbCenterSettings {
    wordbook_center_url: Option<String>,
}

async fn user_set_settings(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<UpdateUserWbCenterSettings>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if let Some(ref url) = req.wordbook_center_url {
        if !url.is_empty() {
            validate_import_url(url)?;
        }
    }
    set_user_wb_center_url(&state, &auth.user_id, req.wordbook_center_url.as_deref()).await?;
    let url = get_user_wb_center_url(&state, &auth.user_id).await?;
    Ok(ok(serde_json::json!({ "wordbookCenterUrl": url })))
}

async fn user_browse(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let base_url = match get_user_wb_center_url(&state, &auth.user_id).await? {
        Some(url) => url,
        None => return Ok(ok(Vec::<BrowseItem>::new())),
    };

    let catalog: RemoteCatalog = fetch_remote_json(&base_url, "index.json").await?;
    let all_imports = state
        .run_store_task("wordbook_center.user_browse.imports", {
            let base_url = base_url.clone();
            move |store| store.list_wb_center_imports_by_source(&base_url)
        })
        .await??;
    let user_imports: Vec<WordbookCenterImport> = all_imports
        .into_iter()
        .filter(|i| i.user_id.as_deref() == Some(&auth.user_id))
        .collect();
    let tags_map = state
        .run_store_task("wordbook_center.user_browse.local_tags", {
            let imports = user_imports.clone();
            move |store| Ok::<_, crate::store::StoreError>(local_tags_map(&store, &imports))
        })
        .await??;
    let items = build_browse_items(catalog.data, &user_imports, &tags_map);
    Ok(ok(items))
}

async fn user_preview(
    auth: AuthUser,
    Path(id): Path<String>,
    Query(q): Query<PreviewQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let base_url = get_user_wb_center_url(&state, &auth.user_id)
        .await?
        .ok_or_else(|| {
            AppError::bad_request("WB_CENTER_NOT_CONFIGURED", "个人词书中心URL未配置")
        })?;

    let remote: RemoteWordbook =
        fetch_remote_json(&base_url, &format!("wordbooks/{}.json", id)).await?;

    // 上界封顶防 (page-1)*per_page 在 release(无 overflow-checks)下溢出回绕成乱序 offset。
    let page = q.page.unwrap_or(1).clamp(1, MAX_PAGE_NUMBER);
    let per_page = q
        .per_page
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    let total = remote.words.len() as u64;
    let offset = ((page - 1) * per_page) as usize;
    let words: Vec<&RemoteWord> = remote
        .words
        .iter()
        .skip(offset)
        .take(per_page as usize)
        .collect();

    Ok(ok(serde_json::json!({
        "id": remote.id,
        "name": remote.name,
        "description": remote.description,
        "wordCount": remote.word_count,
        "coverImage": remote.cover_image,
        "tags": remote.tags,
        "version": remote.version,
        "author": remote.author,
        "downloadCount": remote.download_count,
        "words": paginated_words(&words, total, page, per_page),
    })))
}

async fn user_import_history(
    auth: AuthUser,
    Query(q): Query<ImportHistoryQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // 上界封顶防 (page-1)*per_page 在 release(无 overflow-checks)下溢出回绕成乱序 offset。
    let page = q.page.unwrap_or(1).clamp(1, MAX_PAGE_NUMBER);
    let per_page = q
        .per_page
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    let user_id = auth.user_id.clone();
    let all = state
        .run_store_task("wordbook_center.import_history", move |store| {
            store.list_wordbook_import_history(&user_id)
        })
        .await??;
    let total = all.len() as u64;
    let offset = ((page - 1) * per_page) as usize;
    let items = all
        .into_iter()
        .skip(offset)
        .take(per_page as usize)
        .collect::<Vec<_>>();
    Ok(paginated(items, total, page, per_page))
}

async fn user_import(
    auth: AuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id = auth.user_id.clone();
    let base_url = get_user_wb_center_url(&state, &auth.user_id)
        .await?
        .ok_or_else(|| {
            AppError::bad_request("WB_CENTER_NOT_CONFIGURED", "个人词书中心URL未配置")
        })?;

    let result = do_import(
        &state,
        &base_url,
        &id,
        WordbookType::User,
        Some(user_id.clone()),
    )
    .await;
    match result {
        Ok(result) => {
            record_import_history(
                &state,
                success_import_history(&user_id, "center", Some(id), Some(base_url), &result),
            )
            .await;
            Ok(created(result))
        }
        Err(error) => {
            record_import_history(
                &state,
                failed_import_history(&user_id, "center", Some(id), Some(base_url), &error),
            )
            .await;
            Err(error)
        }
    }
}

#[derive(Debug, Deserialize)]
struct ImportUrlRequest {
    url: String,
}

async fn user_import_url(
    auth: AuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<ImportUrlRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let user_id_value = auth.user_id.clone();
    let source_name = source_name_from_url(&req.url);
    let source_url_for_history = Some(req.url.clone());
    // Validate URL (SSRF protection)
    if let Err(error) = validate_import_url(&req.url) {
        record_import_history(
            &state,
            failed_import_history(
                &user_id_value,
                "url",
                source_name.clone(),
                source_url_for_history.clone(),
                &error,
            ),
        )
        .await;
        return Err(error);
    }

    // Split URL into base and filename for fetch
    let (base, file) = req.url.rsplit_once('/').unwrap_or((&req.url, ""));

    let remote: RemoteWordbook = match fetch_remote_json(base, file).await {
        Ok(remote) => remote,
        Err(error) => {
            record_import_history(
                &state,
                failed_import_history(
                    &user_id_value,
                    "url",
                    source_name.clone(),
                    source_url_for_history.clone(),
                    &error,
                ),
            )
            .await;
            return Err(error);
        }
    };

    // Use the full URL as source for dedup
    let source_url = req.url.clone();
    let store = state.store().clone();
    let user_id = Some(user_id_value.clone());
    let result = crate::blocking::run_blocking("wordbook_center.user_import_url", move || {
        persist_remote_wordbook_import(&store, &source_url, remote, WordbookType::User, user_id)
    })
    .await?;

    match result {
        Ok(result) => {
            record_import_history(
                &state,
                success_import_history(
                    &user_id_value,
                    "url",
                    source_name,
                    source_url_for_history,
                    &result,
                ),
            )
            .await;
            Ok(created(result))
        }
        Err(app_error) => {
            record_import_history(
                &state,
                failed_import_history(
                    &user_id_value,
                    "url",
                    source_name,
                    source_url_for_history,
                    &app_error,
                ),
            )
            .await;
            Err(app_error)
        }
    }
}

async fn user_updates(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let base_url = match get_user_wb_center_url(&state, &auth.user_id).await? {
        Some(url) => url,
        None => return Ok(ok(Vec::<UpdateInfo>::new())),
    };

    let imports = state
        .run_store_task("wordbook_center.user_updates.imports", {
            let user_id = auth.user_id.clone();
            move |store| store.list_wb_center_imports_by_user(Some(&user_id))
        })
        .await??;
    if imports.is_empty() {
        return Ok(ok(Vec::<UpdateInfo>::new()));
    }

    let catalog: RemoteCatalog = fetch_remote_json(&base_url, "index.json").await?;
    let remote_map: HashMap<&str, &RemoteWordbookMeta> =
        catalog.data.iter().map(|m| (m.id.as_str(), m)).collect();

    let updates: Vec<UpdateInfo> = imports
        .iter()
        .filter_map(|imp| {
            let remote = remote_map.get(imp.remote_id.as_str())?;
            if !remote.version.is_empty() && imp.version != remote.version {
                Some(UpdateInfo {
                    remote_id: imp.remote_id.clone(),
                    name: remote.name.clone(),
                    local_version: imp.version.clone(),
                    remote_version: remote.version.clone(),
                    local_wordbook_id: imp.local_wordbook_id.clone(),
                })
            } else {
                None
            }
        })
        .collect();

    Ok(ok(updates))
}

async fn user_sync(
    auth: AuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let base_url = get_user_wb_center_url(&state, &auth.user_id)
        .await?
        .ok_or_else(|| {
            AppError::bad_request("WB_CENTER_NOT_CONFIGURED", "个人词书中心URL未配置")
        })?;

    let import_record = state
        .run_store_task("wordbook_center.user_sync.import_record", {
            let base_url = base_url.clone();
            let id = id.clone();
            let uid = auth.user_id.clone();
            // 按本人命名空间取记录:他人对同一 center 词书的导入记录不可见(主键已含 user_id)。
            move |store| store.get_wb_center_import(&base_url, &id, Some(&uid))
        })
        .await??
        .ok_or_else(|| AppError::not_found("导入记录不存在"))?;

    if import_record.user_id.as_deref() != Some(&auth.user_id) {
        return Err(AppError::forbidden("只能同步自己导入的词书"));
    }

    let result = do_sync(&state, &base_url, &import_record).await?;
    Ok(ok(result))
}
