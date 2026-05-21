/// M0-C4：验证 /api/v1/* 端点响应包含正确的 Deprecation / Sunset header（RFC 8594）。
mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token};
use common::http::request;
use learning_backend::middleware::deprecation::{
    V1_DEPRECATION_DATE, V1_LINK_URL, V1_SUNSET_DATE,
};

#[tokio::test]
async fn v1_words_response_has_deprecation_and_sunset_headers() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    let response = request(
        &app.app,
        Method::GET,
        "/api/v1/words",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);

    let deprecation = response
        .headers()
        .get("Deprecation")
        .and_then(|v| v.to_str().ok())
        .expect("Deprecation header 应存在");
    assert_eq!(deprecation, V1_DEPRECATION_DATE, "Deprecation 日期不符");

    let sunset = response
        .headers()
        .get("Sunset")
        .and_then(|v| v.to_str().ok())
        .expect("Sunset header 应存在");
    assert_eq!(sunset, V1_SUNSET_DATE, "Sunset 日期不符");

    let link = response
        .headers()
        .get("Link")
        .and_then(|v| v.to_str().ok())
        .expect("Link header 应存在");
    assert!(
        link.contains(V1_LINK_URL),
        "Link header 应包含迁移文档 URL，实际：{link}"
    );
    assert!(link.contains("rel=\"sunset\""), "Link header 应含 rel=sunset");
}

#[tokio::test]
async fn v1_study_config_response_has_deprecation_headers() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    let response = request(
        &app.app,
        Method::GET,
        "/api/v1/study-config",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response.headers().contains_key("Deprecation"),
        "Deprecation header 应存在"
    );
    assert!(
        response.headers().contains_key("Sunset"),
        "Sunset header 应存在"
    );
}

#[tokio::test]
async fn non_v1_endpoint_has_no_deprecation_headers() {
    // /api/words（非 v1）不应有弃用 header
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    let response = request(
        &app.app,
        Method::GET,
        "/api/words",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        !response.headers().contains_key("Deprecation"),
        "/api/words 不应有 Deprecation header"
    );
    assert!(
        !response.headers().contains_key("Sunset"),
        "/api/words 不应有 Sunset header"
    );
}
