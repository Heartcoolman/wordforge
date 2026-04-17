mod common;

use axum::http::{Method, StatusCode};
use axum::Router;

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token, setup_admin_and_get_token};
use common::http::{request, response_json};

async fn create_word(app: &Router, token: &str, text: &str, meaning: &str) -> String {
    let response = request(
        app,
        Method::POST,
        "/api/words",
        Some(serde_json::json!({
            "text": text,
            "meaning": meaning,
            "difficulty": 0.4,
        })),
        &[("authorization", auth_header(token))],
    )
    .await;
    let (status, _, body) = response_json(response).await;
    assert_eq!(status, StatusCode::CREATED);
    body["data"]["id"]
        .as_str()
        .expect("created word id")
        .to_string()
}

#[tokio::test]
async fn learning_routes_fill_batch_size_when_inventory_is_sufficient() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    for idx in 0..15 {
        let _ = create_word(
            &app.app,
            &admin_token,
            &format!("learning-word-{idx}"),
            &format!("学习词-{idx}"),
        )
        .await;
    }

    let study_words = request(
        &app.app,
        Method::GET,
        "/api/learning/study-words",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (study_status, _, study_body) = response_json(study_words).await;
    assert_eq!(study_status, StatusCode::OK);
    let study_words_len = study_body["data"]["words"].as_array().unwrap().len();
    let study_batch_size = study_body["data"]["strategy"]["batchSize"]
        .as_u64()
        .unwrap() as usize;
    assert_eq!(study_words_len, study_batch_size);

    let next_words = request(
        &app.app,
        Method::POST,
        "/api/learning/next-words",
        Some(serde_json::json!({
            "excludeWordIds": [],
            "masteredWordIds": [],
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (next_status, _, next_body) = response_json(next_words).await;
    assert_eq!(next_status, StatusCode::OK);
    let next_words_len = next_body["data"]["words"].as_array().unwrap().len();
    let next_batch_size = next_body["data"]["batchSize"].as_u64().unwrap() as usize;
    assert_eq!(next_words_len, next_batch_size);
}
