//! REGION 覆盖补充：updates / feedback / broadcast / settings / monitoring 的错误/边界分支。
//!
//! 专攻已有 cov_* / *_extra 测试未覆盖的错误臂：
//!   updates:   rollback / changelog 的 UPDATER_DISABLED(503)、CURRENT_VERSION_MISMATCH(400)、
//!              备份 list/create happy、restore BACKUP_NOT_FOUND(400)、download 404、各端点 401。
//!   feedback:  公告 CRUD(INVALID_KIND/INVALID_TITLE/INVALID_BODY/404)、reply INVALID_REPLY(400)/404、
//!              assign/resolve/merge 404、draft 404、github-issue GITHUB_NOT_CONFIGURED(409)。
//!   broadcast: INVALID_VERSION_MIN(400)、INVALID_SCHEDULED_AT(400)、EMPTY_AUDIENCE(400)、
//!              cancel_scheduled ALREADY_DISPATCHED(409)、preview happy、scheduled list happy。
//!   settings:  version-gate INVALID_MIN_CLIENT_VERSION(400)、config UNKNOWN_SECTION(400)/
//!              INVALID_SECTION_BODY(400)、snapshot LABEL_TOO_LONG(400)/restore 404、canary 阈值 400、
//!              export.toml / backup-status happy、各端点 401。
//!   monitoring:dead-letter requeue/purge 404、events/logs/requests window 边界、各端点 401。

mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token, setup_admin_and_get_token};
use common::http::{request, response_json};

fn h(token: &str) -> [(&'static str, String); 1] {
    [("authorization", auth_header(token))]
}

// ════════════════════════════ updates ════════════════════════════

#[tokio::test]
async fn updates_rollback_returns_503_when_updater_disabled() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/updates/rollback",
        Some(serde_json::json!({
            "channel": "stable",
            "targetVersion": "1.0.0",
            "confirmCurrentVersion": "1.1.0"
        })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "body={body}");
    assert_eq!(body["code"], "UPDATER_DISABLED");
}

#[tokio::test]
async fn updates_changelog_returns_503_when_updater_disabled() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/updates/changelog",
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "body={body}");
    assert_eq!(body["code"], "UPDATER_DISABLED");
}

#[tokio::test]
async fn updates_rollback_malformed_body_returns_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // channel 缺失 → JsonBody 反序列化失败 → 400 INVALID_REQUEST_BODY
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/updates/rollback",
        Some(serde_json::json!({ "targetVersion": "1.0.0" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["code"], "INVALID_REQUEST_BODY");
}

#[tokio::test]
async fn updates_rollback_invalid_channel_enum_returns_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/updates/rollback",
        Some(serde_json::json!({
            "channel": "nightly",
            "targetVersion": "1.0.0",
            "confirmCurrentVersion": "1.1.0"
        })),
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn updates_backups_list_returns_envelope() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/updates/backups",
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert!(body["data"]["backups"].is_array());
    assert_eq!(body["data"]["thresholdBytes"], 10_737_418_240u64);
}

#[tokio::test]
async fn updates_create_backup_then_appears_in_list() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // 测试 DB 落在 tempdir 真实磁盘，故 create_backup 走成功路径（非 NO_DATA_DIR）。
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/updates/backups",
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["data"]["kind"], "manual");
    let name = body["data"]["name"].as_str().expect("backup name").to_string();
    assert!(name.starts_with("backup-manual-"));

    // 列表能看到刚建的备份
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/updates/backups",
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let backups = body["data"]["backups"].as_array().expect("backups array");
    assert!(backups.iter().any(|b| b["name"] == name));
}

#[tokio::test]
async fn updates_restore_nonexistent_backup_returns_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/updates/backups/no-such-backup.db/restore",
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["code"], "BACKUP_NOT_FOUND");
}

#[tokio::test]
async fn updates_restore_path_traversal_name_rejected() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // 含 .. 的文件名被 resolve_backup 拒绝 → BACKUP_NOT_FOUND。
    // 用 %2e%2e 编码以免被路由层规范化吃掉，落到 handler 仍是非法名。
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/updates/backups/%2e%2e%2fevil.db/restore",
        None,
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    // 非法名 → 400，或路由不匹配 → 404；两者都说明未越权恢复。
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::NOT_FOUND,
        "got {status}"
    );
}

