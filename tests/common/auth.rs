use axum::http::Method;
use axum::Router;

use super::http::{request, response_json};

pub async fn login_and_get_token(app: &Router) -> String {
    let (access, _refresh) = login_and_get_tokens(app).await;
    access
}

/// Returns (access_token, refresh_token).
pub async fn login_and_get_tokens(app: &Router) -> (String, String) {
    let email = format!("user-{}@test.com", uuid::Uuid::new_v4());
    let username = format!("user-{}", uuid::Uuid::new_v4().simple());
    let password = "Passw0rd!";

    let response = request(
        app,
        Method::POST,
        "/api/auth/register",
        Some(serde_json::json!({
            "email": email,
            "username": username,
            "password": password,
        })),
        &[],
    )
    .await;

    let (status, _headers, body) = response_json(response).await;
    assert!(status.is_success(), "register failed: {body}");

    let access = body["data"]["accessToken"]
        .as_str()
        .or_else(|| body["data"]["token"].as_str())
        .expect("access token in register response")
        .to_string();

    let refresh = body["data"]["refreshToken"]
        .as_str()
        .expect("refresh token in register response")
        .to_string();

    (access, refresh)
}

pub fn auth_header(token: &str) -> String {
    format!("Bearer {token}")
}
