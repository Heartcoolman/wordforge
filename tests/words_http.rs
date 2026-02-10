mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token};
use common::http::{assert_json_error, request, response_json};

#[tokio::test]
async fn it_word_create_and_list() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    let create = request(
        &app.app,
        Method::POST,
        "/api/words",
        Some(serde_json::json!({
            "text": "apple",
            "meaning": "苹果",
            "difficulty": 0.4
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;

    let (create_status, _, _) = response_json(create).await;
    assert_eq!(create_status, StatusCode::CREATED);

    let list = request(
        &app.app,
        Method::GET,
        "/api/words?limit=20&offset=0",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;

    let (list_status, _, body) = response_json(list).await;
    assert_eq!(list_status, StatusCode::OK);
    assert!(body["data"]["items"].is_array());
    assert!(body["data"]["items"].as_array().unwrap().len() >= 1);
}

#[tokio::test]
async fn it_word_list_invalid_limit() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    let list = request(
        &app.app,
        Method::GET,
        "/api/words?limit=101&offset=0",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;

    let (status, _, body) = response_json(list).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_json_error(&body, "WORDS_INVALID_LIMIT");
}
