use axum::http::{HeaderMap, Method};
use axum::Router;

use super::http::{request, response_json};

pub async fn login_and_get_token(app: &Router) -> String {
    let (access, _refresh) = login_and_get_tokens(app).await;
    access
}

/// 从 Set-Cookie header 中提取指定 cookie 的值
fn extract_cookie_value(headers: &HeaderMap, cookie_name: &str) -> Option<String> {
    for value in headers.get_all("set-cookie") {
        if let Ok(s) = value.to_str() {
            // cookie 格式: "name=value; Path=/; ..."
            if let Some(rest) = s.strip_prefix(&format!("{cookie_name}=")) {
                let val = rest.split(';').next().unwrap_or("");
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

/// Returns (access_token, refresh_token).
/// refresh_token 从响应的 Set-Cookie header 中提取。
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

    let (status, headers, body) = response_json(response).await;
    assert!(status.is_success(), "register failed: {body}");

    let access = body["data"]["accessToken"]
        .as_str()
        .expect("access token in register response")
        .to_string();

    let refresh = extract_cookie_value(&headers, "refresh_token")
        .expect("refresh_token cookie in register response");

    (access, refresh)
}

pub fn auth_header(token: &str) -> String {
    format!("Bearer {token}")
}

pub async fn setup_admin_and_get_token(app: &Router) -> String {
    let email = format!("admin-{}@test.com", uuid::Uuid::new_v4());
    let password = "AdminPassw0rd!";

    let response = request(
        app,
        Method::POST,
        "/api/admin/auth/setup",
        Some(serde_json::json!({
            "email": email,
            "password": password,
        })),
        &[],
    )
    .await;

    let (status, _headers, body) = response_json(response).await;
    assert!(status.is_success(), "admin setup failed: {body}");

    body["data"]["token"]
        .as_str()
        .expect("admin token in setup response")
        .to_string()
}
