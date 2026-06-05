//! REGION 覆盖补充：wordbook_store_ops 簇。
//!
//! 专攻已有 happy-path 测试未触及的错误/边界分支，连带覆盖对应 store ops：
//!
//! 1. admin wordbook-center /upload —— UploadWordbookRequest 校验分支
//!    (WB_UPLOAD_INVALID 空 id/name、WB_UPLOAD_EMPTY 空 words)、happy 落库 +
//!    携带 tags 写 wordbook_local_tags、401。经 persist_remote_wordbook_import
//!    → import_remote_wordbook_atomic 走完整 store 写入。
//! 2. admin wordbook-center PATCH /:id/tags —— replace / add+remove 两路径，
//!    经 set_/add_/remove_/list_wordbook_local_tags。
//! 3. admin probe-telemetry PATCH /sampling/:event_type —— upsert_sampling_rule
//!    的 EMPTY_PATCH(400)、SAMPLING_RATE_OUT_OF_RANGE(400)、add 新规则(200)、
//!    enabled-only pause(200)、401。
//! 4. user feedback POST /api/feedback —— create_feedback handler 的 INVALID_FEEDBACK
//!    各校验臂(空 body / body 超长 / category 超长 / route 超长 / CSAT 越界 /
//!    附件过多 / 附件名超长 / 附件 URL 超长 / device_profile 过大 / answer_snapshot
//!    过大)+ happy(经 create_feedback store op)+ 401。

mod common;

use axum::http::{Method, StatusCode};
use axum::Router;

use common::app::spawn_test_server;
use common::auth::{auth_header, setup_admin_and_get_token};
use common::http::{request, response_json};

// ── 注册一个用户并拿到 token ──
async fn register_user(app: &Router) -> String {
    let email = format!("wso-{}@test.com", uuid::Uuid::new_v4());
    let username = format!("wso-{}", uuid::Uuid::new_v4().simple());
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
    body["data"]["accessToken"].as_str().unwrap().to_string()
}

// ════════════════════ admin wordbook-center /upload ════════════════════

#[tokio::test]
async fn admin_upload_missing_id_or_name_400() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    let h = [("authorization", auth_header(&admin))];

    // 空 id
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::POST,
            "/api/admin/wordbook-center/upload",
            Some(serde_json::json!({
                "id": "",
                "name": "Book",
                "words": [{ "spelling": "apple", "meanings": ["苹果"] }],
            })),
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["code"], "WB_UPLOAD_INVALID");

    // 空 name
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::POST,
            "/api/admin/wordbook-center/upload",
            Some(serde_json::json!({
                "id": "bk-1",
                "name": "",
                "words": [{ "spelling": "apple", "meanings": ["苹果"] }],
            })),
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["code"], "WB_UPLOAD_INVALID");
}

#[tokio::test]
async fn admin_upload_empty_words_400() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    let h = [("authorization", auth_header(&admin))];

    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::POST,
            "/api/admin/wordbook-center/upload",
            Some(serde_json::json!({
                "id": "bk-empty",
                "name": "Empty Book",
                "words": [],
            })),
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["code"], "WB_UPLOAD_EMPTY");
}

#[tokio::test]
async fn admin_upload_happy_persists_with_tags() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    let h = [("authorization", auth_header(&admin))];

    // 含 1 个空 spelling(会被 skip)+ 2 个有效词 + 携带 tags
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::POST,
            "/api/admin/wordbook-center/upload",
            Some(serde_json::json!({
                "id": "upload-ok-1",
                "name": "上传词书",
                "description": "desc",
                "version": "2.0",
                "tags": ["手动上传", "测试"],
                "words": [
                    { "spelling": "apple", "phonetic": "ˈæpl", "meanings": ["苹果"], "examples": ["an apple"] },
                    { "spelling": "banana", "meanings": ["香蕉"] },
                    { "spelling": "   ", "meanings": ["空白被跳过"] },
                ],
            })),
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "body={b}");
    assert_eq!(b["data"]["wordsImported"], 2);
    assert_eq!(b["data"]["wordsSkipped"], 1);
    let wordbook_id = b["data"]["wordbook"]["id"].as_str().unwrap().to_string();
    assert_eq!(b["data"]["wordbook"]["name"], "上传词书");

    // upload 携带的 tags 应已落 wordbook_local_tags（best-effort 写入）
    let tags = app
        .state
        .store()
        .list_wordbook_local_tags(&wordbook_id)
        .unwrap();
    assert!(tags.contains(&"手动上传".to_string()));
    assert!(tags.contains(&"测试".to_string()));
}

