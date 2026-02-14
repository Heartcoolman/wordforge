use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::Router;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::IpAddr;

use crate::auth::{AdminAuthUser, AuthUser};
use crate::constants::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE};
use crate::extractors::JsonBody;
use crate::response::{created, ok, AppError};
use crate::routes::words::{resolve_import_url_addrs, validate_import_url};
use crate::state::AppState;
use crate::store::operations::wb_center::WordbookCenterImport;
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

// ── Admin routes ──

pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/browse", get(admin_browse))
        .route("/browse/:id", get(admin_preview))
        .route("/import/:id", post(admin_import))
        .route("/updates", get(admin_updates))
        .route("/updates/:id/sync", post(admin_sync))
}

// ── User routes ──

pub fn user_router() -> Router<AppState> {
    Router::new()
        .route("/browse", get(user_browse))
        .route("/browse/:id", get(user_preview))
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
        AppError::bad_request(
            "WB_CENTER_FETCH_FAILED",
            &format!("获取远程数据失败：{e}"),
        )
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
            AppError::bad_request(
                "WB_CENTER_READ_FAILED",
                &format!("读取内容失败：{e}"),
            )
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
        AppError::bad_request(
            "WB_CENTER_PARSE_FAILED",
            &format!("解析远程数据失败：{e}"),
        )
    })
}

fn build_browse_items(
    catalog: Vec<RemoteWordbookMeta>,
    imports: &[WordbookCenterImport],
) -> Vec<BrowseItem> {
    let import_map: HashMap<&str, &WordbookCenterImport> = imports
        .iter()
        .map(|i| (i.remote_id.as_str(), i))
        .collect();

    catalog
        .into_iter()
        .map(|meta| {
            let imp = import_map.get(meta.id.as_str());
            BrowseItem {
                imported: imp.is_some(),
                local_wordbook_id: imp.map(|i| i.local_wordbook_id.clone()),
                local_version: imp.map(|i| i.version.clone()),
                has_update: imp
                    .map(|i| !meta.version.is_empty() && i.version != meta.version)
                    .unwrap_or(false),
                meta,
            }
        })
        .collect()
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
        embedding: None,
        created_at: Utc::now(),
    }
}

fn import_words_to_store(
    state: &AppState,
    wordbook_id: &str,
    remote_id: &str,
    words: &[RemoteWord],
) -> Result<(u64, u64), AppError> {
    let mut imported = 0u64;
    let mut skipped = 0u64;
    for rw in words {
        if rw.spelling.trim().is_empty() {
            skipped += 1;
            continue;
        }
        let word = map_remote_word(rw, remote_id);
        let word_id = word.id.clone();
        if state.store().upsert_word(&word).is_ok() {
            let _ = state.store().add_word_to_wordbook(wordbook_id, &word_id);
            imported += 1;
        } else {
            skipped += 1;
        }
    }
    Ok((imported, skipped))
}

