mod common;

use axum::http::{Method, StatusCode};

use common::app::{spawn_test_server, TestApp};
use common::auth::{auth_header, login_and_get_token};
use common::fixtures::seed_words;
use common::http::{request, response_json};

// 直接通过 store 取一个登录用户的 user_id（从 /api/users/me）。
async fn me_id(app: &axum::Router, token: &str) -> String {
    let resp = request(
        app,
        Method::GET,
        "/api/users/me",
        None,
        &[("authorization", auth_header(token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "me failed: {body}");
    body["data"]["id"].as_str().expect("user id").to_string()
}

// 创建/恢复会话，返回 sessionId。
async fn create_session(app: &axum::Router, token: &str) -> String {
    let resp = request(
        app,
        Method::POST,
        "/api/learning/session",
        Some(serde_json::json!({})),
        &[("authorization", auth_header(token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "create session failed: {body}");
    body["data"]["sessionId"]
        .as_str()
        .expect("session id")
        .to_string()
}

// ─────────────────────────── pick-next-word ───────────────────────────

#[tokio::test]
async fn pick_next_word_requires_auth() {
    let TestApp { app, .. } = spawn_test_server().await;
    let resp = request(
        &app,
        Method::POST,
        "/api/learning/pick-next-word",
        Some(serde_json::json!({ "activeWordIds": ["x"], "errorWordIds": [] })),
        &[],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "AUTH_UNAUTHORIZED");
}

#[tokio::test]
async fn pick_next_word_empty_active_ids() {
    let TestApp { app, .. } = spawn_test_server().await;
    let token = login_and_get_token(&app).await;
    let resp = request(
        &app,
        Method::POST,
        "/api/learning/pick-next-word",
        Some(serde_json::json!({ "activeWordIds": [], "errorWordIds": [] })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "LEARNING_NO_ACTIVE_WORDS");
}

#[tokio::test]
async fn pick_next_word_normal_priority() {
    let TestApp { app, state, .. } = spawn_test_server().await;
    let token = login_and_get_token(&app).await;
    let words = seed_words(state.store(), 3);
    let ids: Vec<String> = words.iter().map(|w| w.id.clone()).collect();

    let mut priority_map = serde_json::Map::new();
    priority_map.insert(ids[2].clone(), serde_json::json!(9));
    priority_map.insert(ids[0].clone(), serde_json::json!(1));

    let resp = request(
        &app,
        Method::POST,
        "/api/learning/pick-next-word",
        Some(serde_json::json!({
            "activeWordIds": ids,
            "errorWordIds": [],
            "priorityMap": priority_map,
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["priority"], "normal");
    // priorityMap 最高的 ids[2] 应被选中
    assert_eq!(body["data"]["word"]["id"], ids[2]);
}

#[tokio::test]
async fn pick_next_word_error_review_priority() {
    let TestApp { app, state, .. } = spawn_test_server().await;
    let token = login_and_get_token(&app).await;
    let words = seed_words(state.store(), 3);
    let ids: Vec<String> = words.iter().map(|w| w.id.clone()).collect();

    let resp = request(
        &app,
        Method::POST,
        "/api/learning/pick-next-word",
        Some(serde_json::json!({
            "activeWordIds": ids,
            "errorWordIds": [ids[1].clone()],
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["priority"], "error_review");
    assert_eq!(body["data"]["word"]["id"], ids[1]);
}

#[tokio::test]
async fn pick_next_word_word_not_found() {
    let TestApp { app, .. } = spawn_test_server().await;
    let token = login_and_get_token(&app).await;
    let resp = request(
        &app,
        Method::POST,
        "/api/learning/pick-next-word",
        Some(serde_json::json!({
            "activeWordIds": ["nonexistent-word-id"],
            "errorWordIds": [],
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "NOT_FOUND");
}

// ─────────────────────────── generate-options ───────────────────────────

#[tokio::test]
async fn generate_options_invalid_mode() {
    let TestApp { app, state, .. } = spawn_test_server().await;
    let token = login_and_get_token(&app).await;
    let words = seed_words(state.store(), 1);
    let resp = request(
        &app,
        Method::POST,
        "/api/learning/generate-options",
        Some(serde_json::json!({
            "wordId": words[0].id,
            "mode": "totally-bogus",
            "poolWordIds": [],
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "LEARNING_INVALID_MODE");
}

#[tokio::test]
async fn generate_options_word_not_found() {
    let TestApp { app, .. } = spawn_test_server().await;
    let token = login_and_get_token(&app).await;
    let resp = request(
        &app,
        Method::POST,
        "/api/learning/generate-options",
        Some(serde_json::json!({
            "wordId": "no-such-word",
            "mode": "word-to-meaning",
            "poolWordIds": [],
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "NOT_FOUND");
}

#[tokio::test]
async fn generate_options_word_to_meaning_with_pool() {
    let TestApp { app, state, .. } = spawn_test_server().await;
    let token = login_and_get_token(&app).await;
    let words = seed_words(state.store(), 6);
    let target = words[0].id.clone();
    let pool: Vec<String> = words.iter().map(|w| w.id.clone()).collect();

    let resp = request(
        &app,
        Method::POST,
        "/api/learning/generate-options",
        Some(serde_json::json!({
            "wordId": target,
            "mode": "word-to-meaning",
            "poolWordIds": pool,
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let options = body["data"]["options"].as_array().expect("options array");
    assert_eq!(options.len(), 4, "应生成 4 个选项");
    let idx = body["data"]["correctIndex"].as_u64().expect("correctIndex") as usize;
    // 正确选项应为 target word 的 meaning
    assert_eq!(options[idx], "meaning-0");
}

#[tokio::test]
async fn generate_options_meaning_to_word_fallback() {
    // poolWordIds 为空 → 走 list_words fallback 补足干扰项
    let TestApp { app, state, .. } = spawn_test_server().await;
    let token = login_and_get_token(&app).await;
    let words = seed_words(state.store(), 5);
    let target = words[0].id.clone();

    let resp = request(
        &app,
        Method::POST,
        "/api/learning/generate-options",
        Some(serde_json::json!({
            "wordId": target,
            "mode": "meaning-to-word",
            "poolWordIds": [],
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let options = body["data"]["options"].as_array().expect("options array");
    assert_eq!(options.len(), 4);
    let idx = body["data"]["correctIndex"].as_u64().expect("correctIndex") as usize;
    // meaning-to-word：正确选项应为 target 的 text
    assert_eq!(options[idx], "word-0");
}

// ─────────────────────────── session create/resume ───────────────────────────

#[tokio::test]
async fn create_session_requires_auth() {
    let TestApp { app, .. } = spawn_test_server().await;
    let resp = request(
        &app,
        Method::POST,
        "/api/learning/session",
        Some(serde_json::json!({})),
        &[],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_session_fresh_then_resume() {
    let TestApp { app, .. } = spawn_test_server().await;
    let token = login_and_get_token(&app).await;

    // 首次创建：resumed=false
    let resp = request(
        &app,
        Method::POST,
        "/api/learning/session",
        Some(serde_json::json!({ "targetMasteryCount": 7 })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["resumed"], false);
    assert_eq!(body["data"]["status"], "active");
    assert_eq!(body["data"]["targetMasteryCount"], 7);
    let first_id = body["data"]["sessionId"].as_str().unwrap().to_string();

    // 再次创建：应恢复同一 active 会话，resumed=true
    let resp = request(
        &app,
        Method::POST,
        "/api/learning/session",
        Some(serde_json::json!({})),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["resumed"], true);
    assert_eq!(body["data"]["sessionId"], first_id);
}

#[tokio::test]
async fn create_session_no_body() {
    // body 可选：完全不带 body 也应成功（走 unwrap_or_default）
    let TestApp { app, .. } = spawn_test_server().await;
    let token = login_and_get_token(&app).await;
    let resp = request(
        &app,
        Method::POST,
        "/api/learning/session",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["resumed"], false);
}

// ─────────────────────────── sessions list ───────────────────────────

#[tokio::test]
async fn list_sessions_requires_auth() {
    let TestApp { app, .. } = spawn_test_server().await;
    let resp = request(&app, Method::GET, "/api/learning/sessions", None, &[]).await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_sessions_empty() {
    let TestApp { app, .. } = spawn_test_server().await;
    let token = login_and_get_token(&app).await;
    let resp = request(
        &app,
        Method::GET,
        "/api/learning/sessions",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["data"].as_array().unwrap().len(), 0);
    assert_eq!(body["data"]["total"], 0);
}

#[tokio::test]
async fn list_sessions_with_data_and_pagination() {
    let TestApp { app, .. } = spawn_test_server().await;
    let token = login_and_get_token(&app).await;
    create_session(&app, &token).await;

    let resp = request(
        &app,
        Method::GET,
        "/api/learning/sessions?page=1&perPage=10",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["total"], 1);
    let item = &body["data"]["data"][0];
    assert_eq!(item["status"], "active");
    assert!(item["sessionId"].is_string());
}

#[tokio::test]
async fn list_sessions_invalid_date() {
    let TestApp { app, .. } = spawn_test_server().await;
    let token = login_and_get_token(&app).await;
    let resp = request(
        &app,
        Method::GET,
        "/api/learning/sessions?startDate=not-a-date",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "LEARNING_INVALID_DATE");
}

#[tokio::test]
async fn list_sessions_invalid_date_range() {
    let TestApp { app, .. } = spawn_test_server().await;
    let token = login_and_get_token(&app).await;
    // startDate 晚于 endDate → 区间非法
    let resp = request(
        &app,
        Method::GET,
        "/api/learning/sessions?startDate=2025-12-31&endDate=2025-01-01",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "LEARNING_INVALID_DATE_RANGE");
}

#[tokio::test]
async fn list_sessions_valid_date_range() {
    // 合法日期区间正常返回（覆盖 shanghai_midnight + 区间过滤路径）
    let TestApp { app, .. } = spawn_test_server().await;
    let token = login_and_get_token(&app).await;
    let resp = request(
        &app,
        Method::GET,
        "/api/learning/sessions?startDate=2025-01-01&endDate=2025-12-31",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(body["data"]["data"].is_array());
}

// ─────────────────────────── get session by id ───────────────────────────

#[tokio::test]
async fn get_session_not_found() {
    let TestApp { app, .. } = spawn_test_server().await;
    let token = login_and_get_token(&app).await;
    let resp = request(
        &app,
        Method::GET,
        "/api/learning/sessions/does-not-exist",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "LEARNING_SESSION_NOT_FOUND");
}

#[tokio::test]
async fn get_session_owned() {
    let TestApp { app, .. } = spawn_test_server().await;
    let token = login_and_get_token(&app).await;
    let sid = create_session(&app, &token).await;
    let resp = request(
        &app,
        Method::GET,
        &format!("/api/learning/sessions/{sid}"),
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["id"], sid);
    assert_eq!(body["data"]["status"], "active");
}

#[tokio::test]
async fn get_session_other_user_404() {
    // 他人会话也返回 404 LEARNING_SESSION_NOT_FOUND（防信息泄漏）
    let TestApp { app, .. } = spawn_test_server().await;
    let token_a = login_and_get_token(&app).await;
    let token_b = login_and_get_token(&app).await;
    let sid_a = create_session(&app, &token_a).await;

    let resp = request(
        &app,
        Method::GET,
        &format!("/api/learning/sessions/{sid_a}"),
        None,
        &[("authorization", auth_header(&token_b))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "LEARNING_SESSION_NOT_FOUND");
}

// ─────────────────────────── sync-progress ───────────────────────────

#[tokio::test]
async fn sync_progress_requires_auth() {
    let TestApp { app, .. } = spawn_test_server().await;
    let resp = request(
        &app,
        Method::POST,
        "/api/learning/sync-progress",
        Some(serde_json::json!({ "sessionId": "x" })),
        &[],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn sync_progress_session_not_found() {
    let TestApp { app, .. } = spawn_test_server().await;
    let token = login_and_get_token(&app).await;
    let resp = request(
        &app,
        Method::POST,
        "/api/learning/sync-progress",
        Some(serde_json::json!({ "sessionId": "ghost", "totalQuestions": 5 })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "NOT_FOUND");
}

#[tokio::test]
async fn sync_progress_updates_counts() {
    let TestApp { app, .. } = spawn_test_server().await;
    let token = login_and_get_token(&app).await;
    let sid = create_session(&app, &token).await;
    let resp = request(
        &app,
        Method::POST,
        "/api/learning/sync-progress",
        Some(serde_json::json!({
            "sessionId": sid,
            "totalQuestions": 12,
            "contextShifts": 3,
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["totalQuestions"], 12);
    assert_eq!(body["data"]["contextShifts"], 3);
    assert_eq!(body["data"]["id"], sid);
}

#[tokio::test]
async fn sync_progress_other_user_forbidden() {
    let TestApp { app, .. } = spawn_test_server().await;
    let token_a = login_and_get_token(&app).await;
    let token_b = login_and_get_token(&app).await;
    let sid_a = create_session(&app, &token_a).await;

    let resp = request(
        &app,
        Method::POST,
        "/api/learning/sync-progress",
        Some(serde_json::json!({ "sessionId": sid_a, "totalQuestions": 1 })),
        &[("authorization", auth_header(&token_b))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "FORBIDDEN");
}

#[tokio::test]
async fn sync_progress_with_events_ingest() {
    // events 路径：先校验归属（owner），再进入 ingest。用真实 seed word 摄取 1 条记录。
    let TestApp { app, state, .. } = spawn_test_server().await;
    let token = login_and_get_token(&app).await;
    let _uid = me_id(&app, &token).await;
    let words = seed_words(state.store(), 1);
    let sid = create_session(&app, &token).await;

    let resp = request(
        &app,
        Method::POST,
        "/api/learning/sync-progress",
        Some(serde_json::json!({
            "sessionId": sid,
            "totalQuestions": 1,
            "events": [{
                "clientEventId": "evt-sync-1",
                "wordId": words[0].id,
                "isCorrect": true,
                "responseTimeMs": 1500,
                "clientTsMs": 1_700_000_000_000_i64,
            }],
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    // 成功摄取 → eventIngest 存在且无失败
    assert_eq!(body["data"]["eventIngest"]["failed"], 0, "body: {body}");
    assert_eq!(body["data"]["eventIngest"]["count"], 1, "body: {body}");
}

#[tokio::test]
async fn sync_progress_events_invalid_event_id() {
    // events 内含非法 clientEventId（空串）→ 该事件计入 failed，但请求本身 200
    let TestApp { app, state, .. } = spawn_test_server().await;
    let token = login_and_get_token(&app).await;
    let words = seed_words(state.store(), 1);
    let sid = create_session(&app, &token).await;

    let resp = request(
        &app,
        Method::POST,
        "/api/learning/sync-progress",
        Some(serde_json::json!({
            "sessionId": sid,
            "events": [{
                "clientEventId": "   ",
                "wordId": words[0].id,
                "isCorrect": true,
                "responseTimeMs": 1000,
                "clientTsMs": 1_700_000_000_000_i64,
            }],
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["eventIngest"]["failed"], 1, "body: {body}");
    assert_eq!(
        body["data"]["eventIngest"]["errors"][0]["code"],
        "LEARNING_INVALID_EVENT_ID"
    );
}

#[tokio::test]
async fn sync_progress_events_other_user_forbidden() {
    // events 路径下他人会话应返回 403（在 load_for_events 阶段拦截）
    let TestApp { app, state, .. } = spawn_test_server().await;
    let token_a = login_and_get_token(&app).await;
    let token_b = login_and_get_token(&app).await;
    let words = seed_words(state.store(), 1);
    let sid_a = create_session(&app, &token_a).await;

    let resp = request(
        &app,
        Method::POST,
        "/api/learning/sync-progress",
        Some(serde_json::json!({
            "sessionId": sid_a,
            "events": [{
                "clientEventId": "evt-x",
                "wordId": words[0].id,
                "isCorrect": true,
                "responseTimeMs": 1000,
                "clientTsMs": 1_700_000_000_000_i64,
            }],
        })),
        &[("authorization", auth_header(&token_b))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "FORBIDDEN");
}

// ─────────────────────────── complete-session ───────────────────────────

#[tokio::test]
async fn complete_session_requires_auth() {
    let TestApp { app, .. } = spawn_test_server().await;
    let resp = request(
        &app,
        Method::POST,
        "/api/learning/complete-session",
        Some(serde_json::json!({
            "sessionId": "x",
            "masteredWordIds": [],
            "errorProneWordIds": [],
            "avgResponseTimeMs": 100,
        })),
        &[],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn complete_session_not_found() {
    let TestApp { app, .. } = spawn_test_server().await;
    let token = login_and_get_token(&app).await;
    let resp = request(
        &app,
        Method::POST,
        "/api/learning/complete-session",
        Some(serde_json::json!({
            "sessionId": "ghost",
            "masteredWordIds": [],
            "errorProneWordIds": [],
            "avgResponseTimeMs": 100,
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "NOT_FOUND");
}

#[tokio::test]
async fn complete_session_other_user_forbidden() {
    let TestApp { app, .. } = spawn_test_server().await;
    let token_a = login_and_get_token(&app).await;
    let token_b = login_and_get_token(&app).await;
    let sid_a = create_session(&app, &token_a).await;

    let resp = request(
        &app,
        Method::POST,
        "/api/learning/complete-session",
        Some(serde_json::json!({
            "sessionId": sid_a,
            "masteredWordIds": [],
            "errorProneWordIds": [],
            "avgResponseTimeMs": 100,
        })),
        &[("authorization", auth_header(&token_b))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "FORBIDDEN");
}

#[tokio::test]
async fn complete_session_success_no_records() {
    // 无 records、无 events → accuracy=0、status 转 completed
    let TestApp { app, .. } = spawn_test_server().await;
    let token = login_and_get_token(&app).await;
    let sid = create_session(&app, &token).await;

    let resp = request(
        &app,
        Method::POST,
        "/api/learning/complete-session",
        Some(serde_json::json!({
            "sessionId": sid,
            "masteredWordIds": [],
            "errorProneWordIds": [],
            "avgResponseTimeMs": 1234,
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["status"], "completed");
    assert_eq!(body["data"]["id"], sid);
    assert!(body["data"]["summary"].is_object());
}
