mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, setup_admin_and_get_token};
use common::http::{request, response_json};

const ADMINS: &str = "/api/admin/settings/admins";
const KEYS: &str = "/api/admin/settings/api-keys";

fn h(token: &str) -> [(&'static str, String); 1] {
    [("authorization", auth_header(token))]
}

/// 管理员登录拿 token（用于"非 super-admin"分支）。
async fn admin_login(app: &axum::Router, email: &str, password: &str) -> String {
    let resp = request(
        app,
        Method::POST,
        "/api/admin/auth/login",
        Some(serde_json::json!({ "email": email, "password": password })),
        &[],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "admin login failed: {body}");
    body["data"]["token"].as_str().expect("login token").to_string()
}

/// super-admin 邀请一个普通 admin，返回 (id, token)。
async fn invite_regular(app: &axum::Router, super_token: &str) -> (String, String) {
    let email = format!("ra-{}@test.com", uuid::Uuid::new_v4());
    let pw = "Regular123!";
    let resp = request(
        app,
        Method::POST,
        ADMINS,
        Some(serde_json::json!({ "email": email, "password": pw, "role": "admin" })),
        &h(super_token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CREATED, "invite failed: {body}");
    let id = body["data"]["id"].as_str().expect("invited id").to_string();
    let token = admin_login(app, &email, pw).await;
    (id, token)
}

async fn setup_admin_id(app: &axum::Router, super_token: &str) -> String {
    let resp = request(app, Method::GET, ADMINS, None, &h(super_token)).await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "list admins: {body}");
    // 唯一的 super_admin 即 setup 创建的首个 admin
    let admins = body["data"]["admins"].as_array().expect("admins array");
    admins
        .iter()
        .find(|a| a["role"] == "super_admin")
        .and_then(|a| a["id"].as_str())
        .expect("super admin id")
        .to_string()
}

// ─────────────────────────── admins ───────────────────────────

#[tokio::test]
async fn list_admins_returns_setup_super_admin() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(&app.app, Method::GET, ADMINS, None, &h(&token)).await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let admins = body["data"]["admins"].as_array().unwrap();
    assert!(!admins.is_empty());
    assert!(admins.iter().any(|a| a["role"] == "super_admin"));
}

#[tokio::test]
async fn invite_admin_invalid_email_rejected() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        ADMINS,
        Some(serde_json::json!({ "email": "not-an-email", "password": "Valid123!" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "AUTH_INVALID_EMAIL");
}

#[tokio::test]
async fn invite_admin_weak_password_rejected() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        ADMINS,
        Some(serde_json::json!({ "email": "ok@test.com", "password": "weak" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "AUTH_WEAK_PASSWORD");
}

#[tokio::test]
async fn invite_admin_invalid_role_rejected() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        ADMINS,
        Some(serde_json::json!({ "email": "ok2@test.com", "password": "Valid123!", "role": "king" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_ADMIN_ROLE");
}

#[tokio::test]
async fn invite_admin_by_non_super_forbidden() {
    let app = spawn_test_server().await;
    let super_token = setup_admin_and_get_token(&app.app).await;
    let (_id, regular_token) = invite_regular(&app.app, &super_token).await;
    // 普通 admin 邀请他人 → 403
    let resp = request(
        &app.app,
        Method::POST,
        ADMINS,
        Some(serde_json::json!({ "email": "x@test.com", "password": "Valid123!" })),
        &h(&regular_token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn patch_role_not_found() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::PATCH,
        &format!("{ADMINS}/no-such-admin/role"),
        Some(serde_json::json!({ "role": "admin" })),
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn patch_role_invalid_value() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let (id, _) = invite_regular(&app.app, &token).await;
    let resp = request(
        &app.app,
        Method::PATCH,
        &format!("{ADMINS}/{id}/role"),
        Some(serde_json::json!({ "role": "bogus" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_ADMIN_ROLE");
}

#[tokio::test]
async fn patch_role_promote_then_demote_last_super_guard() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let super_id = setup_admin_id(&app.app, &token).await;

    // 降级唯一 super → 409 LAST_SUPER_ADMIN
    let resp = request(
        &app.app,
        Method::PATCH,
        &format!("{ADMINS}/{super_id}/role"),
        Some(serde_json::json!({ "role": "admin" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CONFLICT, "body={body}");
    assert_eq!(body["code"], "LAST_SUPER_ADMIN");

    // 把一个普通 admin 提升为 super → 200
    let (id, _) = invite_regular(&app.app, &token).await;
    let resp = request(
        &app.app,
        Method::PATCH,
        &format!("{ADMINS}/{id}/role"),
        Some(serde_json::json!({ "role": "super_admin" })),
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn patch_role_by_non_super_forbidden() {
    let app = spawn_test_server().await;
    let super_token = setup_admin_and_get_token(&app.app).await;
    let (id, regular_token) = invite_regular(&app.app, &super_token).await;
    let resp = request(
        &app.app,
        Method::PATCH,
        &format!("{ADMINS}/{id}/role"),
        Some(serde_json::json!({ "role": "super_admin" })),
        &h(&regular_token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn delete_admin_not_found() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::DELETE,
        &format!("{ADMINS}/no-such-admin"),
        None,
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_admin_success_and_non_super_forbidden() {
    let app = spawn_test_server().await;
    let super_token = setup_admin_and_get_token(&app.app).await;
    let (id, regular_token) = invite_regular(&app.app, &super_token).await;

    // 普通 admin 删除他人 → 403
    let resp = request(
        &app.app,
        Method::DELETE,
        &format!("{ADMINS}/{id}"),
        None,
        &h(&regular_token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // super 删除该普通 admin → 成功
    let resp = request(
        &app.app,
        Method::DELETE,
        &format!("{ADMINS}/{id}"),
        None,
        &h(&super_token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert!(status.is_success(), "delete should succeed, got {status}");
}

// ─────────────────────────── api-keys ───────────────────────────

#[tokio::test]
async fn list_api_keys_ok() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(&app.app, Method::GET, KEYS, None, &h(&token)).await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn generate_api_key_success() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        KEYS,
        Some(serde_json::json!({ "name": "ci-key", "scope": "read" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CREATED, "body={body}");
}

#[tokio::test]
async fn generate_api_key_invalid_scope_rejected() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        KEYS,
        Some(serde_json::json!({ "name": "k", "scope": "god" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_SCOPE");
}

#[tokio::test]
async fn generate_api_key_blank_name_rejected() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        KEYS,
        Some(serde_json::json!({ "name": "   " })),
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rotate_and_revoke_api_key() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // 先建一个
    let resp = request(
        &app.app,
        Method::POST,
        KEYS,
        Some(serde_json::json!({ "name": "rotateme", "scope": "write" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CREATED, "body={body}");
    let id = body["data"]["key"]["id"].as_str().expect("api key id").to_string();

    // rotate
    let resp = request(
        &app.app,
        Method::POST,
        &format!("{KEYS}/{id}/rotate"),
        None,
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert!(status.is_success(), "rotate should succeed, got {status}");

    // revoke
    let resp = request(
        &app.app,
        Method::DELETE,
        &format!("{KEYS}/{id}"),
        None,
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert!(status.is_success(), "revoke should succeed, got {status}");
}

#[tokio::test]
async fn rotate_nonexistent_api_key_not_found() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        &format!("{KEYS}/no-such-key/rotate"),
        None,
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rbac_endpoints_require_auth() {
    let app = spawn_test_server().await;
    let resp = request(&app.app, Method::GET, ADMINS, None, &[]).await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
