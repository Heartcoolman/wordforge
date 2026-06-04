//! /api/wordbook-center/* 与 /api/admin/wordbook-center/* 集成测试。
//!
//! 设计约束：fetch_remote_json 走真实 reqwest + SSRF 校验，无法在本地起 mock
//! HTTP 服务（127.0.0.1/localhost 被 validate_import_url 拒绝）。因此“fetch 成功”
//! 路径（catalog 解析、导入 409、sync 成功）不可在集成层稳定复现。
//!
//! 本文件聚焦 **fetch 之前** 可达的分支以及 **fetch 必然失败** 的分支：
//! - 未配置 URL → 400 WB_CENTER_NOT_CONFIGURED / 空数组短路
//! - 已配置 URL 但指向内网 → fetch_remote_json 内部 validate_import_url 立即拒绝
//!   （IMPORT_BLOCKED_URL，无网络、无超时，确定性）
//! - user_sync 403（非本人导入）/ 404（无记录）、admin_sync 404
//! - import-url 校验分支（非法 scheme / SSRF / 缺字段）
//! - settings get/set/清空、import-history 分页
//! - 鉴权缺失 401
//!
//! 用 store.set_user_preferences 直接写内网 URL 绕过 PUT settings 的写入校验，
//! 从而把内网 URL 喂给后续 fetch 分支。

mod common;

use axum::http::{Method, StatusCode};
use axum::Router;
use chrono::Utc;

use common::app::spawn_test_server;
use common::auth::{auth_header, setup_admin_and_get_token};
use common::http::{request, response_json};
use learning_backend::store::operations::wb_center::WordbookCenterImport;
use learning_backend::store::operations::wordbooks::{Wordbook, WordbookType};
use learning_backend::store::Store;

// 内网 URL：通过 store 直接写入用户 prefs，绕过 PUT 校验。
const BLOCKED_URL: &str = "http://127.0.0.1/wb";

// ── 注册一个用户并拿到 (token, user_id) ──
async fn register_user(app: &Router) -> (String, String) {
    let email = format!("wbc-{}@test.com", uuid::Uuid::new_v4());
    let username = format!("wbc-{}", uuid::Uuid::new_v4().simple());
    let response = request(
        app,
        Method::POST,
        "/api/auth/register",
        Some(serde_json::json!({
            "email": email,
            "username": username,
            "password": "Passw0rd!",
        })),
        &[],
    )
    .await;
    let (status, _, body) = response_json(response).await;
    assert_eq!(status, StatusCode::CREATED, "register failed: {body}");
    let token = body["data"]["accessToken"].as_str().unwrap().to_string();
    let user_id = body["data"]["user"]["id"].as_str().unwrap().to_string();
    (token, user_id)
}

// 直接把内网词书中心 URL 写进用户 prefs（绕过 PUT settings 校验）。
fn set_user_center_url(store: &Store, user_id: &str, url: &str) {
    store
        .set_user_preferences(
            user_id,
            &serde_json::json!({ "wordbook_center_url": url }),
        )
        .unwrap();
}

fn seed_import(
    store: &Store,
    remote_id: &str,
    source_url: &str,
    user_id: Option<&str>,
) -> Wordbook {
    let wb = Wordbook {
        id: uuid::Uuid::new_v4().to_string(),
        name: format!("Book {remote_id}"),
        description: String::new(),
        book_type: WordbookType::User,
        user_id: user_id.map(ToOwned::to_owned),
        word_count: 5,
        created_at: Utc::now(),
    };
    store.upsert_wordbook(&wb).unwrap();
    store
        .upsert_wb_center_import(&WordbookCenterImport {
            remote_id: remote_id.to_string(),
            local_wordbook_id: wb.id.clone(),
            source_url: source_url.to_string(),
            version: "1.0".to_string(),
            user_id: user_id.map(ToOwned::to_owned),
            imported_at: Utc::now(),
            updated_at: Utc::now(),
            word_count: 5,
        })
        .unwrap();
    wb
}

// ════════════════════ 用户侧：未配置 ════════════════════

#[tokio::test]
async fn user_endpoints_unconfigured() {
    let app = spawn_test_server().await;
    let (token, _uid) = register_user(&app.app).await;
    let h = [("authorization", auth_header(&token))];

    // browse → 空数组（短路）
    let (s, _, b) = response_json(
        request(&app.app, Method::GET, "/api/wordbook-center/browse", None, &h).await,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(b["data"].as_array().unwrap().is_empty());

    // updates → 空数组（短路）
    let (s, _, b) = response_json(
        request(&app.app, Method::GET, "/api/wordbook-center/updates", None, &h).await,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(b["data"].as_array().unwrap().is_empty());

    // preview → 400 未配置
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::GET,
            "/api/wordbook-center/browse/x",
            None,
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["code"], "WB_CENTER_NOT_CONFIGURED");

    // import → 400 未配置
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::POST,
            "/api/wordbook-center/import/x",
            None,
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["code"], "WB_CENTER_NOT_CONFIGURED");

    // sync → 400 未配置（URL 校验在 import 记录查询之前）
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::POST,
            "/api/wordbook-center/updates/x/sync",
            None,
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["code"], "WB_CENTER_NOT_CONFIGURED");
}

