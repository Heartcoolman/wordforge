mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token};
use common::http::{request, response_json};

#[tokio::test]
async fn it_record_create_and_query() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    let create = request(
        &app.app,
        Method::POST,
        "/api/records",
        Some(serde_json::json!({
            "wordId": "w-test",
            "isCorrect": true,
            "responseTimeMs": 1200,
            "sessionId": "s-1"
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;

    let (status, _, body) = response_json(create).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(body["data"]["record"]["id"].is_string());
    assert!(body["data"]["amasResult"]["strategy"].is_object());

    let list = request(
        &app.app,
        Method::GET,
        "/api/records?limit=50",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;

    let (list_status, _, list_body) = response_json(list).await;
    assert_eq!(list_status, StatusCode::OK);
    assert!(list_body["data"].is_array());
    assert!(list_body["data"].as_array().unwrap().len() >= 1);
}