async fn do_import(
    state: &AppState,
    base_url: &str,
    remote_id: &str,
    book_type: WordbookType,
    user_id: Option<String>,
) -> Result<serde_json::Value, AppError> {
    if state
        .store()
        .get_wb_center_import(base_url, remote_id)?
        .is_some()
    {
        return Err(AppError::conflict(
            "WB_CENTER_ALREADY_IMPORTED",
            "该词书已被导入",
        ));
    }

    let remote: RemoteWordbook =
        fetch_remote_json(base_url, &format!("wordbooks/{}.json", remote_id)).await?;

    let wordbook_id = uuid::Uuid::new_v4().to_string();
    let book = Wordbook {
        id: wordbook_id.clone(),
        name: remote.name.clone(),
        description: remote.description.clone(),
        book_type,
        user_id: user_id.clone(),
        word_count: 0,
        created_at: Utc::now(),
    };
    state.store().upsert_wordbook(&book)?;

    let (imported, skipped) = import_words_to_store(state, &wordbook_id, &remote.id, &remote.words)?;

    if let Some(mut wb) = state.store().get_wordbook(&wordbook_id)? {
        wb.word_count = imported;
        state.store().upsert_wordbook(&wb)?;
    }

    let import_record = WordbookCenterImport {
        remote_id: remote.id.clone(),
        local_wordbook_id: wordbook_id.clone(),
        source_url: base_url.to_string(),
        version: remote.version.clone(),
        user_id,
        imported_at: Utc::now(),
        updated_at: Utc::now(),
        word_count: imported,
    };
    state.store().upsert_wb_center_import(&import_record)?;

    // Fire-and-forget download counter
    let counter_url = format!(
        "{}/wordbooks/{}/download",
        base_url.trim_end_matches('/'),
        remote.id
    );
    tokio::spawn(async move {
        let _ = reqwest::Client::new()
            .post(&counter_url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await;
    });

    let wb = state.store().get_wordbook(&wordbook_id)?;
    Ok(serde_json::json!({
        "wordbook": wb,
        "wordsImported": imported,
        "wordsSkipped": skipped,
    }))
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

    let wb_id = import_record.local_wordbook_id.clone();

    // Build local word index: text -> Word
    let local_word_ids = state.store().list_wordbook_words(&wb_id, 100_000, 0)?;
    let local_words = state.store().get_words_by_ids(&local_word_ids)?;
    let mut text_to_word: HashMap<String, Word> = HashMap::new();
    for w in local_words.values() {
        text_to_word.insert(w.text.to_lowercase(), w.clone());
    }

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
                let _ = state.store().upsert_word(&w);
                words_updated += 1;
            }
        } else {
            let word = map_remote_word(rw, &import_record.remote_id);
            let word_id = word.id.clone();
            if state.store().upsert_word(&word).is_ok() {
                let _ = state.store().add_word_to_wordbook(&wb_id, &word_id);
                words_added += 1;
            }
        }
    }

    // Remove words no longer in remote
    let mut words_removed = 0u64;
    for (text_lower, word) in &text_to_word {
        if !remote_texts.contains(text_lower) {
            let _ = state.store().remove_word_from_wordbook(&wb_id, &word.id);
            words_removed += 1;
        }
    }

    // Update import record
    let mut updated_import = import_record.clone();
    updated_import.version = remote.version;
    updated_import.updated_at = Utc::now();
    updated_import.word_count = state.store().count_wordbook_words(&wb_id)?;
    state.store().upsert_wb_center_import(&updated_import)?;

    if let Some(mut wb) = state.store().get_wordbook(&wb_id)? {
        wb.word_count = updated_import.word_count;
        state.store().upsert_wordbook(&wb)?;
    }

    let wb = state.store().get_wordbook(&wb_id)?;
    Ok(serde_json::json!({
        "wordbook": wb,
        "wordsAdded": words_added,
        "wordsUpdated": words_updated,
        "wordsRemoved": words_removed,
    }))
}

