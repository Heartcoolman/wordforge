use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::Router;

use crate::auth::{AdminAuthUser, AuthUser};
use crate::constants::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE};
use crate::extractors::JsonBody;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};

use crate::response::{created, ok, paginated, AppError};
use crate::state::AppState;
use crate::store::operations::words::Word;

/// 对外 API 使用的 Word 视图，排除 embedding 等内部字段
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordPublic {
    id: String,
    text: String,
    meaning: String,
    pronunciation: Option<String>,
    part_of_speech: Option<String>,
    difficulty: f64,
    examples: Vec<String>,
    tags: Vec<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<&Word> for WordPublic {
    fn from(w: &Word) -> Self {
        Self {
            id: w.id.clone(),
            text: w.text.clone(),
            meaning: w.meaning.clone(),
            pronunciation: w.pronunciation.clone(),
            part_of_speech: w.part_of_speech.clone(),
            difficulty: w.difficulty,
            examples: w.examples.clone(),
            tags: w.tags.clone(),
            created_at: w.created_at,
        }
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_words).post(create_word))
        .route("/count", get(count_words))
        .route("/batch", post(batch_create_words))
        .route("/import-url", post(import_from_url))
        .route("/:id", get(get_word).put(update_word).delete(delete_word))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListWordsQuery {
    page: Option<u64>,
    per_page: Option<u64>,
    search: Option<String>,
}

impl ListWordsQuery {
    fn page(&self) -> u64 {
        self.page.unwrap_or(1).clamp(1, u64::MAX)
    }

    fn per_page(&self) -> u64 {
        self.per_page.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE)
    }
}

async fn list_words(
    _user: AuthUser,
    Query(query): Query<ListWordsQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let page = query.page();
    let per_page = query.per_page();
    let offset = ((page - 1) * per_page) as usize;
    let limit = per_page as usize;

    // B15: search support
    if let Some(ref search) = query.search {
        if !search.trim().is_empty() {
            let (items, total) = state.store().search_words(search, limit, offset)?;
            let items: Vec<WordPublic> = items.iter().map(WordPublic::from).collect();
            return Ok(paginated(items, total, page, per_page));
        }
    }

    let total = state.store().count_words()?;
    let items = state.store().list_words(limit, offset)?;
    let items: Vec<WordPublic> = items.iter().map(WordPublic::from).collect();
    Ok(paginated(items, total, page, per_page))
}

// B17: Count all words
async fn count_words(
    _user: AuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let total = state.store().count_words()?;
    Ok(ok(serde_json::json!({"total": total})))
}

// B14: Delete word
async fn delete_word(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let _ = state
        .store()
        .get_word(&id)?
        .ok_or_else(|| AppError::not_found("Word not found"))?;
    state.store().delete_word(&id)?;
    Ok(ok(serde_json::json!({"deleted": true, "id": id})))
}

