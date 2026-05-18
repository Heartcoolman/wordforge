//! 后端 routes 补充集成测试：覆盖 status / users / word_favorites / word_notes /
//! word_states / study_config / notifications / wordbook_center 等模块的剩余路径。
//!
//! 注：wordbook_center 远程拉取因 SSRF 防护拒绝 localhost，只覆盖未配置 / 已配置但
//! 拉取失败 / 个人 URL 缺失 等错误路径；远程 happy path 由 acceptance/e2e 类测试承担。

mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token, setup_admin_and_get_token};
use common::http::{request, response_json};

async fn create_word(app: &axum::Router, token: &str, text: &str, meaning: &str) -> String {
    let resp = request(
        app,
        Method::POST,
        "/api/words",
        Some(serde_json::json!({
            "text": text,
            "meaning": meaning,
            "difficulty": 0.4,
            "tags": ["extra"]
        })),
        &[("authorization", auth_header(token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert!(status.is_success(), "create_word failed: {body}");
    body["data"]["id"].as_str().expect("word id").to_string()
}

// ────────────────────── status ──────────────────────

#[tokio::test]
async fn it_status_get_returns_version_and_maintenance() {
    let app = spawn_test_server().await;
    let resp = request(&app.app, Method::GET, "/api/status", None, &[]).await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["maintenanceMode"], false);
    assert!(body["data"]["version"].is_string());
}

#[tokio::test]
async fn it_status_device_ban_returns_false_for_unknown_device() {
    let app = spawn_test_server().await;
    let resp = request(
        &app.app,
        Method::GET,
        "/api/status/device-ban?deviceId=unknown-device",
        None,
        &[],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["banned"], false);
}

#[tokio::test]
async fn it_status_device_ban_missing_query_returns_400() {
    let app = spawn_test_server().await;
    let resp = request(&app.app, Method::GET, "/api/status/device-ban", None, &[]).await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ────────────────────── users ──────────────────────

#[tokio::test]
async fn it_users_change_password_full_flow() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    // 弱密码 -> 400
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/users/me/password",
        Some(serde_json::json!({
            "currentPassword": "Passw0rd!",
            "newPassword": "weak",
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "AUTH_WEAK_PASSWORD");

    // 当前密码错误 -> 401
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/users/me/password",
        Some(serde_json::json!({
            "currentPassword": "WrongPassw0rd!",
            "newPassword": "Newpassw0rd!!",
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // 正确流程 -> 200
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/users/me/password",
        Some(serde_json::json!({
            "currentPassword": "Passw0rd!",
            "newPassword": "Newpassw0rd!!",
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["passwordChanged"], true);
}

#[tokio::test]
async fn it_users_update_profile_username_validation() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    // 无效 username
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/users/me",
        Some(serde_json::json!({"username": "x"})),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "USER_INVALID_USERNAME");

    // 不传字段：返回当前用户
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/users/me",
        Some(serde_json::json!({})),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);

    // 合法 username
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/users/me",
        Some(serde_json::json!({"username": "valid_user_42"})),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["username"], "valid_user_42");
}

#[tokio::test]
async fn it_users_stats_no_records_returns_zero() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        Method::GET,
        "/api/users/me/stats",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["totalRecords"], 0);
    assert_eq!(body["data"]["accuracyRate"], 0.0);
}

#[tokio::test]
async fn it_users_unauthorized_paths() {
    let app = spawn_test_server().await;
    for path in [
        "/api/users/me",
        "/api/users/me/stats",
    ] {
        let resp = request(&app.app, Method::GET, path, None, &[]).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "path {path}");
    }
}

// ────────────────────── word_favorites ──────────────────────

#[tokio::test]
async fn it_word_favorites_full_flow() {
    let app = spawn_test_server().await;
    let user_token = login_and_get_token(&app.app).await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let word_id = create_word(&app.app, &admin_token, "favalpha", "收藏-A").await;
    let bad_id = "missing-word-id".to_string();

    // 加收藏：not found
    let resp = request(
        &app.app,
        Method::POST,
        &format!("/api/word-favorites/{bad_id}"),
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 加收藏：ok
    let resp = request(
        &app.app,
        Method::POST,
        &format!("/api/word-favorites/{word_id}"),
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["favorited"], true);

    // 列表
    let resp = request(
        &app.app,
        Method::GET,
        "/api/word-favorites",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let items = body["data"].as_array().expect("items array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["wordId"], word_id);

    // 状态查询
    let resp = request(
        &app.app,
        Method::GET,
        &format!("/api/word-favorites/status?wordIds={word_id},{bad_id}"),
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let arr = body["data"].as_array().expect("status array");
    assert_eq!(arr.len(), 2);

    // 状态超限
    let large = (0..1000)
        .map(|i| format!("id{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let resp = request(
        &app.app,
        Method::GET,
        &format!("/api/word-favorites/status?wordIds={large}"),
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "WORD_FAVORITES_TOO_MANY_IDS");

    // 删除
    let resp = request(
        &app.app,
        Method::DELETE,
        &format!("/api/word-favorites/{word_id}"),
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["deleted"], true);
}

#[tokio::test]
async fn it_word_favorites_unauthorized() {
    let app = spawn_test_server().await;
    let resp = request(&app.app, Method::GET, "/api/word-favorites", None, &[]).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ────────────────────── word_notes ──────────────────────

#[tokio::test]
async fn it_word_notes_full_flow() {
    let app = spawn_test_server().await;
    let user_token = login_and_get_token(&app.app).await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let word_id = create_word(&app.app, &admin_token, "noteword", "笔记词").await;

    // empty content -> 400
    let resp = request(
        &app.app,
        Method::POST,
        "/api/word-notes",
        Some(serde_json::json!({"wordId": word_id, "content": "  "})),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "WORD_NOTE_EMPTY_CONTENT");

    // too long -> 400
    let long_content = "x".repeat(5001);
    let resp = request(
        &app.app,
        Method::POST,
        "/api/word-notes",
        Some(serde_json::json!({"wordId": word_id, "content": long_content})),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "WORD_NOTE_TOO_LONG");

    // not found word -> 404
    let resp = request(
        &app.app,
        Method::POST,
        "/api/word-notes",
        Some(serde_json::json!({"wordId": "no-such-word", "content": "ok"})),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // happy: create
    let resp = request(
        &app.app,
        Method::POST,
        "/api/word-notes",
        Some(serde_json::json!({"wordId": word_id, "content": "first note"})),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CREATED);
    let note_id = body["data"]["id"].as_str().expect("note id").to_string();

    // list with filter
    let resp = request(
        &app.app,
        Method::GET,
        &format!("/api/word-notes?wordId={word_id}"),
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"].as_array().unwrap().len(), 1);

    // list without filter
    let resp = request(
        &app.app,
        Method::GET,
        "/api/word-notes",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"].as_array().unwrap().len(), 1);

    // update
    let resp = request(
        &app.app,
        Method::PUT,
        &format!("/api/word-notes/{note_id}"),
        Some(serde_json::json!({"content": "updated note"})),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["content"], "updated note");

    // update non-existent -> 404
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/word-notes/nonexistent",
        Some(serde_json::json!({"content": "x"})),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // delete non-existent -> 404
    let resp = request(
        &app.app,
        Method::DELETE,
        "/api/word-notes/nonexistent",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // delete actual
    let resp = request(
        &app.app,
        Method::DELETE,
        &format!("/api/word-notes/{note_id}"),
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["deleted"], true);
}

#[tokio::test]
async fn it_word_notes_unauthorized() {
    let app = spawn_test_server().await;
    let resp = request(&app.app, Method::GET, "/api/word-notes", None, &[]).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ────────────────────── word_states ──────────────────────

#[tokio::test]
async fn it_word_states_full_flow() {
    let app = spawn_test_server().await;
    let user_token = login_and_get_token(&app.app).await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let word_id = create_word(&app.app, &admin_token, "stateword", "状态词").await;

    // get_word_state: 不存在 -> 404
    let resp = request(
        &app.app,
        Method::GET,
        &format!("/api/word-states/{word_id}"),
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // mark_mastered: word 不存在 -> 404
    let resp = request(
        &app.app,
        Method::POST,
        "/api/word-states/missing/mark-mastered",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // mark_mastered: happy
    let resp = request(
        &app.app,
        Method::POST,
        &format!("/api/word-states/{word_id}/mark-mastered"),
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["masteryLevel"], 1.0);

    // get_word_state 现在 OK
    let resp = request(
        &app.app,
        Method::GET,
        &format!("/api/word-states/{word_id}"),
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);

    // reset_word: word 不存在 -> 404
    let resp = request(
        &app.app,
        Method::POST,
        "/api/word-states/missing/reset",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // reset_word: happy
    let resp = request(
        &app.app,
        Method::POST,
        &format!("/api/word-states/{word_id}/reset"),
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);

    // batch_query: 超限
    let large_ids: Vec<String> = (0..1000).map(|i| format!("id{i}")).collect();
    let resp = request(
        &app.app,
        Method::POST,
        "/api/word-states/batch",
        Some(serde_json::json!({"wordIds": large_ids})),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "BATCH_TOO_LARGE");

    // batch_query: ok
    let resp = request(
        &app.app,
        Method::POST,
        "/api/word-states/batch",
        Some(serde_json::json!({"wordIds": [word_id.clone()]})),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"].as_array().unwrap().len() <= 1);

    // due_list
    let resp = request(
        &app.app,
        Method::GET,
        "/api/word-states/due/list?limit=10",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);

    // stats_overview with various categories
    for cat in ["all", "learning", "review"] {
        let resp = request(
            &app.app,
            Method::GET,
            &format!("/api/word-states/stats/overview?category={cat}"),
            None,
            &[("authorization", auth_header(&user_token))],
        )
        .await;
        let (status, _, _) = response_json(resp).await;
        assert_eq!(status, StatusCode::OK, "category={cat}");
    }

    // invalid category
    let resp = request(
        &app.app,
        Method::GET,
        "/api/word-states/stats/overview?category=bogus",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_CATEGORY");

    // batch_update: 超限
    let bad_updates: Vec<serde_json::Value> = (0..1000)
        .map(|i| serde_json::json!({"wordId": format!("id{i}")}))
        .collect();
    let resp = request(
        &app.app,
        Method::POST,
        "/api/word-states/batch-update",
        Some(serde_json::json!({"updates": bad_updates})),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // batch_update: word 不存在 -> 400
    let resp = request(
        &app.app,
        Method::POST,
        "/api/word-states/batch-update",
        Some(serde_json::json!({"updates": [{"wordId": "missing-id"}]})),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "WORD_NOT_FOUND");

    // batch_update: ok with both state and mastery_level
    let resp = request(
        &app.app,
        Method::POST,
        "/api/word-states/batch-update",
        Some(serde_json::json!({
            "updates": [{
                "wordId": word_id,
                "state": "LEARNING",
                "masteryLevel": 0.5
            }]
        })),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["updated"], 1);
}

// ────────────────────── study_config ──────────────────────

#[tokio::test]
async fn it_study_config_full_flow() {
    let app = spawn_test_server().await;
    let user_token = login_and_get_token(&app.app).await;

    // get_config
    let resp = request(
        &app.app,
        Method::GET,
        "/api/study-config",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);

    // update_config with invalid wordbook id -> 400
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/study-config",
        Some(serde_json::json!({
            "selectedWordbookIds": ["no-such-wordbook"]
        })),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "WORDBOOK_NOT_FOUND");

    // update_config with daily/study_mode/mastery
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/study-config",
        Some(serde_json::json!({
            "dailyWordCount": 600,
            "studyMode": "intensive",
            "dailyMasteryTarget": 200
        })),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    // dailyWordCount 应被 clamp 到 200
    assert_eq!(body["data"]["dailyWordCount"], 200);
    // dailyMasteryTarget 应被 clamp 到 100
    assert_eq!(body["data"]["dailyMasteryTarget"], 100);
    assert_eq!(body["data"]["studyMode"], "intensive");

    // today-words (empty wordbook list -> fallback to global words)
    let resp = request(
        &app.app,
        Method::GET,
        "/api/study-config/today-words",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);

    // progress
    let resp = request(
        &app.app,
        Method::GET,
        "/api/study-config/progress",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
}

// ────────────────────── notifications ──────────────────────

#[tokio::test]
async fn it_notifications_basic_flow() {
    let app = spawn_test_server().await;
    let user_token = login_and_get_token(&app.app).await;

    // 空列表
    let resp = request(
        &app.app,
        Method::GET,
        "/api/notifications?limit=10",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"].as_array().unwrap().len(), 0);

    // unread count
    let resp = request(
        &app.app,
        Method::GET,
        "/api/notifications/unread-count",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["unreadCount"], 0);

    // mark non-existent read -> 404
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/notifications/missing/read",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // mark all read (空)
    let resp = request(
        &app.app,
        Method::POST,
        "/api/notifications/read-all",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["markedRead"], 0);

    // badges
    let resp = request(
        &app.app,
        Method::GET,
        "/api/notifications/badges",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    // 默认 3 个徽章
    assert_eq!(body["data"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn it_notifications_preferences_validation() {
    let app = spawn_test_server().await;
    let user_token = login_and_get_token(&app.app).await;

    // 默认 prefs
    let resp = request(
        &app.app,
        Method::GET,
        "/api/notifications/preferences",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);

    // invalid theme
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/notifications/preferences",
        Some(serde_json::json!({"theme": "neon"})),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_THEME");

    // invalid language
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/notifications/preferences",
        Some(serde_json::json!({"language": "xx"})),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_LANGUAGE");

    // valid all
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/notifications/preferences",
        Some(serde_json::json!({
            "theme": "dark",
            "language": "zh",
            "notificationEnabled": false,
            "soundEnabled": false
        })),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["theme"], "dark");
    assert_eq!(body["data"]["language"], "zh");

    // re-read uses persisted
    let resp = request(
        &app.app,
        Method::GET,
        "/api/notifications/preferences",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (_, _, body) = response_json(resp).await;
    assert_eq!(body["data"]["theme"], "dark");
    assert_eq!(body["data"]["notificationEnabled"], false);
}

// ────────────────────── wordbook_center (user) ──────────────────────

#[tokio::test]
async fn it_wordbook_center_user_unconfigured_returns_empty_or_error() {
    let app = spawn_test_server().await;
    let user_token = login_and_get_token(&app.app).await;

    // 默认 prefs 里没有 wordbook_center_url
    // user_browse -> []
    let resp = request(
        &app.app,
        Method::GET,
        "/api/wordbook-center/browse",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"].as_array().unwrap().is_empty());

    // user_updates -> []
    let resp = request(
        &app.app,
        Method::GET,
        "/api/wordbook-center/updates",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"].as_array().unwrap().is_empty());

    // user_preview -> 400 (未配置)
    let resp = request(
        &app.app,
        Method::GET,
        "/api/wordbook-center/browse/some-id",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "WB_CENTER_NOT_CONFIGURED");

    // user_import -> 400
    let resp = request(
        &app.app,
        Method::POST,
        "/api/wordbook-center/import/some-id",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "WB_CENTER_NOT_CONFIGURED");

    // user_sync -> 400
    let resp = request(
        &app.app,
        Method::POST,
        "/api/wordbook-center/updates/some-id/sync",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "WB_CENTER_NOT_CONFIGURED");
}

#[tokio::test]
async fn it_wordbook_center_user_settings_set_and_get_then_invalid_url() {
    let app = spawn_test_server().await;
    let user_token = login_and_get_token(&app.app).await;

    // 初始 settings：空
    let resp = request(
        &app.app,
        Method::GET,
        "/api/wordbook-center/settings",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["wordbookCenterUrl"].is_null());

    // 设置一个 public URL（不真发请求，仅写 prefs）
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/wordbook-center/settings",
        Some(serde_json::json!({
            "wordbookCenterUrl": "https://example.com/wb"
        })),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["wordbookCenterUrl"], "https://example.com/wb");

    // 清空（empty -> remove）
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/wordbook-center/settings",
        Some(serde_json::json!({"wordbookCenterUrl": ""})),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["wordbookCenterUrl"].is_null());

    // invalid URL -> 400
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/wordbook-center/settings",
        Some(serde_json::json!({
            "wordbookCenterUrl": "ftp://example.com/wb"
        })),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "IMPORT_INVALID_URL");

    // SSRF blocked URL
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/wordbook-center/settings",
        Some(serde_json::json!({
            "wordbookCenterUrl": "http://127.0.0.1/wb"
        })),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "IMPORT_BLOCKED_URL");
}

#[tokio::test]
async fn it_wordbook_center_user_import_history_pagination() {
    let app = spawn_test_server().await;
    let user_token = login_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        Method::GET,
        "/api/wordbook-center/import-history?page=1&perPage=10",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["data"].as_array().unwrap().is_empty());
    assert_eq!(body["data"]["total"], 0);
}

#[tokio::test]
async fn it_wordbook_center_user_import_url_validation_errors() {
    let app = spawn_test_server().await;
    let user_token = login_and_get_token(&app.app).await;

    // SSRF blocked URL
    let resp = request(
        &app.app,
        Method::POST,
        "/api/wordbook-center/import-url",
        Some(serde_json::json!({"url": "http://localhost:8080/wb.json"})),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "IMPORT_BLOCKED_URL");

    // Invalid scheme
    let resp = request(
        &app.app,
        Method::POST,
        "/api/wordbook-center/import-url",
        Some(serde_json::json!({"url": "ftp://example.com/wb.json"})),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "IMPORT_INVALID_URL");
}

// ────────────────────── wordbook_center (admin) ──────────────────────

#[tokio::test]
async fn it_wordbook_center_admin_url_blocked_returns_blocked() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    // 将 wordbook_center_url 设置为本地 IP（会被 SSRF 拒绝）
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/admin/settings",
        Some(serde_json::json!({"wordbookCenterUrl": "http://127.0.0.1/wb"})),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);

    // admin_browse: fetch_remote_json 拒绝 -> 400
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/wordbook-center/browse",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "IMPORT_BLOCKED_URL");

    // admin_preview
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/wordbook-center/browse/some-id",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // admin_import
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/wordbook-center/import/some-id",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // admin_updates: 没 import 记录 -> 直接返回空（提前 short-circuit）
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/wordbook-center/updates",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"].as_array().unwrap().is_empty());

    // admin_sync: 没记录 -> 404
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/wordbook-center/updates/some-id/sync",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn it_wordbook_center_admin_unauthorized() {
    let app = spawn_test_server().await;
    for path in [
        "/api/admin/wordbook-center/browse",
        "/api/admin/wordbook-center/updates",
    ] {
        let resp = request(&app.app, Method::GET, path, None, &[]).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "path {path}");
    }
}