// ════════════════════ 用户侧：已配置内网 URL → fetch 被 SSRF 拒绝 ════════════════════

#[tokio::test]
async fn user_browse_configured_blocked_url() {
    let app = spawn_test_server().await;
    let (token, uid) = register_user(&app.app).await;
    set_user_center_url(app.state.store(), &uid, BLOCKED_URL);
    let h = [("authorization", auth_header(&token))];

    // browse 会进入 fetch_remote_json(index.json) → validate_import_url 拒绝内网
    let (s, _, b) = response_json(
        request(&app.app, Method::GET, "/api/wordbook-center/browse", None, &h).await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["code"], "IMPORT_BLOCKED_URL");

    // preview 同样进入 fetch → 拒绝
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::GET,
            "/api/wordbook-center/browse/some-id?page=2&perPage=5",
            None,
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["code"], "IMPORT_BLOCKED_URL");
}

#[tokio::test]
async fn user_import_configured_blocked_records_failed_history() {
    let app = spawn_test_server().await;
    let (token, uid) = register_user(&app.app).await;
    set_user_center_url(app.state.store(), &uid, BLOCKED_URL);
    let h = [("authorization", auth_header(&token))];

    // import → do_import 内 fetch 失败 → 返回 400 并记录 failed history
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::POST,
            "/api/wordbook-center/import/remote-x",
            None,
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["code"], "IMPORT_BLOCKED_URL");

    // history 应记录一条 failed
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::GET,
            "/api/wordbook-center/import-history?page=1&perPage=20",
            None,
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["data"]["total"], 1);
    assert_eq!(b["data"]["data"][0]["status"], "failed");
    assert_eq!(b["data"]["data"][0]["sourceType"], "center");
    assert!(b["data"]["data"][0]["errorMessage"].is_string());
}

#[tokio::test]
async fn user_updates_configured_with_import_hits_fetch() {
    let app = spawn_test_server().await;
    let (token, uid) = register_user(&app.app).await;
    set_user_center_url(app.state.store(), &uid, BLOCKED_URL);
    // 预置一条属于该用户的导入，使 user_updates 跳过 imports.is_empty() 短路、进入 fetch
    seed_import(app.state.store(), "r1", BLOCKED_URL, Some(&uid));
    let h = [("authorization", auth_header(&token))];

    let (s, _, b) = response_json(
        request(&app.app, Method::GET, "/api/wordbook-center/updates", None, &h).await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["code"], "IMPORT_BLOCKED_URL");
}

// ════════════════════ 用户侧 sync：404 / 403 ════════════════════

#[tokio::test]
async fn user_sync_record_not_found_returns_404() {
    let app = spawn_test_server().await;
    let (token, uid) = register_user(&app.app).await;
    set_user_center_url(app.state.store(), &uid, BLOCKED_URL);
    let h = [("authorization", auth_header(&token))];

    // 已配置 URL 但无对应 import 记录 → 404（发生在 fetch 之前）
    let (s, _, _) = response_json(
        request(
            &app.app,
            Method::POST,
            "/api/wordbook-center/updates/nope/sync",
            None,
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn user_sync_other_users_record_returns_404() {
    let app = spawn_test_server().await;
    let (token, uid) = register_user(&app.app).await;
    set_user_center_url(app.state.store(), &uid, BLOCKED_URL);
    // 该 import 记录归属另一个用户
    seed_import(app.state.store(), "shared", BLOCKED_URL, Some("other-user-id"));
    let h = [("authorization", auth_header(&token))];

    let (s, _, _) = response_json(
        request(
            &app.app,
            Method::POST,
            "/api/wordbook-center/updates/shared/sync",
            None,
            &h,
        )
        .await,
    )
    .await;
    // m049:wb_center_imports 主键含 user_id 后,他人记录在本人命名空间不可见 → 404(不泄露存在性),而非旧的 403。
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn user_sync_own_record_reaches_fetch_blocked() {
    let app = spawn_test_server().await;
    let (token, uid) = register_user(&app.app).await;
    set_user_center_url(app.state.store(), &uid, BLOCKED_URL);
    // 归属本人 → 通过 403/404 守卫 → 进入 do_sync fetch → 内网被拒
    seed_import(app.state.store(), "mine", BLOCKED_URL, Some(&uid));
    let h = [("authorization", auth_header(&token))];

    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::POST,
            "/api/wordbook-center/updates/mine/sync",
            None,
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["code"], "IMPORT_BLOCKED_URL");
}

// ════════════════════ 用户侧 import-url 校验分支 ════════════════════

#[tokio::test]
async fn user_import_url_invalid_scheme() {
    let app = spawn_test_server().await;
    let (token, _uid) = register_user(&app.app).await;
    let h = [("authorization", auth_header(&token))];

    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::POST,
            "/api/wordbook-center/import-url",
            Some(serde_json::json!({ "url": "ftp://example.com/wb.json" })),
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["code"], "IMPORT_INVALID_URL");
}

#[tokio::test]
async fn user_import_url_blocked_records_failed_history() {
    let app = spawn_test_server().await;
    let (token, _uid) = register_user(&app.app).await;
    let h = [("authorization", auth_header(&token))];

    // SSRF 拦截 → 400，并记录 failed history（source_type=url）
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::POST,
            "/api/wordbook-center/import-url",
            Some(serde_json::json!({ "url": "http://192.168.0.5/wb.json" })),
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["code"], "IMPORT_BLOCKED_URL");

    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::GET,
            "/api/wordbook-center/import-history?page=1&perPage=20",
            None,
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["data"]["total"], 1);
    assert_eq!(b["data"]["data"][0]["status"], "failed");
    assert_eq!(b["data"]["data"][0]["sourceType"], "url");
    // source_name 取自 URL 末段
    assert_eq!(b["data"]["data"][0]["sourceName"], "wb.json");
}

