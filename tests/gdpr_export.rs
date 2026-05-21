//! GDPR Article 20 数据导出端点集成测试 (M1-G1)。
//!
//! 验收场景：
//! 1. 创建用户 → 注入学习/收藏/备注数据 → 导出 → 校验 JSON Lines 完整
//! 2. DELETE /api/users/me → 重新注册同邮箱 → 导出为空（无旧数据）
//! 3. 24h 冷却：同一用户第二次导出返回 429 + Retry-After header
//! 4. 未认证请求返回 401

mod common;

use axum::body::to_bytes;
use axum::http::{Method, StatusCode};
use chrono::Utc;
use rusqlite::params;
use serde_json::Value;

use common::app::spawn_test_server;
use common::auth::auth_header;
use common::http::{request, response_json};

/// 注册并返回 (access_token, user_id)。
async fn register_user(
    app: &axum::Router,
    email: &str,
    username: &str,
) -> (String, String) {
    let resp = request(
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
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CREATED, "register failed: {body}");
    let token = body["data"]["accessToken"]
        .as_str()
        .unwrap()
        .to_string();
    let user_id = body["data"]["user"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    (token, user_id)
}

/// 解析 NDJSON 响应体为各行 Value，按 table 字段索引。
async fn parse_ndjson(resp: axum::response::Response) -> Vec<Value> {
    let bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read body");
    let text = String::from_utf8(bytes.to_vec()).expect("utf8");
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<Value>(l).expect("parse json line"))
        .collect()
}

#[tokio::test]
async fn it_gdpr_export_returns_ndjson_with_all_data() {
    let app = spawn_test_server().await;
    let email = format!("gdpr-{}@test.com", uuid::Uuid::new_v4());
    let username = format!("gdpr-{}", uuid::Uuid::new_v4().simple());

    let (token, user_id) = register_user(&app.app, &email, &username).await;

    // 注入测试数据（绕过 HTTP，直接写库）
    let now = Utc::now().to_rfc3339();
    let store = app.state.store();
    let conn = store.connection().unwrap();

    conn.execute(
        "INSERT INTO word_favorites (user_id, word_id, created_at) VALUES (?1, 'w1', ?2)",
        params![&user_id, now],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO word_notes (user_id, id, word_id, content, created_at, updated_at)
         VALUES (?1, 'n1', 'w1', 'my note', ?2, ?2)",
        params![&user_id, now],
    )
    .unwrap();
    drop(conn);

    // 导出
    let resp = request(
        &app.app,
        Method::GET,
        "/api/users/me/export",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;

    assert_eq!(resp.status(), StatusCode::OK);
    // Content-Type 应为 ndjson
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("ndjson") || ct.contains("application/x"), "unexpected content-type: {ct}");

    let lines = parse_ndjson(resp).await;
    assert!(!lines.is_empty(), "export should not be empty");

    // 找到 profile 块，验证 email 正确
    let profile_line = lines.iter().find(|l| l["table"] == "profile");
    assert!(profile_line.is_some(), "missing profile block");
    assert_eq!(profile_line.unwrap()["data"]["email"], email);

    // 找到 favorites 块，验证 word_id
    let fav_line = lines.iter().find(|l| l["table"] == "favorites");
    assert!(fav_line.is_some(), "missing favorites block");
    let fav_data = &fav_line.unwrap()["data"];
    assert!(
        fav_data.as_array().map(|a| a.iter().any(|e| e["wordId"] == "w1")).unwrap_or(false),
        "favorites should contain w1, got: {fav_data}"
    );

    // 找到 notes 块，验证内容
    let notes_line = lines.iter().find(|l| l["table"] == "notes");
    assert!(notes_line.is_some(), "missing notes block");
    let notes_data = &notes_line.unwrap()["data"];
    assert!(
        notes_data
            .as_array()
            .map(|a| a.iter().any(|e| e["content"] == "my note"))
            .unwrap_or(false),
        "notes should contain 'my note'"
    );
}

#[tokio::test]
async fn it_gdpr_export_requires_auth() {
    let app = spawn_test_server().await;
    let resp = request(&app.app, Method::GET, "/api/users/me/export", None, &[]).await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn it_gdpr_export_rate_limited_on_second_call() {
    let app = spawn_test_server().await;
    let email = format!("gdpr-rl-{}@test.com", uuid::Uuid::new_v4());
    let username = format!("gdpr-rl-{}", uuid::Uuid::new_v4().simple());
    let (token, _) = register_user(&app.app, &email, &username).await;

    // 第一次：成功
    let resp1 = request(
        &app.app,
        Method::GET,
        "/api/users/me/export",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    assert_eq!(resp1.status(), StatusCode::OK, "first export should succeed");

    // 第二次：429
    let resp2 = request(
        &app.app,
        Method::GET,
        "/api/users/me/export",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let status2 = resp2.status();
    let retry_after = resp2
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.parse::<i64>().ok())
        .flatten();
    assert_eq!(status2, StatusCode::TOO_MANY_REQUESTS, "second export should be rate limited");
    assert!(
        retry_after.map(|s| s > 0).unwrap_or(false),
        "retry-after header should be a positive integer"
    );

    let (_, _, body2) = response_json(resp2).await;
    assert_eq!(body2["code"], "GDPR_EXPORT_RATE_LIMITED");
}

#[tokio::test]
async fn it_gdpr_export_empty_after_delete_and_reregister() {
    let app = spawn_test_server().await;
    let email = format!("gdpr-del-{}@test.com", uuid::Uuid::new_v4());
    let username1 = format!("gdpr-del1-{}", uuid::Uuid::new_v4().simple());
    let username2 = format!("gdpr-del2-{}", uuid::Uuid::new_v4().simple());

    let (token1, user_id1) = register_user(&app.app, &email, &username1).await;

    // 注入收藏数据
    let now = Utc::now().to_rfc3339();
    let conn = app.state.store().connection().unwrap();
    conn.execute(
        "INSERT INTO word_favorites (user_id, word_id, created_at) VALUES (?1, 'w_old', ?2)",
        params![&user_id1, now],
    )
    .unwrap();
    drop(conn);

    // 删除账号
    let del_resp = request(
        &app.app,
        Method::DELETE,
        "/api/users/me",
        None,
        &[("authorization", auth_header(&token1))],
    )
    .await;
    let (del_status, _, _) = response_json(del_resp).await;
    assert_eq!(del_status, StatusCode::OK);

    // 同邮箱重新注册
    let (token2, _) = register_user(&app.app, &email, &username2).await;

    // 新账号导出：favorites 应为空
    let resp = request(
        &app.app,
        Method::GET,
        "/api/users/me/export",
        None,
        &[("authorization", auth_header(&token2))],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let lines = parse_ndjson(resp).await;

    // favorites 块不存在，或 data 数组为空
    let fav_data = lines
        .iter()
        .find(|l| l["table"] == "favorites")
        .and_then(|l| l["data"].as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    assert_eq!(fav_data, 0, "new user should have no favorites from deleted account");
}