async fn get_word(
    _user: AuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let word = state
        .store()
        .get_word(&id)?
        .ok_or_else(|| AppError::not_found("Word not found"))?;
    Ok(ok(WordPublic::from(&word)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpsertWordRequest {
    id: Option<String>,
    text: String,
    meaning: String,
    pronunciation: Option<String>,
    part_of_speech: Option<String>,
    difficulty: Option<f64>,
    examples: Option<Vec<String>>,
    tags: Option<Vec<String>>,
}

async fn create_word(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<UpsertWordRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if req.text.trim().is_empty() || req.meaning.trim().is_empty() {
        return Err(AppError::bad_request(
            "WORDS_INVALID_PAYLOAD",
            "text and meaning are required",
        ));
    }

    let word = Word {
        id: req.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        text: req.text.trim().to_string(),
        meaning: req.meaning.trim().to_string(),
        pronunciation: req.pronunciation,
        part_of_speech: req.part_of_speech,
        difficulty: req.difficulty.unwrap_or(0.5).clamp(0.0, 1.0),
        examples: req.examples.unwrap_or_default(),
        tags: req.tags.unwrap_or_default(),
        embedding: None,
        created_at: Utc::now(),
    };

    state.store().upsert_word(&word)?;
    Ok(created(WordPublic::from(&word)))
}

async fn update_word(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<UpsertWordRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let existing = state
        .store()
        .get_word(&id)?
        .ok_or_else(|| AppError::not_found("Word not found"))?;

    let word = Word {
        id: existing.id,
        text: if req.text.trim().is_empty() {
            existing.text
        } else {
            req.text.trim().to_string()
        },
        meaning: if req.meaning.trim().is_empty() {
            existing.meaning
        } else {
            req.meaning.trim().to_string()
        },
        pronunciation: req.pronunciation.or(existing.pronunciation),
        part_of_speech: req.part_of_speech.or(existing.part_of_speech),
        difficulty: req
            .difficulty
            .unwrap_or(existing.difficulty)
            .clamp(0.0, 1.0),
        examples: req.examples.unwrap_or(existing.examples),
        tags: req.tags.unwrap_or(existing.tags),
        embedding: existing.embedding,
        created_at: existing.created_at,
    };

    state.store().upsert_word(&word)?;
    Ok(ok(WordPublic::from(&word)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchCreateWordsRequest {
    words: Vec<UpsertWordRequest>,
}

async fn batch_create_words(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<BatchCreateWordsRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    if req.words.len() > state.config().limits.max_batch_size {
        return Err(AppError::bad_request(
            "BATCH_TOO_LARGE",
            &format!(
                "batch_create_words accepts at most {} words",
                state.config().limits.max_batch_size
            ),
        ));
    }
    let mut created_words: Vec<WordPublic> = Vec::new();
    let mut skipped_indices = Vec::new();

    for (i, item) in req.words.into_iter().enumerate() {
        if item.text.trim().is_empty() || item.meaning.trim().is_empty() {
            skipped_indices.push(i);
            continue;
        }
        let word = Word {
            id: item.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            text: item.text.trim().to_string(),
            meaning: item.meaning.trim().to_string(),
            pronunciation: item.pronunciation,
            part_of_speech: item.part_of_speech,
            difficulty: item.difficulty.unwrap_or(0.5).clamp(0.0, 1.0),
            examples: item.examples.unwrap_or_default(),
            tags: item.tags.unwrap_or_default(),
            embedding: None,
            created_at: Utc::now(),
        };
        state.store().upsert_word(&word)?;
        created_words.push(WordPublic::from(&word));
    }

    Ok(created(serde_json::json!({
        "count": created_words.len(),
        "skipped": skipped_indices,
        "items": created_words
    })))
}

// B30: Import words from URL
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportUrlRequest {
    url: String,
}

async fn import_from_url(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
    JsonBody(req): JsonBody<ImportUrlRequest>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    // SSRF 防护：验证 URL
    let url_parsed = validate_import_url(&req.url)?;

    // SSRF 防护：先完成 DNS 解析并校验公网 IP，再将请求固定到已校验地址，避免 DNS 重绑定窗口
    let (resolved_host, resolved_addrs) = resolve_import_url_addrs(&url_parsed).await?;

    // 限制响应大小为 10MB，使用流式读取
    const MAX_RESPONSE_SIZE: usize = 10 * 1_024 * 1_024;
    let mut client_builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none()); // 禁止自动跟随重定向，防止 DNS 重绑定

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

    let response = client.get(url_parsed.clone()).send().await.map_err(|e| {
        AppError::bad_request("IMPORT_FETCH_FAILED", &format!("Failed to fetch URL: {e}"))
    })?;

    // 检查 Content-Length（如果服务端提供了）
    if let Some(len) = response.content_length() {
        if len > MAX_RESPONSE_SIZE as u64 {
            return Err(AppError::bad_request(
                "IMPORT_TOO_LARGE",
                "Response too large (max 10MB)",
            ));
        }
    }

    // 流式读取 body，逐块累积并检查大小
    let mut body_bytes = Vec::new();
    let mut stream = response.bytes_stream();
    use futures::StreamExt;
    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| {
            AppError::bad_request(
                "IMPORT_READ_FAILED",
                &format!("Failed to read content: {e}"),
            )
        })?;
        body_bytes.extend_from_slice(&chunk);
        if body_bytes.len() > MAX_RESPONSE_SIZE {
            return Err(AppError::bad_request(
                "IMPORT_TOO_LARGE",
                "Response too large (max 10MB)",
            ));
        }
    }
    let content = String::from_utf8_lossy(&body_bytes);

    // Parse lines as "word\tmeaning" or "word - meaning"
    let mut imported: Vec<WordPublic> = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if imported.len() >= state.config().limits.max_import_words {
            break;
        }

        let (text, meaning) = if line.contains('\t') {
            let parts: Vec<&str> = line.splitn(2, '\t').collect();
            (
                parts[0].trim().to_string(),
                parts.get(1).unwrap_or(&"").trim().to_string(),
            )
        } else if line.contains(" - ") {
            let parts: Vec<&str> = line.splitn(2, " - ").collect();
            (
                parts[0].trim().to_string(),
                parts.get(1).unwrap_or(&"").trim().to_string(),
            )
        } else {
            (line.to_string(), String::new())
        };

        if text.is_empty() {
            continue;
        }

        let word = Word {
            id: uuid::Uuid::new_v4().to_string(),
            text,
            meaning: if meaning.is_empty() {
                String::new()
            } else {
                meaning
            },
            pronunciation: None,
            part_of_speech: None,
            difficulty: 0.5,
            examples: Vec::new(),
            tags: vec!["imported".to_string()],
            embedding: None,
            created_at: Utc::now(),
        };
        state.store().upsert_word(&word)?;
        imported.push(WordPublic::from(&word));
    }

    Ok(created(serde_json::json!({
        "imported": imported.len(),
        "items": imported,
    })))
}

fn validate_import_url(raw_url: &str) -> Result<reqwest::Url, AppError> {
    let parsed = reqwest::Url::parse(raw_url)
        .map_err(|e| AppError::bad_request("IMPORT_INVALID_URL", &format!("Invalid URL: {e}")))?;

    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(AppError::bad_request(
            "IMPORT_INVALID_URL",
            "Only http and https URLs are allowed",
        ));
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::bad_request("IMPORT_INVALID_URL", "URL must have a host"))?;

    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_private_ip(ip) {
            return Err(AppError::bad_request(
                "IMPORT_BLOCKED_URL",
                "Access to private networks is not allowed",
            ));
        }
    }

    let lower_host = host.to_lowercase();
    if lower_host == "localhost"
        || lower_host.ends_with(".local")
        || lower_host.ends_with(".internal")
    {
        return Err(AppError::bad_request(
            "IMPORT_BLOCKED_URL",
            "Access to localhost is not allowed",
        ));
    }

    Ok(parsed)
}