#[tokio::test]
async fn user_import_url_malformed_url() {
    let app = spawn_test_server().await;
    let (token, _uid) = register_user(&app.app).await;
    let h = [("authorization", auth_header(&token))];

    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::POST,
            "/api/wordbook-center/import-url",
            Some(serde_json::json!({ "url": "not-a-url" })),
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["code"], "IMPORT_INVALID_URL");
}

#[tokio::test]
async fn user_import_url_missing_field_is_422_or_400() {
    let app = spawn_test_server().await;
    let (token, _uid) = register_user(&app.app).await;
    let h = [("authorization", auth_header(&token))];

    // 缺少必填 url 字段 → JsonBody 反序列化失败
    let (s, _, _) = response_json(
        request(
            &app.app,
            Method::POST,
            "/api/wordbook-center/import-url",
            Some(serde_json::json!({})),
            &h,
        )
        .await,
    )
    .await;
    assert!(
        s == StatusCode::BAD_REQUEST || s == StatusCode::UNPROCESSABLE_ENTITY,
        "expected 400/422, got {s}"
    );
}

// ════════════════════ 用户侧 settings ════════════════════

#[tokio::test]
async fn user_settings_get_set_clear() {
    let app = spawn_test_server().await;
    let (token, _uid) = register_user(&app.app).await;
    let h = [("authorization", auth_header(&token))];

    // 初始为空
    let (s, _, b) = response_json(
        request(&app.app, Method::GET, "/api/wordbook-center/settings", None, &h).await,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(b["data"]["wordbookCenterUrl"].is_null());

    // 设置 public URL
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::PUT,
            "/api/wordbook-center/settings",
            Some(serde_json::json!({ "wordbookCenterUrl": "https://cdn.example.com/wb" })),
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["data"]["wordbookCenterUrl"], "https://cdn.example.com/wb");

    // GET 再读应持久化
    let (s, _, b) = response_json(
        request(&app.app, Method::GET, "/api/wordbook-center/settings", None, &h).await,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["data"]["wordbookCenterUrl"], "https://cdn.example.com/wb");

    // 空串 → 清除
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::PUT,
            "/api/wordbook-center/settings",
            Some(serde_json::json!({ "wordbookCenterUrl": "" })),
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(b["data"]["wordbookCenterUrl"].is_null());
}

#[tokio::test]
async fn user_settings_set_blocked_url_rejected() {
    let app = spawn_test_server().await;
    let (token, _uid) = register_user(&app.app).await;
    let h = [("authorization", auth_header(&token))];

    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::PUT,
            "/api/wordbook-center/settings",
            Some(serde_json::json!({ "wordbookCenterUrl": "http://10.0.0.1/wb" })),
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["code"], "IMPORT_BLOCKED_URL");
}

#[tokio::test]
async fn user_settings_set_null_is_noop_ok() {
    let app = spawn_test_server().await;
    let (token, _uid) = register_user(&app.app).await;
    let h = [("authorization", auth_header(&token))];

    // wordbookCenterUrl 为 None（字段省略）→ 不校验、清除
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::PUT,
            "/api/wordbook-center/settings",
            Some(serde_json::json!({})),
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(b["data"]["wordbookCenterUrl"].is_null());
}

// ════════════════════ 用户侧 import-history 分页 ════════════════════

