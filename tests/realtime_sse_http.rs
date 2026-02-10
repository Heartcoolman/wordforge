mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token};
use common::http::request;

#[tokio::test]
async fn it_sse_endpoint_is_reachable() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    let response = request(
        &app.app,
        Method::GET,
        "/api/realtime/events",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(content_type.contains("text/event-stream"));
}