#[tokio::test]
async fn admin_upload_unauthorized_401() {
    let app = spawn_test_server().await;
    let (s, _, _) = response_json(
        request(
            &app.app,
            Method::POST,
            "/api/admin/wordbook-center/upload",
            Some(serde_json::json!({ "id": "x", "name": "y", "words": [] })),
            &[],
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

// ════════════════════ admin wordbook-center PATCH /:id/tags ════════════════════

#[tokio::test]
async fn admin_patch_tags_replace_then_add_remove() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    let h = [("authorization", auth_header(&admin))];

    // 先 upload 一个词书拿到本地 wordbook_id
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::POST,
            "/api/admin/wordbook-center/upload",
            Some(serde_json::json!({
                "id": "tag-target-1",
                "name": "标签目标",
                "words": [{ "spelling": "word", "meanings": ["词"] }],
            })),
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "body={b}");
    let wid = b["data"]["wordbook"]["id"].as_str().unwrap().to_string();

    // replace：整体替换为 [a, b]
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::PATCH,
            &format!("/api/admin/wordbook-center/{wid}/tags"),
            Some(serde_json::json!({ "replace": ["alpha", "beta"] })),
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "body={b}");
    let tags: Vec<String> = b["data"]["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(tags, vec!["alpha".to_string(), "beta".to_string()]); // list 按 tag ASC

    // add + remove：加 gamma、删 alpha → 结果 {beta, gamma}
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::PATCH,
            &format!("/api/admin/wordbook-center/{wid}/tags"),
            Some(serde_json::json!({ "add": ["gamma"], "remove": ["alpha"] })),
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "body={b}");
    let tags: Vec<String> = b["data"]["tags"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(tags, vec!["beta".to_string(), "gamma".to_string()]);
}

#[tokio::test]
async fn admin_patch_tags_empty_body_is_noop_ok() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    let h = [("authorization", auth_header(&admin))];

    // 未导入的随机 wordbook_id + 空 body（add/remove 均空、无 replace）
    // → 不报错，返回该词书当前空标签列表。
    let wid = uuid::Uuid::new_v4().to_string();
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::PATCH,
            &format!("/api/admin/wordbook-center/{wid}/tags"),
            Some(serde_json::json!({})),
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "body={b}");
    assert!(b["data"]["tags"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn admin_patch_tags_unauthorized_401() {
    let app = spawn_test_server().await;
    let (s, _, _) = response_json(
        request(
            &app.app,
            Method::PATCH,
            "/api/admin/wordbook-center/some-id/tags",
            Some(serde_json::json!({ "replace": ["x"] })),
            &[],
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

// ════════════════════ admin probe-telemetry PATCH /sampling/:event_type ════════════════════

#[tokio::test]
async fn patch_sampling_empty_patch_400() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    let h = [("authorization", auth_header(&admin))];

    // 既无 sampleRate 也无 enabled → EMPTY_PATCH
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::PATCH,
            "/api/admin/probe-telemetry/sampling/periodic",
            Some(serde_json::json!({})),
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["code"], "EMPTY_PATCH");
}

#[tokio::test]
async fn patch_sampling_rate_out_of_range_400() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    let h = [("authorization", auth_header(&admin))];

    // rate > 1.0 → upsert_sampling_rule 返回 Validation(SAMPLING_RATE_OUT_OF_RANGE) → 400
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::PATCH,
            "/api/admin/probe-telemetry/sampling/periodic",
            Some(serde_json::json!({ "sampleRate": 2.0 })),
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["code"], "SAMPLING_RATE_OUT_OF_RANGE");

    // rate < 0.0 同样越界
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::PATCH,
            "/api/admin/probe-telemetry/sampling/periodic",
            Some(serde_json::json!({ "sampleRate": -0.5 })),
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(b["code"], "SAMPLING_RATE_OUT_OF_RANGE");
}

#[tokio::test]
async fn patch_sampling_add_new_rule_200() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    let h = [("authorization", auth_header(&admin))];

    // 未 seed 的全新 event_type → upsert 走 'add' 分支，priority 默认 100、locked=0
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::PATCH,
            "/api/admin/probe-telemetry/sampling/brand_new_event",
            Some(serde_json::json!({ "sampleRate": 0.3 })),
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "body={b}");
    assert_eq!(b["data"]["eventType"], "brand_new_event");
    assert!((b["data"]["sampleRate"].as_f64().unwrap() - 0.3).abs() < 1e-9);
    assert!(!b["data"]["locked"].as_bool().unwrap());
}