#[tokio::test]
async fn user_import_history_pagination_empty_and_offset() {
    let app = spawn_test_server().await;
    let (token, _uid) = register_user(&app.app).await;
    let h = [("authorization", auth_header(&token))];

    // 空历史
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::GET,
            "/api/wordbook-center/import-history?page=1&perPage=10",
            None,
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["data"]["total"], 0);
    assert!(b["data"]["data"].as_array().unwrap().is_empty());

    // 越界 page 仍返回 200、空 data
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::GET,
            "/api/wordbook-center/import-history?page=99&perPage=10",
            None,
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["data"]["total"], 0);
    assert!(b["data"]["data"].as_array().unwrap().is_empty());
}

// ════════════════════ 用户侧鉴权缺失 ════════════════════

#[tokio::test]
async fn user_endpoints_unauthorized() {
    let app = spawn_test_server().await;
    for (m, path) in [
        (Method::GET, "/api/wordbook-center/browse"),
        (Method::GET, "/api/wordbook-center/updates"),
        (Method::GET, "/api/wordbook-center/import-history"),
        (Method::GET, "/api/wordbook-center/settings"),
        (Method::GET, "/api/wordbook-center/browse/x"),
    ] {
        let resp = request(&app.app, m, path, None, &[]).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "path {path}");
    }
}

// ════════════════════ 管理员侧 ════════════════════

// 把系统词书中心 URL 设为内网（绕过 PUT 校验，直接写 store）。
fn set_admin_center_url(store: &Store, url: &str) {
    let mut s = store.get_system_settings().unwrap();
    s.wordbook_center_url = Some(url.to_string());
    store.save_system_settings(&s).unwrap();
}

#[tokio::test]
async fn admin_endpoints_configured_blocked_url() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    set_admin_center_url(app.state.store(), BLOCKED_URL);
    let h = [("authorization", auth_header(&admin))];

    // browse → fetch index.json → 内网拒绝
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::GET,
            "/api/admin/wordbook-center/browse",
            None,
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["code"], "IMPORT_BLOCKED_URL");

    // preview（带分页参数）→ fetch wordbooks/{id}.json → 拒绝
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::GET,
            "/api/admin/wordbook-center/browse/abc?page=1&perPage=10",
            None,
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["code"], "IMPORT_BLOCKED_URL");

    // import → do_import fetch → 拒绝
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::POST,
            "/api/admin/wordbook-center/import/abc",
            None,
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["code"], "IMPORT_BLOCKED_URL");
}

#[tokio::test]
async fn admin_updates_empty_when_no_imports() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    set_admin_center_url(app.state.store(), BLOCKED_URL);
    let h = [("authorization", auth_header(&admin))];

    // 已配置 URL 但无系统级导入 → imports.is_empty() 短路返回空数组（不触发 fetch）
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::GET,
            "/api/admin/wordbook-center/updates",
            None,
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(b["data"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn admin_updates_with_system_import_hits_fetch() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    set_admin_center_url(app.state.store(), BLOCKED_URL);
    // 系统级导入 user_id=None，list_wb_center_imports_by_user(None) 命中
    seed_import(app.state.store(), "sys1", BLOCKED_URL, None);
    let h = [("authorization", auth_header(&admin))];

    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::GET,
            "/api/admin/wordbook-center/updates",
            None,
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["code"], "IMPORT_BLOCKED_URL");
}

#[tokio::test]
async fn admin_sync_record_not_found_returns_404() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    set_admin_center_url(app.state.store(), BLOCKED_URL);
    let h = [("authorization", auth_header(&admin))];

    let (s, _, _) = response_json(
        request(
            &app.app,
            Method::POST,
            "/api/admin/wordbook-center/updates/nope/sync",
            None,
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn admin_sync_existing_record_reaches_fetch_blocked() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    set_admin_center_url(app.state.store(), BLOCKED_URL);
    // 系统级导入记录存在 → 通过 404 守卫 → do_sync fetch → 内网拒绝
    seed_import(app.state.store(), "syssync", BLOCKED_URL, None);
    let h = [("authorization", auth_header(&admin))];

    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::POST,
            "/api/admin/wordbook-center/updates/syssync/sync",
            None,
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["code"], "IMPORT_BLOCKED_URL");
}

#[tokio::test]
async fn admin_endpoints_unauthorized() {
    let app = spawn_test_server().await;
    for (m, path) in [
        (Method::GET, "/api/admin/wordbook-center/browse"),
        (Method::GET, "/api/admin/wordbook-center/browse/x"),
        (Method::GET, "/api/admin/wordbook-center/updates"),
        (Method::POST, "/api/admin/wordbook-center/import/x"),
        (Method::POST, "/api/admin/wordbook-center/updates/x/sync"),
    ] {
        let resp = request(&app.app, m, path, None, &[]).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "path {path}");
    }
}
