mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token, login_and_get_tokens};
use common::http::{assert_json_error, request, response_json};

#[tokio::test]
async fn it_auth_register_success() {
    let app = spawn_test_server().await;

    let response = request(
        &app.app,
        Method::POST,
        "/api/auth/register",
        Some(serde_json::json!({
            "email": "auth-register@test.com",
            "username": "auth_register",
            "password": "Passw0rd!"
        })),
        &[],
    )
    .await;

    let (status, _, body) = response_json(response).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["success"], true);
    assert!(body["data"]["token"].is_string());
}

#[tokio::test]
async fn it_auth_duplicate_email_conflict() {
    let app = spawn_test_server().await;

    for _ in 0..2 {
        let _ = request(
            &app.app,
            Method::POST,
            "/api/auth/register",
            Some(serde_json::json!({
                "email": "dup@test.com",
                "username": "dup",
                "password": "Passw0rd!"
            })),
            &[],
        )
        .await;
    }

    let response = request(
        &app.app,
        Method::POST,
        "/api/auth/register",
        Some(serde_json::json!({
            "email": "dup@test.com",
            "username": "dup2",
            "password": "Passw0rd!"
        })),
        &[],
    )
    .await;

    let (status, _, body) = response_json(response).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_json_error(&body, "AUTH_EMAIL_EXISTS");
}

#[tokio::test]
async fn it_auth_login_success() {
    let app = spawn_test_server().await;

    let _ = request(
        &app.app,
        Method::POST,
        "/api/auth/register",
        Some(serde_json::json!({
            "email": "login@test.com",
            "username": "login",
            "password": "Passw0rd!"
        })),
        &[],
    )
    .await;

    let response = request(
        &app.app,
        Method::POST,
        "/api/auth/login",
        Some(serde_json::json!({
            "email": "login@test.com",
            "password": "Passw0rd!"
        })),
        &[],
    )
    .await;

    let (status, _, body) = response_json(response).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["accessToken"].is_string());
}

#[tokio::test]
async fn it_auth_refresh_rotates_token() {
    let app = spawn_test_server().await;
    let (access_token, refresh_token) = login_and_get_tokens(&app.app).await;

    // Use the refresh token (not the access token) to call /api/auth/refresh
    let refresh = request(
        &app.app,
        Method::POST,
        "/api/auth/refresh",
        None,
        &[("authorization", auth_header(&refresh_token))],
    )
    .await;

    let (status, _, body) = response_json(refresh).await;
    assert_eq!(status, StatusCode::OK);
    let new_access = body["data"]["accessToken"].as_str().unwrap().to_string();
    assert_ne!(new_access, access_token);

    // The new access token should work
    let new_me = request(
        &app.app,
        Method::GET,
        "/api/users/me",
        None,
        &[("authorization", auth_header(&new_access))],
    )
    .await;
    let (new_status, _, _) = response_json(new_me).await;
    assert_eq!(new_status, StatusCode::OK);

    // The old refresh token should be revoked (one-time use)
    let refresh_again = request(
        &app.app,
        Method::POST,
        "/api/auth/refresh",
        None,
        &[("authorization", auth_header(&refresh_token))],
    )
    .await;
    let (old_refresh_status, _, _) = response_json(refresh_again).await;
    assert_eq!(old_refresh_status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn it_auth_logout_revokes_session() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    let logout = request(
        &app.app,
        Method::POST,
        "/api/auth/logout",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;

    let (status, _, _) = response_json(logout).await;
    assert_eq!(status, StatusCode::OK);

    let me = request(
        &app.app,
        Method::GET,
        "/api/users/me",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (me_status, _, _) = response_json(me).await;
    assert_eq!(me_status, StatusCode::UNAUTHORIZED);
}
