mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token, setup_admin_and_get_token};
use common::http::{assert_status_ok_json, request, response_json};

#[tokio::test]
async fn it_feedback_requires_auth() {
    let app = spawn_test_server().await;

    let response = request(
        &app.app,
        Method::POST,
        "/api/feedback",
        Some(serde_json::json!({
            "category": "profile",
            "body": "hello",
            "route": "profile"
        })),
        &[],
    )
    .await;
    let (status, _, _) = response_json(response).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn it_feedback_is_persisted_and_visible_to_admin() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let response = request(
        &app.app,
        Method::POST,
        "/api/feedback",
        Some(serde_json::json!({
            "category": "profile",
            "body": "Please improve review stats.",
            "route": "profile"
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(response).await;
    assert_status_ok_json(status, &body);
    assert!(body["data"]["id"].is_string());
    assert_eq!(body["data"]["category"], "profile");
    assert_eq!(body["data"]["route"], "profile");

    let response = request(
        &app.app,
        Method::GET,
        "/api/admin/feedback",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(response).await;
    assert_status_ok_json(status, &body);
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(
        body["data"]["data"][0]["body"],
        "Please improve review stats."
    );
}

#[tokio::test]
async fn it_admin_feedback_requires_admin_auth() {
    let app = spawn_test_server().await;

    let response = request(&app.app, Method::GET, "/api/admin/feedback", None, &[]).await;
    let (status, _, _) = response_json(response).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
