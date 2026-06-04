//! 路由层覆盖：/api/admin/updates 的四个 handler
//!   GET  /status   get_status（含 require_updater 的 UPDATER_DISABLED 分支 + applyTask 合并）
//!   POST /check     force_check
//!   POST /apply     apply（CURRENT_VERSION_MISMATCH / UPDATE_IN_PROGRESS / 非法 body / 未鉴权）
//!   GET  /history   get_history（空 + 预置审计记录）
//!
//! 这些断言全部走路由层错误分支，不依赖平台/网络（services/updater 已被 coverage 忽略），
//! 因此跨平台稳定。涉及真实 GitHub 解析的成功路径已由 updater_http.rs / admin_updates_extra_http.rs 覆盖。

mod common;

use axum::http::{Method, StatusCode};

use common::app::{spawn_test_server, TestApp};
use common::auth::{auth_header, setup_admin_and_get_token};
use common::http::{request, response_json};

use learning_backend::config::UpdateCheckConfig;
use learning_backend::services::updater::Updater;
use learning_backend::state::ApplyTaskStatus;

/// 构造一个永不联网成功的 Updater（api_url 指向一个不可达端口），仅用于让
/// `require_updater` 返回 Some，从而把 handler 推进到 require_updater 之后的分支。
fn make_updater(current_tag: &str, install_dir: &std::path::Path) -> std::sync::Arc<Updater> {
    Updater::new(
        &UpdateCheckConfig {
            // 指向 127.0.0.1:1（保证拒连），任何真实 check 都会失败但不影响纯 handler 分支
            api_url: "http://127.0.0.1:1/repos/o/r/releases/latest".into(),
            cache_ttl_secs: 60,
            worker_enabled: false,
            worker_interval_secs: 3600,
            github_token: None,
            allow_downgrade: false,
            install_dir: Some(install_dir.to_path_buf()),
            max_tarball_bytes: 200 * 1024 * 1024,
            download_mirror_prefix: None,
        },
        current_tag,
    )
    .expect("updater")
}

fn attach_updater(app: &TestApp, current_tag: &str, tmp: &tempfile::TempDir) {
    app.state.set_updater(make_updater(current_tag, tmp.path()));
}

// ───────────────────────── require_updater: UPDATER_DISABLED ─────────────────────────

#[tokio::test]
async fn status_returns_503_when_updater_disabled() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // 不注入 updater → state.updater() 为 None
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/updates/status",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "body={body}");
    assert_eq!(body["code"], "UPDATER_DISABLED");
}

#[tokio::test]
async fn check_returns_503_when_updater_disabled() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/updates/check",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "body={body}");
    assert_eq!(body["code"], "UPDATER_DISABLED");
}

#[tokio::test]
async fn apply_returns_503_when_updater_disabled() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // 注意：apply 是 AdminAuthUser(parts) 先于 JsonBody(body) 解析，再走 require_updater。
    // 这里 body 合法，token 合法，updater 未注入 → 503。
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/updates/apply",
        Some(serde_json::json!({
            "channel": "stable",
            "targetVersion": "v9.9.9",
            "confirmCurrentVersion": "v0.0.1"
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "body={body}");
    assert_eq!(body["code"], "UPDATER_DISABLED");
}

// ───────────────────────── get_status 成功路径 ─────────────────────────

#[tokio::test]
async fn status_returns_snapshot_when_enabled() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let tmp = tempfile::tempdir().unwrap();
    attach_updater(&app, "v1.2.3", &tmp);

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/updates/status",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["data"]["currentVersion"], "v1.2.3");
    // 未发起 apply → 不应有 applyTask 字段
    assert!(body["data"]["applyTask"].is_null());
}

#[tokio::test]
async fn status_merges_apply_task_when_present() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let tmp = tempfile::tempdir().unwrap();
    attach_updater(&app, "v1.2.3", &tmp);

    // 预置一个已完成的 apply task，验证 get_status 把它合并进 payload.applyTask
    app.state.set_apply_task(Some(ApplyTaskStatus {
        task_id: "task-abc".into(),
        phase: "completed".into(),
        percent: 100,
        target_version: "v2.0.0".into(),
        started_at: chrono::Utc::now(),
        completed_at: Some(chrono::Utc::now()),
        error: None,
    }));

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/updates/status",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["data"]["applyTask"]["taskId"], "task-abc");
    assert_eq!(body["data"]["applyTask"]["phase"], "completed");
    assert_eq!(body["data"]["applyTask"]["percent"], 100);
}

// ───────────────────────── apply 错误分支（无网络依赖） ─────────────────────────

