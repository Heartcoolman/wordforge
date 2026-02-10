mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token};
use common::http::{request, response_json};

#[tokio::test]
async fn it_amas_process_event_cold_start() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    let response = request(
        &app.app,
        Method::POST,
        "/api/amas/process-event",
        Some(serde_json::json!({
            "wordId": "word-1",
            "isCorrect": true,
            "responseTime": 1000,
            "sessionId": "session-1"
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;

    let (status, _, body) = response_json(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["coldStartPhase"], "Classify");
}

#[tokio::test]
async fn it_amas_high_fatigue_applies_constraints() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    let mut last_body = serde_json::json!({});

    for idx in 0..6 {
        let response = request(
            &app.app,
            Method::POST,
            "/api/amas/process-event",
            Some(serde_json::json!({
                "wordId": format!("word-{idx}"),
                "isCorrect": false,
                "responseTime": 2000,
                "sessionId": "fatigue-session",
                "isQuit": true,
                "hintUsed": true
            })),
            &[("authorization", auth_header(&token))],
        )
        .await;

        let (status, _, body) = response_json(response).await;
        assert_eq!(status, StatusCode::OK);
        last_body = body;
    }

    let difficulty = last_body["data"]["strategy"]["difficulty"]
        .as_f64()
        .unwrap();
    assert!(difficulty <= 0.55);
}
