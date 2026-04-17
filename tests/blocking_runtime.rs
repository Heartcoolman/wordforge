mod common;

use std::time::Duration;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::http::{request, response_json};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn it_keeps_liveness_responsive_while_store_requests_wait_for_connections() {
    let app = spawn_test_server().await;

    let register = request(
        &app.app,
        Method::POST,
        "/api/auth/register",
        Some(serde_json::json!({
            "email": "blocking-login@test.com",
            "username": "blocking_login",
            "password": "Passw0rd!"
        })),
        &[],
    )
    .await;
    let (register_status, _, _) = response_json(register).await;
    assert_eq!(register_status, StatusCode::CREATED);

    let held_conn_a = app
        .state
        .store()
        .connection()
        .expect("hold first sqlite connection");
    let held_conn_b = app
        .state
        .store()
        .connection()
        .expect("hold second sqlite connection");

    let ready_app = app.app.clone();
    let ready_task =
        tokio::spawn(
            async move { request(&ready_app, Method::GET, "/health/ready", None, &[]).await },
        );

    let login_app = app.app.clone();
    let login_task = tokio::spawn(async move {
        request(
            &login_app,
            Method::POST,
            "/api/auth/login",
            Some(serde_json::json!({
                "email": "blocking-login@test.com",
                "password": "Passw0rd!"
            })),
            &[("x-device-id", "blocking-device".to_string())],
        )
        .await
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let live_response = tokio::time::timeout(
        Duration::from_millis(250),
        request(&app.app, Method::GET, "/health/live", None, &[]),
    )
    .await
    .expect("liveness should stay responsive while store tasks wait");
    let (live_status, _, _) = response_json(live_response).await;
    assert_eq!(live_status, StatusCode::OK);

    drop(held_conn_a);
    drop(held_conn_b);

    let ready_response = tokio::time::timeout(Duration::from_secs(2), ready_task)
        .await
        .expect("readiness task should finish after releasing sqlite connections")
        .expect("readiness join");
    let (ready_status, _, _) = response_json(ready_response).await;
    assert_eq!(ready_status, StatusCode::OK);

    let login_response = tokio::time::timeout(Duration::from_secs(2), login_task)
        .await
        .expect("login task should finish after releasing sqlite connections")
        .expect("login join");
    let (login_status, _, login_body) = response_json(login_response).await;
    assert_eq!(login_status, StatusCode::OK);
    assert!(login_body["data"]["accessToken"].is_string());
}