#[tokio::test]
async fn updates_download_nonexistent_backup_returns_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/updates/backups/no-such-backup.db/download",
        None,
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn updates_endpoints_require_admin_token() {
    let app = spawn_test_server().await;
    for (m, path) in [
        (Method::POST, "/api/admin/updates/rollback"),
        (Method::GET, "/api/admin/updates/changelog"),
        (Method::GET, "/api/admin/updates/backups"),
        (Method::POST, "/api/admin/updates/backups"),
    ] {
        let resp = request(&app.app, m.clone(), path, None, &[]).await;
        let (status, _, _) = response_json(resp).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "path={path}");
    }
}

// ════════════════════════════ feedback ════════════════════════════

const FB: &str = "/api/admin/feedback";

#[tokio::test]
async fn feedback_announcement_invalid_kind_returns_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        &format!("{FB}/announcements"),
        Some(serde_json::json!({ "title": "t", "body": "b", "kind": "bogus" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["code"], "INVALID_KIND");
}

#[tokio::test]
async fn feedback_announcement_empty_title_returns_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        &format!("{FB}/announcements"),
        Some(serde_json::json!({ "title": "   ", "body": "b" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["code"], "INVALID_TITLE");
}

#[tokio::test]
async fn feedback_announcement_empty_body_returns_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        &format!("{FB}/announcements"),
        Some(serde_json::json!({ "title": "t", "body": "  " })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["code"], "INVALID_BODY");
}

#[tokio::test]
async fn feedback_announcement_create_list_then_update_and_delete() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // create（默认草稿）
    let resp = request(
        &app.app,
        Method::POST,
        &format!("{FB}/announcements"),
        Some(serde_json::json!({ "title": "公告标题", "body": "公告正文", "kind": "faq" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CREATED, "body={body}");
    let id = body["data"]["id"].as_str().expect("announcement id").to_string();

    // list 带 kind 过滤
    let resp = request(
        &app.app,
        Method::GET,
        &format!("{FB}/announcements?kind=faq"),
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["data"].is_array());

    // list 非法 kind → 400 INVALID_KIND
    let resp = request(
        &app.app,
        Method::GET,
        &format!("{FB}/announcements?kind=zzz"),
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_KIND");

    // update：空标题 → 400
    let resp = request(
        &app.app,
        Method::PATCH,
        &format!("{FB}/announcements/{id}"),
        Some(serde_json::json!({ "title": "  " })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_TITLE");

    // update：发布
    let resp = request(
        &app.app,
        Method::PATCH,
        &format!("{FB}/announcements/{id}"),
        Some(serde_json::json!({ "published": true })),
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);

    // delete
    let resp = request(
        &app.app,
        Method::DELETE,
        &format!("{FB}/announcements/{id}"),
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["deleted"], true);
}

#[tokio::test]
async fn feedback_announcement_update_nonexistent_returns_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::PATCH,
        &format!("{FB}/announcements/no-such-id"),
        Some(serde_json::json!({ "published": true })),
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn feedback_announcement_delete_nonexistent_returns_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::DELETE,
        &format!("{FB}/announcements/no-such-id"),
        None,
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn feedback_reply_empty_body_returns_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        &format!("{FB}/some-id/replies"),
        Some(serde_json::json!({ "body": "   " })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["code"], "INVALID_REPLY");
}

#[tokio::test]
async fn feedback_reply_nonexistent_ticket_returns_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        &format!("{FB}/no-such-ticket/replies"),
        Some(serde_json::json!({ "body": "回复内容" })),
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn feedback_assign_nonexistent_returns_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        &format!("{FB}/no-such/assign"),
        Some(serde_json::json!({ "assigneeAdminId": null })),
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn feedback_resolve_nonexistent_returns_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        &format!("{FB}/no-such/resolve"),
        Some(serde_json::json!({ "resolution": "已处理" })),
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn feedback_merge_nonexistent_returns_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        &format!("{FB}/no-such/merge"),
        Some(serde_json::json!({ "targetId": "also-missing" })),
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn feedback_save_draft_nonexistent_returns_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        &format!("{FB}/no-such/draft"),
        Some(serde_json::json!({ "body": "草稿" })),
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn feedback_get_draft_nonexistent_returns_null_draft() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::GET,
        &format!("{FB}/no-such/draft"),
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    // get_reply_draft 对不存在工单返回 draft=null，不报 404。
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert!(body["data"]["draft"].is_null());
}

#[tokio::test]
async fn feedback_github_issue_not_configured_returns_409() {
    // 注意：本测试依赖未设置 GITHUB_TOKEN / FEEDBACK_GITHUB_REPO。
    // 测试环境默认不带这些 env，故走 GITHUB_NOT_CONFIGURED 分支。
    if std::env::var("GITHUB_TOKEN").is_ok() && std::env::var("FEEDBACK_GITHUB_REPO").is_ok() {
        return; // 环境已配置则跳过（避免误打真实 API）
    }
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        &format!("{FB}/some-id/github-issue"),
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CONFLICT, "body={body}");
    assert_eq!(body["code"], "GITHUB_NOT_CONFIGURED");
}

#[tokio::test]
async fn feedback_get_detail_nonexistent_returns_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::GET,
        &format!("{FB}/no-such-detail"),
        None,
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn feedback_export_csv_returns_csv_header() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::GET,
        &format!("{FB}/export.csv"),
        None,
        &h(&token),
    )
    .await;
    let status = resp.status();
    assert_eq!(status, StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("text/csv"), "content-type={ct}");
}

#[tokio::test]
async fn feedback_announcements_require_admin_token() {
    let app = spawn_test_server().await;
    let resp = request(
        &app.app,
        Method::GET,
        &format!("{FB}/announcements"),
        None,
        &[],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ════════════════════════════ broadcast ════════════════════════════

const BC: &str = "/api/admin/broadcast";

#[tokio::test]
async fn broadcast_invalid_version_min_returns_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        BC,
        Some(serde_json::json!({
            "title": "标题",
            "message": "正文",
            "audience": { "versionMin": "not-a-semver" }
        })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["code"], "INVALID_VERSION_MIN");
}

#[tokio::test]
async fn broadcast_accepts_version_min_with_v_prefix() {
    // version_min 允许前导 'v'（trim_start_matches('v')）→ 合法 semver → 不因校验报 400。
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // 无任何用户 + 受众过滤 → 命中零 → EMPTY_AUDIENCE 400（而非 INVALID_VERSION_MIN）。
    let resp = request(
        &app.app,
        Method::POST,
        BC,
        Some(serde_json::json!({
            "title": "标题",
            "message": "正文",
            "audience": { "versionMin": "v1.2.3" }
        })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["code"], "EMPTY_AUDIENCE");
}

#[tokio::test]
async fn broadcast_invalid_scheduled_at_returns_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        BC,
        Some(serde_json::json!({
            "title": "标题",
            "message": "正文",
            "scheduledAt": "not-a-time"
        })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["code"], "INVALID_SCHEDULED_AT");
}

#[tokio::test]
async fn broadcast_empty_audience_match_returns_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // platforms 过滤产生 Some(列表)：库里无任何 client_devices → 命中零 → EMPTY_AUDIENCE。
    // 注意：explicit userIds 走并集（即便不存在也计入），故不能用它触发空受众；
    // 用 platforms 维度才会得到「已应用过滤但零命中」的真实空集。
    let resp = request(
        &app.app,
        Method::POST,
        BC,
        Some(serde_json::json!({
            "title": "标题",
            "message": "正文",
            "audience": { "platforms": ["ios"] }
        })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["code"], "EMPTY_AUDIENCE");
}

#[tokio::test]
async fn broadcast_future_scheduled_at_enqueues() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let future = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
    let resp = request(
        &app.app,
        Method::POST,
        BC,
        Some(serde_json::json!({
            "title": "排程标题",
            "message": "排程正文",
            "scheduledAt": future
        })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["data"]["scheduled"], true);
    let bid = body["data"]["broadcastId"].as_str().expect("broadcastId");

    // scheduled 列表能看到这条
    let resp = request(&app.app, Method::GET, &format!("{BC}/scheduled"), None, &h(&token)).await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let items = body["data"]["items"].as_array().expect("items");
    assert!(items.iter().any(|i| i["id"] == bid));
}

#[tokio::test]
async fn broadcast_cancel_nonexistent_scheduled_returns_409() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::DELETE,
        &format!("{BC}/scheduled/no-such-id"),
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CONFLICT, "body={body}");
    assert_eq!(body["code"], "ALREADY_DISPATCHED");
}