async fn resolve_import_url_addrs(
    url: &reqwest::Url,
) -> Result<(String, Vec<SocketAddr>), AppError> {
    let host = url
        .host_str()
        .ok_or_else(|| AppError::bad_request("IMPORT_INVALID_URL", "URL must have a host"))?
        .to_string();
    let port = url.port_or_known_default().unwrap_or(443);

    let addrs = if let Ok(ip) = host.parse::<IpAddr>() {
        vec![SocketAddr::new(ip, port)]
    } else {
        tokio::net::lookup_host((host.as_str(), port))
            .await
            .map_err(|_| AppError::bad_request("IMPORT_DNS_FAILED", "Could not resolve hostname"))?
            .collect::<Vec<SocketAddr>>()
    };

    let addrs = ensure_public_import_addrs(addrs)?;
    Ok((host, addrs))
}

fn ensure_public_import_addrs(addrs: Vec<SocketAddr>) -> Result<Vec<SocketAddr>, AppError> {
    if addrs.is_empty() {
        return Err(AppError::bad_request(
            "IMPORT_DNS_FAILED",
            "Could not resolve hostname",
        ));
    }

    for socket_addr in &addrs {
        if is_private_ip(socket_addr.ip()) {
            return Err(AppError::bad_request(
                "IMPORT_BLOCKED_URL",
                "URL resolves to private IP",
            ));
        }
    }

    Ok(addrs)
}

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 64  // 100.64.0.0/10 (CGN)
                || v4.octets()[0] == 169 && v4.octets()[1] == 254  // 169.254.0.0/16
                || v4.octets()[0] == 192 && v4.octets()[1] == 0 && v4.octets()[2] == 0
            // 192.0.0.0/24
        }
        IpAddr::V6(v6) => {
            v6.is_loopback() || v6.is_unspecified()
                // IPv4-mapped IPv6: ::ffff:x.x.x.x
                || if let Some(v4) = v6.to_ipv4_mapped() {
                    is_private_ip(IpAddr::V4(v4))
                } else { false }
                // 链路本地 fe80::/10
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                // 唯一本地 fc00::/7
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                // 弃用的站点本地 fec0::/10
                || (v6.segments()[0] & 0xffc0) == 0xfec0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn validate_import_url_rejects_non_http_scheme() {
        let err = validate_import_url("ftp://example.com/words.txt").unwrap_err();
        assert_eq!(err.code, "IMPORT_INVALID_URL");
    }

    #[test]
    fn validate_import_url_rejects_private_host() {
        let err = validate_import_url("http://127.0.0.1/words.txt").unwrap_err();
        assert_eq!(err.code, "IMPORT_BLOCKED_URL");
    }

    #[test]
    fn validate_import_url_allows_public_https() {
        let parsed = validate_import_url("https://example.com/words.txt").unwrap();
        assert_eq!(parsed.host_str(), Some("example.com"));
    }

    #[test]
    fn ensure_public_import_addrs_rejects_private_ip() {
        let err = ensure_public_import_addrs(vec![SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            443,
        )])
        .unwrap_err();
        assert_eq!(err.code, "IMPORT_BLOCKED_URL");
    }

    #[test]
    fn ensure_public_import_addrs_accepts_public_ip() {
        let addrs = ensure_public_import_addrs(vec![SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34)),
            443,
        )])
        .unwrap();
        assert_eq!(addrs.len(), 1);
    }
}
