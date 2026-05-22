mod common;

use axum::http::{Method, StatusCode};
use futures::future::join_all;

use common::app::{spawn_test_server_with_dual_limits, spawn_test_server_with_limits};
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

/// v1.1-P2.3：双轨限流——匿名打满后 429，但同 IP 上的已登录用户仍可继续。
#[tokio::test]
async fn it_dual_track_anonymous_429_but_authenticated_still_allowed() {
    // api_limit fallback=1000（保险）、anon=3、authed=20
    let app = spawn_test_server_with_dual_limits(1000, 3, 20).await;
    let token = login_and_get_token(&app.app).await;

    // 匿名请求：第 4 次必拒（默认 3 次后 429）。
    let mut anon_last = StatusCode::OK;
    for _ in 0..4 {
        let response = request(&app.app, Method::GET, "/api/users/me", None, &[]).await;
        anon_last = response.status();
    }
    assert_eq!(
        anon_last,
        StatusCode::TOO_MANY_REQUESTS,
        "匿名 4 次后应当 429（anon_max=3）"
    );

    // 已登录请求：在匿名已 429 之后，同 IP 仍可继续 ≥10 次成功（authed_max=20）。
    let auth = auth_header(&token);
    let mut success_count = 0;
    for _ in 0..10 {
        let response = request(
            &app.app,
            Method::GET,
            "/api/users/me",
            None,
            &[("authorization", auth.clone())],
        )
        .await;
        if response.status() == StatusCode::OK {
            success_count += 1;
        }
    }
    assert_eq!(
        success_count, 10,
        "已登录 user_id 限流独立于匿名 IP 限流；10 次应全部 OK，实际 {success_count}"
    );
}

/// 已登录用户打满 authenticated_max 后仍会被 429（限流确实生效，不是无界）。
#[tokio::test]
async fn it_dual_track_authenticated_429_after_exceeding_authed_max() {
    let app = spawn_test_server_with_dual_limits(1000, 100, 5).await;
    let token = login_and_get_token(&app.app).await;
    let auth = auth_header(&token);

    let mut last_status = StatusCode::OK;
    for _ in 0..6 {
        let response = request(
            &app.app,
            Method::GET,
            "/api/users/me",
            None,
            &[("authorization", auth.clone())],
        )
        .await;
        last_status = response.status();
    }
    assert_eq!(
        last_status,
        StatusCode::TOO_MANY_REQUESTS,
        "已登录 6 次后应 429（authed_max=5）"
    );
}

#[tokio::test]
async fn it_rate_limit_rejects_parallel_burst_above_limit() {
    let app = spawn_test_server_with_limits(1, 10).await;
    let token = login_and_get_token(&app.app).await;

    let auth = auth_header(&token);
    let requests = (0..8).map(|_| async {
        let auth = auth.clone();
        request(
            &app.app,
            Method::GET,
            "/api/users/me",
            None,
            &[("authorization", auth)],
        )
        .await
    });
    let responses = join_all(requests).await;
    let rejected = responses
        .iter()
        .filter(|response| response.status() == StatusCode::TOO_MANY_REQUESTS)
        .count();

    assert!(
        rejected >= 7,
        "with limit=1, at least 7 of 8 concurrent requests should be rejected; got {rejected}"
    );
}
