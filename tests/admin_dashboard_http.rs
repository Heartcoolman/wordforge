mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, setup_admin_and_get_token};
use common::http::{request, response_json};

const OVERVIEW: &str = "/api/admin/dashboard/overview";

fn h(token: &str) -> [(&'static str, String); 1] {
    [("authorization", auth_header(token))]
}

#[tokio::test]
async fn overview_returns_aggregated_blocks() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;

    let resp = request(&app.app, Method::GET, OVERVIEW, None, &h(&token)).await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["success"], true);

    let d = &body["data"];
    // 首屏各分块齐备
    for key in [
        "generatedAt",
        "totals",
        "trend",
        "online",
        "platforms",
        "health",
        "series",
    ] {
        assert!(!d[key].is_null(), "缺少分块 {key}: {body}");
    }
    assert_eq!(d["days"], 7, "默认窗口 7 天");
    assert!(d["totals"]["users"].is_number());
    assert!(d["online"]["sseConnections"].is_number());
    assert!(d["health"]["storeProbeOk"].as_bool().unwrap_or(false));
    assert!(d["series"]["dailyActiveUsers"].is_array());
}

#[tokio::test]
async fn overview_respects_days_param() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        Method::GET,
        &format!("{OVERVIEW}?days=14"),
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["days"], 14);
}

#[tokio::test]
async fn overview_requires_admin_token() {
    let app = spawn_test_server().await;
    let resp = request(&app.app, Method::GET, OVERVIEW, None, &[]).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn overview_health_has_rate_limit_hits() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(&app.app, Method::GET, OVERVIEW, None, &h(&token)).await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let rl = &body["data"]["health"]["rateLimitHits"];
    for k in ["user", "anon", "admin", "authBruteforce"] {
        assert!(rl[k].is_number(), "rateLimitHits 缺 {k}: {body}");
    }
}

#[tokio::test]
async fn learning_endpoint_returns_all_blocks() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/dashboard/learning",
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let d = &body["data"];
    for k in [
        "responseTime",
        "firstAttemptAccuracy",
        "sessionStatus",
        "selfRating",
        "wordAccuracyBins",
        "questionDifficultyMatrix",
        "wordbookLearningStats",
        "masteryDistribution",
        "consecutiveStudyDays",
        "peakTimeHeatmap",
        "generatedAt",
    ] {
        assert!(!d[k].is_null(), "learning 缺分块 {k}: {body}");
    }
}

#[tokio::test]
async fn amas_endpoint_returns_all_blocks() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/dashboard/amas",
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let d = &body["data"];
    for k in [
        "experimentArmComparison",
        "coldStartQuality",
        "eloTrends",
        "generatedAt",
    ] {
        assert!(!d[k].is_null(), "amas 缺分块 {k}: {body}");
    }
}

#[tokio::test]
async fn requires_admin_token_for_sub_endpoints() {
    let app = spawn_test_server().await;
    for path in [
        "/api/admin/dashboard/learning",
        "/api/admin/dashboard/amas",
    ] {
        let resp = request(&app.app, Method::GET, path, None, &[]).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "path {path}");
    }
}
