mod common;

use axum::http::{Method, StatusCode};

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