#[tokio::test]
async fn apply_rejects_current_version_mismatch() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let tmp = tempfile::tempdir().unwrap();
    attach_updater(&app, "v1.0.0", &tmp);

    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/updates/apply",
        Some(serde_json::json!({
            "channel": "beta",
            "targetVersion": "v2.0.0",
            "confirmCurrentVersion": "v0.0.1" // 与后端实际 v1.0.0 不符
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["code"], "CURRENT_VERSION_MISMATCH");
}

#[tokio::test]
async fn apply_rejects_when_task_already_running() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let tmp = tempfile::tempdir().unwrap();
    attach_updater(&app, "v1.0.0", &tmp);

    // 预置一个"进行中"的 task（completed_at=None & error=None → is_running()）
    app.state.set_apply_task(Some(ApplyTaskStatus {
        task_id: "running-task".into(),
        phase: "downloading".into(),
        percent: 30,
        target_version: "v1.5.0".into(),
        started_at: chrono::Utc::now(),
        completed_at: None,
        error: None,
    }));

    // confirmCurrentVersion 必须匹配，才能越过第一道校验抵达 UPDATE_IN_PROGRESS 分支
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/updates/apply",
        Some(serde_json::json!({
            "channel": "stable",
            "targetVersion": "v2.0.0",
            "confirmCurrentVersion": "v1.0.0"
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CONFLICT, "body={body}");
    assert_eq!(body["code"], "UPDATE_IN_PROGRESS");
}

#[tokio::test]
async fn apply_rejects_malformed_body() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let tmp = tempfile::tempdir().unwrap();
    attach_updater(&app, "v1.0.0", &tmp);

    // 缺少必填字段 channel/targetVersion → JsonBody 反序列化失败
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/updates/apply",
        Some(serde_json::json!({ "confirmCurrentVersion": "v1.0.0" })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["code"], "INVALID_REQUEST_BODY");
}

#[tokio::test]
async fn apply_rejects_invalid_channel_enum() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let tmp = tempfile::tempdir().unwrap();
    attach_updater(&app, "v1.0.0", &tmp);

    // channel 非 stable/beta → enum 反序列化失败 → INVALID_REQUEST_BODY
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/updates/apply",
        Some(serde_json::json!({
            "channel": "nightly",
            "targetVersion": "v2.0.0",
            "confirmCurrentVersion": "v1.0.0"
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["code"], "INVALID_REQUEST_BODY");
}

// ───────────────────────── get_history ─────────────────────────

#[tokio::test]
async fn history_returns_empty_entries_when_no_audit() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/updates/history",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    let entries = body["data"]["entries"].as_array().expect("entries array");
    assert!(entries.is_empty(), "无审计记录时 entries 应为空, body={body}");
}

#[tokio::test]
async fn history_returns_seeded_entries_newest_first() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;

    // 直接走 store 预置两条审计记录
    let store = app.state.store();
    store
        .insert_update_audit("audit-old", "admin-1", "v1.0.0", "v1.1.0", "stable")
        .expect("insert old");
    store
        .insert_update_audit("audit-new", "admin-1", "v1.1.0", "v1.2.0", "beta")
        .expect("insert new");

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/updates/history",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    let entries = body["data"]["entries"].as_array().expect("entries array");
    assert_eq!(entries.len(), 2, "应返回两条审计记录, body={body}");
    // outcome 初始为 in_progress；camelCase 序列化字段名核验
    assert!(entries.iter().all(|e| e["outcome"] == "in_progress"));
    assert!(entries.iter().all(|e| e["fromVersion"].is_string()));
    assert!(entries.iter().all(|e| e["adminId"] == "admin-1"));
    // 两条 id 都应出现
    let ids: Vec<&str> = entries.iter().filter_map(|e| e["id"].as_str()).collect();
    assert!(ids.contains(&"audit-old") && ids.contains(&"audit-new"), "ids={ids:?}");
}

// ───────────────────────── 鉴权缺失 / 非法 token（AdminAuthUser 拒绝） ─────────────────────────

#[tokio::test]
async fn all_endpoints_require_admin_token() {
    let app = spawn_test_server().await;
    let tmp = tempfile::tempdir().unwrap();
    attach_updater(&app, "v1.0.0", &tmp);

    let cases: &[(Method, &str, Option<serde_json::Value>)] = &[
        (Method::GET, "/api/admin/updates/status", None),
        (Method::POST, "/api/admin/updates/check", None),
        (Method::GET, "/api/admin/updates/history", None),
        (
            Method::POST,
            "/api/admin/updates/apply",
            Some(serde_json::json!({
                "channel": "stable",
                "targetVersion": "v2.0.0",
                "confirmCurrentVersion": "v1.0.0"
            })),
        ),
    ];

    for (m, path, body) in cases {
        // 完全不带 authorization 头 → 401
        let resp = request(&app.app, m.clone(), path, body.clone(), &[]).await;
        let (status, _, b) = response_json(resp).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{m} {path} 缺 token 应 401, body={b}"
        );
        assert_eq!(b["code"], "AUTH_UNAUTHORIZED", "{m} {path} body={b}");
    }
}

#[tokio::test]
async fn rejects_garbage_bearer_token() {
    let app = spawn_test_server().await;
    let tmp = tempfile::tempdir().unwrap();
    attach_updater(&app, "v1.0.0", &tmp);

    // 携带一个无法解析的 JWT → verify_jwt 失败 → 401
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/updates/status",
        None,
        &[("authorization", "Bearer not-a-real-jwt".to_string())],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "body={body}");
}

#[tokio::test]
async fn rejects_non_admin_user_token() {
    let app = spawn_test_server().await;
    // 普通用户 token（token_type = "user"）→ AdminAuthUser 拒绝
    let user_token = common::auth::login_and_get_token(&app.app).await;
    let tmp = tempfile::tempdir().unwrap();
    attach_updater(&app, "v1.0.0", &tmp);

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/updates/history",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "body={body}");
}
