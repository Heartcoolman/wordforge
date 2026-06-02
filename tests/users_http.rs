mod common;

use axum::http::{Method, StatusCode};
use chrono::Utc;
use rusqlite::params;

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token};
use common::http::{assert_status_ok_json, request, response_json};

#[tokio::test]
async fn it_user_get_me_success() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    let response = request(
        &app.app,
        Method::GET,
        "/api/users/me",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;

    let (status, _, body) = response_json(response).await;
    assert_status_ok_json(status, &body);
    assert!(body["data"]["id"].is_string());
}

#[tokio::test]
async fn it_user_me_requires_auth() {
    let app = spawn_test_server().await;

    let response = request(&app.app, Method::GET, "/api/users/me", None, &[]).await;
    let (status, _, _) = response_json(response).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn it_user_delete_me_removes_account_and_session() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    let response = request(
        &app.app,
        Method::DELETE,
        "/api/users/me",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(response).await;
    assert_status_ok_json(status, &body);
    assert_eq!(body["data"]["deleted"], true);

    let response = request(
        &app.app,
        Method::GET,
        "/api/users/me",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, _) = response_json(response).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn it_user_delete_me_cascades_favorites_notes_history_and_user_wordbooks() {
    let app = spawn_test_server().await;

    // 注册以拿到 user_id + token
    let email = format!("cascade-{}@test.com", uuid::Uuid::new_v4());
    let username = format!("cascade-{}", uuid::Uuid::new_v4().simple());
    let response = request(
        &app.app,
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

    // 通过 store 注入用户作用域子数据（绕过 HTTP，避免引入无关端点依赖）
    let now = Utc::now().to_rfc3339();
    let store = app.state.store();
    let conn = store.connection().unwrap();
    conn.execute(
        "INSERT INTO word_favorites (user_id, word_id, created_at) VALUES (?1,'w_fav',?2)",
        params![&user_id, now],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO word_notes (user_id, id, word_id, content, created_at, updated_at)
         VALUES (?1,'note_e2e','w_fav','memo',?2,?2)",
        params![&user_id, now],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO wordbook_import_history (id, user_id, source_type, status, created_at)
         VALUES ('hist_e2e',?1,'json','success',?2)",
        params![&user_id, now],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO wordbooks (id, name, description, book_type, user_id, word_count, created_at)
         VALUES ('wb_e2e','my','d','user',?1,1,?2)",
        params![&user_id, now],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO wordbook_words (wordbook_id, word_id, added_at) VALUES ('wb_e2e','w_fav',?1)",
        params![now],
    )
    .unwrap();
    drop(conn);

    // DELETE /api/users/me
    let response = request(
        &app.app,
        Method::DELETE,
        "/api/users/me",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(response).await;
    assert_status_ok_json(status, &body);
    assert_eq!(body["data"]["deleted"], true);

    // 直接查 store 验证四类数据全部清空
    let conn = store.connection().unwrap();
    let cnt =
        |sql: &str, uid: &str| -> i64 { conn.query_row(sql, params![uid], |r| r.get(0)).unwrap() };
    assert_eq!(
        cnt(
            "SELECT COUNT(*) FROM word_favorites WHERE user_id=?1",
            &user_id
        ),
        0
    );
    assert_eq!(
        cnt("SELECT COUNT(*) FROM word_notes WHERE user_id=?1", &user_id),
        0
    );
    assert_eq!(
        cnt(
            "SELECT COUNT(*) FROM wordbook_import_history WHERE user_id=?1",
            &user_id,
        ),
        0
    );
    assert_eq!(
        cnt(
            "SELECT COUNT(*) FROM wordbooks WHERE book_type='user' AND user_id=?1",
            &user_id,
        ),
        0
    );
    let wb_words: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM wordbook_words WHERE wordbook_id='wb_e2e'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(wb_words, 0);
}
