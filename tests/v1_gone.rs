/// M0-C5：验证 /api/v1/* 全部端点返回 410 Gone + JSON 错误体。
mod common;

use axum::http::{Method, StatusCode};
use axum::body::to_bytes;

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token};
use common::http::request;

/// 断言给定路径返回 410，且响应体含 "GONE" error 字段。
async fn assert_gone(app: &axum::Router, token: &str, method: Method, path: &str) {
    let response = request(
        app,
        method,
        path,
        None,
        &[("authorization", auth_header(token))],
    )
    .await;

    assert_eq!(
        response.status(),
        StatusCode::GONE,
        "{path} 应返回 410 Gone，实际：{}",
        response.status()
    );

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).expect("410 响应体应为合法 JSON");
    assert_eq!(
        json["error"].as_str(),
        Some("GONE"),
        "{path} 响应体 error 字段应为 \"GONE\""
    );
    assert!(
        json["sunset"].as_str().is_some(),
        "{path} 响应体应含 sunset 字段"
    );
    assert!(
        json["migrationDoc"].as_str().is_some(),
        "{path} 响应体应含 migrationDoc 字段"
    );
}

#[tokio::test]
async fn v1_words_returns_410() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    assert_gone(&app.app, &token, Method::GET, "/api/v1/words").await;
}

#[tokio::test]
async fn v1_word_by_id_returns_410() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    assert_gone(&app.app, &token, Method::GET, "/api/v1/words/some-id").await;
}

#[tokio::test]
async fn v1_records_get_returns_410() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    assert_gone(&app.app, &token, Method::GET, "/api/v1/records").await;
}

#[tokio::test]
async fn v1_records_post_returns_410() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    assert_gone(&app.app, &token, Method::POST, "/api/v1/records").await;
}

#[tokio::test]
async fn v1_study_config_returns_410() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    assert_gone(&app.app, &token, Method::GET, "/api/v1/study-config").await;
}

#[tokio::test]
async fn v1_learning_session_returns_410() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    assert_gone(&app.app, &token, Method::POST, "/api/v1/learning/session").await;
}