fn paginated_words(
    words: &[&RemoteWord],
    total: u64,
    page: u64,
    per_page: u64,
) -> serde_json::Value {
    let total_pages = if per_page > 0 {
        (total + per_page - 1) / per_page
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
    let settings = state.store().get_system_settings()?;
    let base_url = settings.wordbook_center_url.ok_or_else(|| {
        AppError::bad_request(
            "WB_CENTER_NOT_CONFIGURED",
            "词书中心URL未配置",
        )
    })?;

    let catalog: RemoteCatalog = fetch_remote_json(&base_url, "index.json").await?;
    let imports = state.store().list_wb_center_imports_by_source(&base_url)?;
    let items = build_browse_items(catalog.data, &imports);
    Ok(ok(items))
}

async fn admin_preview(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    Query(q): Query<PreviewQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let settings = state.store().get_system_settings()?;
    let base_url = settings.wordbook_center_url.ok_or_else(|| {
        AppError::bad_request(
            "WB_CENTER_NOT_CONFIGURED",
            "词书中心URL未配置",
        )
    })?;

    let remote: RemoteWordbook =
        fetch_remote_json(&base_url, &format!("wordbooks/{}.json", id)).await?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = q
        .per_page
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    let total = remote.words.len() as u64;
    let offset = ((page - 1) * per_page) as usize;
    let words: Vec<&RemoteWord> = remote.words.iter().skip(offset).take(per_page as usize).collect();

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
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let settings = state.store().get_system_settings()?;
    let base_url = settings.wordbook_center_url.ok_or_else(|| {
        AppError::bad_request(
            "WB_CENTER_NOT_CONFIGURED",
            "词书中心URL未配置",
        )
    })?;

    let result = do_import(&state, &base_url, &id, WordbookType::System, None).await?;
    Ok(created(result))
}

async fn admin_updates(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let settings = state.store().get_system_settings()?;
    let base_url = match settings.wordbook_center_url {
        Some(url) => url,
        None => return Ok(ok(Vec::<UpdateInfo>::new())),
    };

    let imports = state.store().list_wb_center_imports_by_user(None)?;
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

async fn admin_sync(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let settings = state.store().get_system_settings()?;
    let base_url = settings.wordbook_center_url.ok_or_else(|| {
        AppError::bad_request(
            "WB_CENTER_NOT_CONFIGURED",
            "词书中心URL未配置",
        )
    })?;

    let import_record = state
        .store()
        .get_wb_center_import(&base_url, &id)?
        .ok_or_else(|| AppError::not_found("导入记录不存在"))?;

    let result = do_sync(&state, &base_url, &import_record).await?;
    Ok(ok(result))
}

// ════════════════════ User endpoints ════════════════════

fn get_user_wb_center_url(state: &AppState, user_id: &str) -> Result<Option<String>, AppError> {
    let key = crate::store::keys::user_preferences_key(user_id)?;
    match state
        .store()
        .user_preferences
        .get(key.as_bytes())
        .map_err(|e| AppError::internal(&e.to_string()))?
    {
        Some(raw) => {
            let val: serde_json::Value =
                serde_json::from_slice(&raw).unwrap_or(serde_json::Value::Null);
            Ok(val
                .get("wordbookCenterUrl")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string()))
        }
        None => Ok(None),
    }
}

fn set_user_wb_center_url(
    state: &AppState,
    user_id: &str,
    url: Option<&str>,
) -> Result<(), AppError> {
    let key = crate::store::keys::user_preferences_key(user_id)?;
    let mut val: serde_json::Value = match state
        .store()
        .user_preferences
        .get(key.as_bytes())
        .map_err(|e| AppError::internal(&e.to_string()))?
    {
        Some(raw) => serde_json::from_slice(&raw).unwrap_or(serde_json::json!({})),
        None => serde_json::json!({}),
    };

    if let Some(obj) = val.as_object_mut() {
        match url {
            Some(u) if !u.is_empty() => {
                obj.insert(
                    "wordbookCenterUrl".to_string(),
                    serde_json::Value::String(u.to_string()),
                );
            }
            _ => {
                obj.remove("wordbookCenterUrl");
            }
        }
    }

    state
        .store()
        .user_preferences
        .insert(
            key.as_bytes(),
            serde_json::to_vec(&val).map_err(|e| AppError::internal(&e.to_string()))?,
        )
        .map_err(|e| AppError::internal(&e.to_string()))?;
    Ok(())
}

async fn user_get_settings(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let url = get_user_wb_center_url(&state, &auth.user_id)?;
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
    set_user_wb_center_url(&state, &auth.user_id, req.wordbook_center_url.as_deref())?;
    let url = get_user_wb_center_url(&state, &auth.user_id)?;
    Ok(ok(serde_json::json!({ "wordbookCenterUrl": url })))
}

async fn user_browse(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let base_url = match get_user_wb_center_url(&state, &auth.user_id)? {
        Some(url) => url,
        None => return Ok(ok(Vec::<BrowseItem>::new())),
    };

    let catalog: RemoteCatalog = fetch_remote_json(&base_url, "index.json").await?;
    let all_imports = state.store().list_wb_center_imports_by_source(&base_url)?;
    let user_imports: Vec<WordbookCenterImport> = all_imports
        .into_iter()
        .filter(|i| i.user_id.as_deref() == Some(&auth.user_id))
        .collect();
    let items = build_browse_items(catalog.data, &user_imports);
    Ok(ok(items))
}

async fn user_preview(
    auth: AuthUser,
    Path(id): Path<String>,
    Query(q): Query<PreviewQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let base_url = get_user_wb_center_url(&state, &auth.user_id)?.ok_or_else(|| {
        AppError::bad_request(
            "WB_CENTER_NOT_CONFIGURED",
            "个人词书中心URL未配置",
        )
    })?;

    let remote: RemoteWordbook =
        fetch_remote_json(&base_url, &format!("wordbooks/{}.json", id)).await?;

    let page = q.page.unwrap_or(1).max(1);
    let per_page = q
        .per_page
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    let total = remote.words.len() as u64;
    let offset = ((page - 1) * per_page) as usize;
    let words: Vec<&RemoteWord> = remote.words.iter().skip(offset).take(per_page as usize).collect();

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

async fn user_import(
    auth: AuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let base_url = get_user_wb_center_url(&state, &auth.user_id)?.ok_or_else(|| {
        AppError::bad_request(
            "WB_CENTER_NOT_CONFIGURED",
            "个人词书中心URL未配置",
        )
    })?;

    let result = do_import(
        &state,
        &base_url,
        &id,
        WordbookType::User,
        Some(auth.user_id),
    )
    .await?;
    Ok(created(result))
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
    // Validate URL (SSRF protection)
    validate_import_url(&req.url)?;

    // Split URL into base and filename for fetch
    let (base, file) = req
        .url
        .rsplit_once('/')
        .unwrap_or((&req.url, ""));

    let remote: RemoteWordbook = fetch_remote_json(base, file).await?;

    // Use the full URL as source for dedup
    let source_url = req.url.clone();

    if state
        .store()
        .get_wb_center_import(&source_url, &remote.id)?
        .is_some()
    {
        return Err(AppError::conflict(
            "WB_CENTER_ALREADY_IMPORTED",
            "该词书已被导入",
        ));
    }

    let wordbook_id = uuid::Uuid::new_v4().to_string();
    let book = Wordbook {
        id: wordbook_id.clone(),
        name: remote.name.clone(),
        description: remote.description.clone(),
        book_type: WordbookType::User,
        user_id: Some(auth.user_id.clone()),
        word_count: 0,
        created_at: Utc::now(),
    };
    state.store().upsert_wordbook(&book)?;

    let (imported, skipped) = import_words_to_store(&state, &wordbook_id, &remote.id, &remote.words)?;

    if let Some(mut wb) = state.store().get_wordbook(&wordbook_id)? {
        wb.word_count = imported;
        state.store().upsert_wordbook(&wb)?;
    }

    let import_record = WordbookCenterImport {
        remote_id: remote.id.clone(),
        local_wordbook_id: wordbook_id.clone(),
        source_url,
        version: remote.version,
        user_id: Some(auth.user_id),
        imported_at: Utc::now(),
        updated_at: Utc::now(),
        word_count: imported,
    };
    state.store().upsert_wb_center_import(&import_record)?;

    let wb = state.store().get_wordbook(&wordbook_id)?;
    Ok(created(serde_json::json!({
        "wordbook": wb,
        "wordsImported": imported,
        "wordsSkipped": skipped,
    })))
}

async fn user_updates(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let base_url = match get_user_wb_center_url(&state, &auth.user_id)? {
        Some(url) => url,
        None => return Ok(ok(Vec::<UpdateInfo>::new())),
    };

    let imports = state
        .store()
        .list_wb_center_imports_by_user(Some(&auth.user_id))?;
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
    let base_url = get_user_wb_center_url(&state, &auth.user_id)?.ok_or_else(|| {
        AppError::bad_request(
            "WB_CENTER_NOT_CONFIGURED",
            "个人词书中心URL未配置",
        )
    })?;

    let import_record = state
        .store()
        .get_wb_center_import(&base_url, &id)?
        .ok_or_else(|| AppError::not_found("导入记录不存在"))?;

    if import_record.user_id.as_deref() != Some(&auth.user_id) {
        return Err(AppError::forbidden(
            "只能同步自己导入的词书",
        ));
    }

    let result = do_sync(&state, &base_url, &import_record).await?;
    Ok(ok(result))
}