#[tokio::test]
async fn patch_sampling_enabled_only_pause_200() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    let h = [("authorization", auth_header(&admin))];

    // 仅改 enabled=false（无 rate）→ 'pause' audit 分支，非 locked 行可改
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::PATCH,
            "/api/admin/probe-telemetry/sampling/periodic",
            Some(serde_json::json!({ "enabled": false })),
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "body={b}");
    assert!(!b["data"]["enabled"].as_bool().unwrap());
}

#[tokio::test]
async fn patch_sampling_unauthorized_401() {
    let app = spawn_test_server().await;
    let (s, _, _) = response_json(
        request(
            &app.app,
            Method::PATCH,
            "/api/admin/probe-telemetry/sampling/periodic",
            Some(serde_json::json!({ "sampleRate": 0.5 })),
            &[],
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

// ════════════════════ user feedback POST /api/feedback 校验分支 ════════════════════

/// 统一断言：POST feedback 期望 400 INVALID_FEEDBACK。
async fn assert_feedback_invalid(app: &Router, token: &str, payload: serde_json::Value) {
    let h = [("authorization", auth_header(token))];
    let (s, _, b) = response_json(
        request(app, Method::POST, "/api/feedback", Some(payload), &h).await,
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "body={b}");
    assert_eq!(b["code"], "INVALID_FEEDBACK");
}

#[tokio::test]
async fn feedback_empty_body_400() {
    let app = spawn_test_server().await;
    let token = register_user(&app.app).await;
    // 纯空白 body → trim 后为空
    assert_feedback_invalid(&app.app, &token, serde_json::json!({ "body": "   " })).await;
}

#[tokio::test]
async fn feedback_body_too_long_400() {
    let app = spawn_test_server().await;
    let token = register_user(&app.app).await;
    let long = "a".repeat(5_001); // > MAX_FEEDBACK_BODY_CHARS(5000)
    assert_feedback_invalid(&app.app, &token, serde_json::json!({ "body": long })).await;
}

#[tokio::test]
async fn feedback_category_too_long_400() {
    let app = spawn_test_server().await;
    let token = register_user(&app.app).await;
    let cat = "c".repeat(65); // > MAX_FEEDBACK_CATEGORY_CHARS(64)
    assert_feedback_invalid(
        &app.app,
        &token,
        serde_json::json!({ "body": "ok", "category": cat }),
    )
    .await;
}

#[tokio::test]
async fn feedback_route_too_long_400() {
    let app = spawn_test_server().await;
    let token = register_user(&app.app).await;
    let route = "/".repeat(201); // > MAX_FEEDBACK_ROUTE_CHARS(200)
    assert_feedback_invalid(
        &app.app,
        &token,
        serde_json::json!({ "body": "ok", "route": route }),
    )
    .await;
}

#[tokio::test]
async fn feedback_csat_out_of_range_400() {
    let app = spawn_test_server().await;
    let token = register_user(&app.app).await;
    // CSAT > 5
    assert_feedback_invalid(
        &app.app,
        &token,
        serde_json::json!({ "body": "ok", "csatScore": 6 }),
    )
    .await;
    // CSAT < 1
    assert_feedback_invalid(
        &app.app,
        &token,
        serde_json::json!({ "body": "ok", "csatScore": 0 }),
    )
    .await;
}

#[tokio::test]
async fn feedback_too_many_attachments_400() {
    let app = spawn_test_server().await;
    let token = register_user(&app.app).await;
    let attachments: Vec<serde_json::Value> = (0..11) // > MAX_FEEDBACK_ATTACHMENTS(10)
        .map(|i| serde_json::json!({ "name": format!("a{i}.png"), "url": "https://x/y.png" }))
        .collect();
    assert_feedback_invalid(
        &app.app,
        &token,
        serde_json::json!({ "body": "ok", "attachments": attachments }),
    )
    .await;
}

#[tokio::test]
async fn feedback_attachment_name_too_long_400() {
    let app = spawn_test_server().await;
    let token = register_user(&app.app).await;
    let name = "n".repeat(201); // > MAX_FEEDBACK_ATTACHMENT_NAME_CHARS(200)
    assert_feedback_invalid(
        &app.app,
        &token,
        serde_json::json!({
            "body": "ok",
            "attachments": [{ "name": name, "url": "https://x/y.png" }],
        }),
    )
    .await;
}

#[tokio::test]
async fn feedback_attachment_url_too_long_400() {
    let app = spawn_test_server().await;
    let token = register_user(&app.app).await;
    let url = format!("https://x/{}", "u".repeat(2_001)); // > MAX_FEEDBACK_ATTACHMENT_URL_CHARS(2000)
    assert_feedback_invalid(
        &app.app,
        &token,
        serde_json::json!({
            "body": "ok",
            "attachments": [{ "name": "ok.png", "url": url }],
        }),
    )
    .await;
}

#[tokio::test]
async fn feedback_device_profile_too_large_400() {
    let app = spawn_test_server().await;
    let token = register_user(&app.app).await;
    // device_profile 序列化后 > 8KB
    let big = "x".repeat(9 * 1024);
    assert_feedback_invalid(
        &app.app,
        &token,
        serde_json::json!({ "body": "ok", "deviceProfile": { "blob": big } }),
    )
    .await;
}

#[tokio::test]
async fn feedback_answer_snapshot_too_large_400() {
    let app = spawn_test_server().await;
    let token = register_user(&app.app).await;
    let big = "y".repeat(9 * 1024);
    assert_feedback_invalid(
        &app.app,
        &token,
        serde_json::json!({ "body": "ok", "answerSnapshot": { "blob": big } }),
    )
    .await;
}

#[tokio::test]
async fn feedback_happy_path_persists() {
    let app = spawn_test_server().await;
    let token = register_user(&app.app).await;
    let h = [("authorization", auth_header(&token))];

    // 携带可选字段的完整 happy path（经 create_feedback store op：item + 附件 + submitted 事件）
    let (s, _, b) = response_json(
        request(
            &app.app,
            Method::POST,
            "/api/feedback",
            Some(serde_json::json!({
                "body": "  这是一条有效反馈  ",
                "category": "bug",
                "route": "/learning",
                "csatScore": 4,
                "csatComment": "还行",
                "deviceProfile": { "platform": "ios", "appVersion": "1.2.3" },
                "answerSnapshot": { "lastWord": "apple" },
                "attachments": [{ "name": "shot.png", "url": "https://cdn/x.png", "kind": "image" }],
            })),
            &h,
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "body={b}");
    assert_eq!(b["success"], true);
    // body 已 trim
    assert_eq!(b["data"]["body"], "这是一条有效反馈");
    assert_eq!(b["data"]["category"], "bug");
    assert_eq!(b["data"]["status"], "open");
    assert_eq!(b["data"]["priority"], "normal");
    let fid = b["data"]["id"].as_str().unwrap();

    // store 侧应能读回该工单详情（连带覆盖 get_feedback_detail / 附件 / 事件读取）
    let detail = app.state.store().get_feedback_detail(fid).unwrap().unwrap();
    assert_eq!(detail.item.body, "这是一条有效反馈");
    assert_eq!(detail.attachments.len(), 1);
    assert_eq!(detail.attachments[0].name, "shot.png");
    // create_feedback 会写一条 'submitted' 事件
    assert!(detail.events.iter().any(|e| e.kind == "submitted"));
}

#[tokio::test]
async fn feedback_unauthorized_401() {
    let app = spawn_test_server().await;
    let (s, _, _) = response_json(
        request(
            &app.app,
            Method::POST,
            "/api/feedback",
            Some(serde_json::json!({ "body": "hi" })),
            &[],
        )
        .await,
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}
