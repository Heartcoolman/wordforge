mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token};
use common::http::{request, response_json};

#[tokio::test]
async fn at_full_flow_smoke() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    let create_word = request(
        &app.app,
        Method::POST,
        "/api/words",
        Some(serde_json::json!({
            "text": "flow-word",
            "meaning": "流程词",
            "difficulty": 0.3
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (word_status, _, word_body) = response_json(create_word).await;
    assert_eq!(word_status, StatusCode::CREATED);
    let word_id = word_body["data"]["id"].as_str().unwrap().to_string();

    let create_record = request(
        &app.app,
        Method::POST,
        "/api/records",
        Some(serde_json::json!({
            "wordId": word_id,
            "isCorrect": true,
            "responseTimeMs": 800,
            "sessionId": "flow-session",
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (record_status, _, record_body) = response_json(create_record).await;
    assert_eq!(record_status, StatusCode::CREATED);
    assert!(record_body["data"]["amasResult"]["strategy"].is_object());

    let health = request(&app.app, Method::GET, "/health/live", None, &[]).await;
    let (health_status, _, _) = response_json(health).await;
    assert_eq!(health_status, StatusCode::OK);
}
