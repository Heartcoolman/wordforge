//! /api/admin/updates 路由集成测试 —— 在 TestApp 上注入真实 Updater，
//! 覆盖 get_status / force_check / map_err 各分支。
//! apply 涉及 fork-exec 自重启，仍由 SERVICE_UNAVAILABLE/UPDATER_DISABLED 路径覆盖。

mod common;

use axum::http::{Method, StatusCode};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use common::app::spawn_test_server;
use common::auth::{auth_header, setup_admin_and_get_token};
use common::http::{request, response_json};

use learning_backend::config::UpdateCheckConfig;
use learning_backend::services::updater::Updater;

fn attach_updater(app: &common::app::TestApp, updater: std::sync::Arc<Updater>) {
    app.state.set_updater(updater);
}

fn make_updater(server_uri: &str, current_tag: &str, install_dir: &std::path::Path) -> std::sync::Arc<Updater> {
    Updater::new(
        &UpdateCheckConfig {
            api_url: format!("{server_uri}/repos/o/r/releases/latest"),
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

#[tokio::test]
async fn it_admin_updates_status_returns_snapshot_when_enabled() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    let updater = make_updater(&server.uri(), "v0.0.1", tmp.path());
    attach_updater(&app, updater);

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
    assert!(body["data"]["currentVersion"].is_string());
}

#[tokio::test]
async fn it_admin_updates_force_check_broadcasts_when_new_release() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;

    let release_body = serde_json::json!({
        "tag_name": "v99.0.0",
        "published_at": "2030-01-01T00:00:00Z",
        "html_url": format!("{}/release", server.uri()),
        "body": "release notes",
        "assets": [
            {"name": "wordforge-linux-x86_64.tar.gz", "browser_download_url": format!("{}/dl/x86.tar.gz", server.uri()), "size": 1024},
            {"name": "wordforge-linux-x86_64.tar.gz.sha256", "browser_download_url": format!("{}/dl/x86.sha256", server.uri()), "size": 64},
        ]
    });
    Mock::given(method("GET"))
        .and(path("/repos/o/r/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(release_body))
        .mount(&server)
        .await;

    let updater = make_updater(&server.uri(), "v0.0.1", tmp.path());
    attach_updater(&app, updater);

    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/updates/check",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    // 仅 Linux 平台能解析出 asset → latest_version + has_update。
    // 其它平台 parse_release_payload 返回 None，二者为默认值。
    if cfg!(target_os = "linux") {
        assert_eq!(body["data"]["hasUpdate"], true);
        assert_eq!(body["data"]["latestVersion"], "v99.0.0");
    }
}

#[tokio::test]
async fn it_admin_updates_force_check_no_update() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;

    let release_body = serde_json::json!({
        "tag_name": "v0.0.1",
        "published_at": "2025-01-01T00:00:00Z",
        "html_url": format!("{}/release", server.uri()),
        "body": "same",
        "assets": []
    });
    Mock::given(method("GET"))
        .and(path("/repos/o/r/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(release_body))
        .mount(&server)
        .await;

    let updater = make_updater(&server.uri(), "v0.0.1", tmp.path());
    attach_updater(&app, updater);

    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/updates/check",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["hasUpdate"], false);
}

#[tokio::test]
async fn it_admin_updates_apply_rejects_version_mismatch() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    let updater = make_updater(&server.uri(), "v1.0.0", tmp.path());
    attach_updater(&app, updater);

    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/updates/apply",
        Some(serde_json::json!({
            "targetVersion": "v2.0.0",
            "confirmCurrentVersion": "v0.0.1"  // 与实际不匹配
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "CURRENT_VERSION_MISMATCH");
}

#[tokio::test]
async fn it_admin_updates_apply_propagates_no_asset_error() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;

    // assets 列表为空 → NoAsset
    let release_body = serde_json::json!({
        "tag_name": "v9.9.9",
        "published_at": "2030-01-01T00:00:00Z",
        "html_url": format!("{}/release", server.uri()),
        "body": "",
        "assets": []
    });
    Mock::given(method("GET"))
        .and(path("/repos/o/r/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(release_body))
        .mount(&server)
        .await;

    let updater = make_updater(&server.uri(), "v0.0.1", tmp.path());
    attach_updater(&app, updater);

    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/updates/apply",
        Some(serde_json::json!({
            "targetVersion": "v9.9.9",
            "confirmCurrentVersion": "v0.0.1"
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    // v0.5.2 起 apply 异步化：handler 立即返回 202 ACCEPTED + taskId，
    // 真实 NoAsset 错误在后台 task 跑完后落到 applyTask.error 字段。
    assert_eq!(
        status,
        StatusCode::ACCEPTED,
        "expected 202 ACCEPTED, got {status} body={body}"
    );
    assert!(
        body["data"]["taskId"].is_string(),
        "should return taskId, body={body}"
    );

    // 轮询 /status 直到后台 task 完成（最多 5s）
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut final_error: Option<String> = None;
    while std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let resp = request(
            &app.app,
            Method::GET,
            "/api/admin/updates/status",
            None,
            &[("authorization", auth_header(&token))],
        )
        .await;
        let (_, _, body) = response_json(resp).await;
        let task = &body["data"]["applyTask"];
        if !task.is_null() && task["completedAt"].is_string() {
            final_error = task["error"].as_str().map(String::from);
            break;
        }
    }
    let err_msg = final_error.expect("apply task should have completed with error");
    assert!(
        err_msg.contains("asset") || err_msg.contains("target"),
        "expected NoAsset/InvalidTarget error in applyTask.error, got {err_msg}"
    );
}
