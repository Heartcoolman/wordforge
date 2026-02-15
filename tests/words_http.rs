mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token, setup_admin_and_get_token};
use common::http::{request, response_json};

#[tokio::test]
async fn it_word_create_and_list() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let create = request(
        &app.app,
        Method::POST,
        "/api/words",
        Some(serde_json::json!({
            "text": "apple",
            "meaning": "苹果",
            "difficulty": 0.4
        })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;

    let (create_status, _, _) = response_json(create).await;
    assert_eq!(create_status, StatusCode::CREATED);

    let list = request(
        &app.app,
        Method::GET,
        "/api/words?page=1&perPage=20",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;

    let (list_status, _, body) = response_json(list).await;
    assert_eq!(list_status, StatusCode::OK);
    assert!(body["data"]["data"].is_array());
    assert!(!body["data"]["data"].as_array().unwrap().is_empty());
    assert!(body["data"]["page"].as_u64().unwrap() == 1);
    assert!(body["data"]["perPage"].as_u64().unwrap() == 20);
}

#[tokio::test]
async fn it_word_list_large_per_page_is_clamped() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    // perPage=200 should be clamped to 100, not error
    let list = request(
        &app.app,
        Method::GET,
        "/api/words?page=1&perPage=200",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;

    let (status, _, body) = response_json(list).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["perPage"].as_u64().unwrap(), 100);
}