#[tokio::test]
async fn broadcast_preview_no_audience_returns_total_as_matched() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        &format!("{BC}/preview"),
        Some(serde_json::json!({})),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    // 无受众 → matched == total（此处都为 0，因无用户）
    assert_eq!(body["data"]["matched"], body["data"]["total"]);
}

#[tokio::test]
async fn broadcast_scheduled_list_requires_admin_token() {
    let app = spawn_test_server().await;
    let resp = request(&app.app, Method::GET, &format!("{BC}/scheduled"), None, &[]).await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn broadcast_message_malformed_body_returns_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // title 类型错误 → JsonBody 反序列化失败 → 400。
    let resp = request(
        &app.app,
        Method::POST,
        BC,
        Some(serde_json::json!({ "title": 123, "message": "x" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["code"], "INVALID_REQUEST_BODY");
}

// ════════════════════════════ settings ════════════════════════════

const ST: &str = "/api/admin/settings";

#[tokio::test]
async fn settings_version_gate_invalid_semver_returns_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::PUT,
        &format!("{ST}/version-gate"),
        Some(serde_json::json!({ "minClientVersion": "abc" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["code"], "INVALID_MIN_CLIENT_VERSION");
}

#[tokio::test]
async fn settings_version_gate_get_and_set_happy() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // GET 当前门控配置
    let resp = request(
        &app.app,
        Method::GET,
        &format!("{ST}/version-gate"),
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert!(body["data"].get("enabled").is_some());

    // PUT 合法 semver + 启用 → 200
    let resp = request(
        &app.app,
        Method::PUT,
        &format!("{ST}/version-gate"),
        Some(serde_json::json!({ "enabled": true, "minClientVersion": "1.2.0" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["data"]["enabled"], true);
    assert_eq!(body["data"]["minClientVersion"], "1.2.0");

    // PUT 空串 minClientVersion → 清空（回落 env），仍 200
    let resp = request(
        &app.app,
        Method::PUT,
        &format!("{ST}/version-gate"),
        Some(serde_json::json!({ "minClientVersion": "" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert!(body["data"]["minClientVersion"].is_null());
}

#[tokio::test]
async fn settings_config_put_unknown_section_returns_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::PUT,
        &format!("{ST}/config/bogus-section"),
        Some(serde_json::json!({ "foo": "bar" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["code"], "UNKNOWN_SECTION");
}

#[tokio::test]
async fn settings_config_put_non_object_body_returns_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // 已知 section 但 body 是数组而非对象 → INVALID_SECTION_BODY。
    let resp = request(
        &app.app,
        Method::PUT,
        &format!("{ST}/config/roles"),
        Some(serde_json::json!([1, 2, 3])),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["code"], "INVALID_SECTION_BODY");
}

#[tokio::test]
async fn settings_config_put_known_passthrough_section_ok() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // roles 是裸 JSON 透传 section，合法对象 → 200。
    let resp = request(
        &app.app,
        Method::PUT,
        &format!("{ST}/config/roles"),
        Some(serde_json::json!({ "customRole": "reviewer" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");

    // GET 全部 config
    let resp = request(&app.app, Method::GET, &format!("{ST}/config"), None, &h(&token)).await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["sections"].is_array());
}

#[tokio::test]
async fn settings_snapshot_label_too_long_returns_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let long_label = "x".repeat(201);
    let resp = request(
        &app.app,
        Method::POST,
        &format!("{ST}/snapshots"),
        Some(serde_json::json!({ "label": long_label })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["code"], "LABEL_TOO_LONG");
}

#[tokio::test]
async fn settings_snapshot_create_list_then_restore_roundtrip() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // create
    let resp = request(
        &app.app,
        Method::POST,
        &format!("{ST}/snapshots"),
        Some(serde_json::json!({ "label": "测试快照" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CREATED, "body={body}");
    let id = body["data"]["id"].as_str().expect("snapshot id").to_string();

    // list
    let resp = request(&app.app, Method::GET, &format!("{ST}/snapshots"), None, &h(&token)).await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["snapshots"].is_array());

    // restore 真实 id → 200
    let resp = request(
        &app.app,
        Method::POST,
        &format!("{ST}/snapshots/{id}/restore"),
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert!(body["data"]["sections"].is_array());
}

#[tokio::test]
async fn settings_snapshot_restore_nonexistent_returns_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        &format!("{ST}/snapshots/no-such-snapshot/restore"),
        None,
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn settings_canary_thresholds_out_of_range_return_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    for field in ["canaryRewardDropThreshold", "canaryAnomalyRiseThreshold"] {
        let resp = request(
            &app.app,
            Method::PUT,
            ST,
            Some(serde_json::json!({ field: 1.5 })),
            &h(&token),
        )
        .await;
        let (status, _, body) = response_json(resp).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "field={field} body={body}");
        assert_eq!(body["code"], "INVALID_CANARY_THRESHOLD", "field={field}");
    }
}

#[tokio::test]
async fn settings_update_invalid_min_client_version_returns_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::PUT,
        ST,
        Some(serde_json::json!({ "minClientVersion": "not-semver" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["code"], "INVALID_MIN_CLIENT_VERSION");
}

#[tokio::test]
async fn settings_export_toml_returns_text_plain() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(&app.app, Method::GET, &format!("{ST}/export.toml"), None, &h(&token)).await;
    let status = resp.status();
    assert_eq!(status, StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("text/plain"), "content-type={ct}");
}

#[tokio::test]
async fn settings_backup_status_returns_targets() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(&app.app, Method::GET, &format!("{ST}/backup-status"), None, &h(&token)).await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert!(body["data"]["targets"].is_array());
}

#[tokio::test]
async fn settings_endpoints_require_admin_token() {
    let app = spawn_test_server().await;
    for (m, path) in [
        (Method::GET, format!("{ST}/version-gate")),
        (Method::PUT, format!("{ST}/version-gate")),
        (Method::GET, format!("{ST}/config")),
        (Method::GET, format!("{ST}/snapshots")),
        (Method::GET, format!("{ST}/export.toml")),
        (Method::GET, format!("{ST}/backup-status")),
    ] {
        let resp = request(&app.app, m.clone(), &path, None, &[]).await;
        let (status, _, _) = response_json(resp).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "path={path}");
    }
}

#[tokio::test]
async fn settings_version_gate_rejects_non_admin_user() {
    let app = spawn_test_server().await;
    let user_token = login_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::GET,
        &format!("{ST}/version-gate"),
        None,
        &h(&user_token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    // 普通用户 token 不是 admin token → 401。
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ════════════════════════════ monitoring ════════════════════════════

const MON: &str = "/api/admin/monitoring";

#[tokio::test]
async fn monitoring_dead_letter_requeue_nonexistent_returns_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        &format!("{MON}/dead-letter/999999/requeue"),
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "body={body}");
}

#[tokio::test]
async fn monitoring_dead_letter_purge_nonexistent_returns_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::DELETE,
        &format!("{MON}/dead-letter/999999"),
        None,
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn monitoring_dead_letter_requeue_non_integer_id_returns_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // Path<i64> 解析失败 → 400（路径段非数字）。
    let resp = request(
        &app.app,
        Method::POST,
        &format!("{MON}/dead-letter/not-a-number/requeue"),
        None,
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn monitoring_dead_letter_list_empty_default_path() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::GET,
        &format!("{MON}/dead-letter"),
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    // 默认同步老路：死信表恒空。
    assert_eq!(body["data"]["entries"].as_array().map(|a| a.len()), Some(0));
}

#[tokio::test]
async fn monitoring_events_window_clamped_and_returns_events_array() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // hours 越界（>168）被 clamp，仍 200。
    let resp = request(
        &app.app,
        Method::GET,
        &format!("{MON}/events?hours=99999"),
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert!(body["data"]["events"].is_array());
}

#[tokio::test]
async fn monitoring_logs_with_level_filter_and_clamped_limit() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // limit 越界被 clamp 到 1000，level 过滤分支。
    let resp = request(
        &app.app,
        Method::GET,
        &format!("{MON}/logs?limit=99999&level=WARN"),
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert!(body["data"]["logs"].is_array());
}

#[tokio::test]
async fn monitoring_requests_unknown_window_falls_back() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // window 未识别 → 回退 1h，仍 200。
    let resp = request(
        &app.app,
        Method::GET,
        &format!("{MON}/requests?window=zzz"),
        None,
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn monitoring_requests_each_known_window_ok() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    for w in ["15m", "1h", "6h", "24h", "7d"] {
        let resp = request(
            &app.app,
            Method::GET,
            &format!("{MON}/requests?window={w}"),
            None,
            &h(&token),
        )
        .await;
        let (status, _, _) = response_json(resp).await;
        assert_eq!(status, StatusCode::OK, "window={w}");
    }
}

#[tokio::test]
async fn monitoring_dead_letter_endpoints_require_admin_token() {
    let app = spawn_test_server().await;
    for (m, path) in [
        (Method::GET, format!("{MON}/dead-letter")),
        (Method::POST, format!("{MON}/dead-letter/1/requeue")),
        (Method::DELETE, format!("{MON}/dead-letter/1")),
        (Method::GET, format!("{MON}/events")),
        (Method::GET, format!("{MON}/logs")),
        (Method::GET, format!("{MON}/requests")),
    ] {
        let resp = request(&app.app, m.clone(), &path, None, &[]).await;
        let (status, _, _) = response_json(resp).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "path={path}");
    }
}
