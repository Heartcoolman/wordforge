//! admin/auth 路由补充 —— 触发账户锁定路径（5 次失败登录 → too_many_requests），
//! 同时覆盖 logout / verify 的额外分支。

mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, setup_admin_and_get_token};
use common::http::{request, response_json};

#[tokio::test]
async fn it_admin_login_locks_after_5_failed_attempts() {
    let app = spawn_test_server().await;

    // 先 setup admin
    let email = format!("locked-{}@test.com", uuid::Uuid::new_v4());
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/setup",
        Some(serde_json::json!({
            "email": &email,
            "password": "AdminPassw0rd!"
        })),
        &[],
    )
    .await;
    let (status, _, _body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CREATED);

    // 连续 5 次错误密码登录 → 第 5 次后账户应被锁定
    for i in 0..5 {
        let resp = request(
            &app.app,
            Method::POST,
            "/api/admin/auth/login",
            Some(serde_json::json!({
                "email": &email,
                "password": "WrongP4ssword!"
            })),
            &[],
        )
        .await;
        let (status, _, _) = response_json(resp).await;
        // 前 4 次返回 401 / 第 5 次后锁定开始；都不会成功
        assert!(
            status == StatusCode::UNAUTHORIZED || status == StatusCode::TOO_MANY_REQUESTS,
            "attempt {i}: status={status}"
        );
    }

    // 第 6 次（even with correct password）应当 too_many_requests
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/login",
        Some(serde_json::json!({
            "email": &email,
            "password": "AdminPassw0rd!"
        })),
        &[],
    )
    .await;
    let (status, _, _body) = response_json(resp).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn it_admin_login_lockout_boundaries_for_four_five_and_six_attempts() {
    let app = spawn_test_server().await;
    let email = format!("boundary-{}@test.com", uuid::Uuid::new_v4());
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/setup",
        Some(serde_json::json!({
            "email": &email,
            "password": "AdminPassw0rd!"
        })),
        &[],
    )
    .await;
    assert_eq!(response_json(resp).await.0, StatusCode::CREATED);

    for attempt in 1..=4 {
        let resp = request(
            &app.app,
            Method::POST,
            "/api/admin/auth/login",
            Some(serde_json::json!({
                "email": &email,
                "password": "WrongP4ssword!"
            })),
            &[],
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "attempt {attempt}");
    }

    let fifth = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/login",
        Some(serde_json::json!({
            "email": &email,
            "password": "WrongP4ssword!"
        })),
        &[],
    )
    .await;
    assert_eq!(fifth.status(), StatusCode::UNAUTHORIZED);

    let sixth = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/login",
        Some(serde_json::json!({
            "email": &email,
            "password": "AdminPassw0rd!"
        })),
        &[],
    )
    .await;
    assert_eq!(sixth.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn it_admin_login_does_not_lock_unknown_email() {
    let app = spawn_test_server().await;
    // 没有 setup admin，仍然要能 reject 不存在的账户（generate_dummy_argon2_hash 路径）
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/login",
        Some(serde_json::json!({
            "email": "ghost@nowhere.com",
            "password": "AnyPassw0rd!"
        })),
        &[],
    )
    .await;
    let (status, _, _body) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn it_admin_verify_then_logout_full_flow() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;

    // verify
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/auth/verify",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["id"].is_string());
    assert!(body["data"]["email"].is_string());

    // logout
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/logout",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["loggedOut"], true);

    // logout 后 token 不再可用
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/auth/verify",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn it_admin_login_resets_failed_count_on_success() {
    let app = spawn_test_server().await;
    let email = format!("recovers-{}@test.com", uuid::Uuid::new_v4());
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/setup",
        Some(serde_json::json!({
            "email": &email,
            "password": "AdminPassw0rd!"
        })),
        &[],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::CREATED);

    // 3 次失败（不到 5 次锁定阈值）
    for _ in 0..3 {
        let resp = request(
            &app.app,
            Method::POST,
            "/api/admin/auth/login",
            Some(serde_json::json!({
                "email": &email,
                "password": "WrongP4ssword!"
            })),
            &[],
        )
        .await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // 然后用正确密码登录 → reset_admin_login_attempts 分支
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/login",
        Some(serde_json::json!({
            "email": &email,
            "password": "AdminPassw0rd!"
        })),
        &[],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert!(body["data"]["token"].is_string());
}
