mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server_with_limits;
use common::auth::{auth_header, login_and_get_token};
use common::http::{request, response_json};

#[tokio::test]
async fn it_rate_limit_triggers_429_with_headers() {
    let app = spawn_test_server_with_limits(3, 10).await;
    let token = login_and_get_token(&app.app).await;

    let mut final_status = StatusCode::OK;
    let mut final_headers = axum::http::HeaderMap::new();

    for _ in 0..4 {
        let response = request(
            &app.app,
            Method::GET,
            "/api/users/me",
            None,
            &[("authorization", auth_header(&token))],
        )
        .await;

        let (status, headers, _) = response_json(response).await;
        final_status = status;
        final_headers = headers;
    }

    assert_eq!(final_status, StatusCode::TOO_MANY_REQUESTS);
    assert!(final_headers.get("retry-after").is_some());
    assert!(final_headers.get("ratelimit-limit").is_some());
    assert!(final_headers.get("ratelimit-remaining").is_some());
    assert!(final_headers.get("ratelimit-reset").is_some());
}
