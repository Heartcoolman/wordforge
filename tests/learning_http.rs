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

#[tokio::test]
async fn get_learning_session_validates_ownership_and_existence() {
    let app = spawn_test_server().await;
    let token_a = login_and_get_token(&app.app).await;
    let token_b = login_and_get_token(&app.app).await;

    // user_a 创建 session
    let create = request(
        &app.app,
        Method::POST,
        "/api/learning/session",
        Some(serde_json::json!({})),
        &[("authorization", auth_header(&token_a))],
    )
    .await;
    let (create_status, _, create_body) = response_json(create).await;
    assert_eq!(create_status, StatusCode::OK);
    let session_id = create_body["data"]["sessionId"]
        .as_str()
        .expect("sessionId")
        .to_string();

    // user_a GET /sessions/:id → 200 + status=active
    let get_own = request(
        &app.app,
        Method::GET,
        &format!("/api/learning/sessions/{session_id}"),
        None,
        &[("authorization", auth_header(&token_a))],
    )
    .await;
    let (own_status, _, own_body) = response_json(get_own).await;
    assert_eq!(own_status, StatusCode::OK);
    assert_eq!(own_body["data"]["status"].as_str(), Some("active"));
    assert_eq!(own_body["data"]["id"].as_str(), Some(session_id.as_str()));

    // 不存在的 id → 404 + LEARNING_SESSION_NOT_FOUND
    let missing = request(
        &app.app,
        Method::GET,
        "/api/learning/sessions/00000000-0000-0000-0000-000000000000",
        None,
        &[("authorization", auth_header(&token_a))],
    )
    .await;
    let (missing_status, _, missing_body) = response_json(missing).await;
    assert_eq!(missing_status, StatusCode::NOT_FOUND);
    assert_eq!(missing_body["code"].as_str(), Some("LEARNING_SESSION_NOT_FOUND"));

    // user_b 查 user_a 的 session → 同样 404（避免信息泄漏）
    let cross = request(
        &app.app,
        Method::GET,
        &format!("/api/learning/sessions/{session_id}"),
        None,
        &[("authorization", auth_header(&token_b))],
    )
    .await;
    let (cross_status, _, cross_body) = response_json(cross).await;
    assert_eq!(cross_status, StatusCode::NOT_FOUND);
    assert_eq!(cross_body["code"].as_str(), Some("LEARNING_SESSION_NOT_FOUND"));
}
