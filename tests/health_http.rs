mod common;

use axum::http::{Method, StatusCode};
use std::time::Duration;

use common::app::spawn_test_server;
use common::http::{request, response_json};
use learning_backend::store::operations::system_settings::SystemSettings;

#[tokio::test]
async fn it_health_live_and_ready() {
    let app = spawn_test_server().await;

    let live = request(&app.app, Method::GET, "/health/live", None, &[]).await;
    let (live_status, _, _) = response_json(live).await;
    assert_eq!(live_status, StatusCode::OK);

    let ready = request(&app.app, Method::GET, "/health/ready", None, &[]).await;
    let (ready_status, _, _) = response_json(ready).await;
    assert_eq!(ready_status, StatusCode::OK);
}

#[tokio::test]
async fn it_health_database_is_ok() {
    let app = spawn_test_server().await;

    use common::auth::{auth_header, setup_admin_and_get_token};
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let db = request(
        &app.app,
        Method::GET,
        "/health/database",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(db).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["healthy"], true);
}

#[tokio::test]
async fn it_public_health_hides_upstream_url_and_reports_store() {
    let app = spawn_test_server().await;

    let response = request(&app.app, Method::GET, "/health", None, &[]).await;
    let (status, _, body) = response_json(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["services"]["store"]["healthy"], true);
    assert!(body["services"]["wordbookCenter"]["healthy"].is_boolean());
    assert!(body["services"]["wordbookCenter"]["probeSkipped"].is_boolean());
    assert!(body["services"]["wordbookCenter"].get("url").is_none());
}

#[tokio::test]
async fn it_public_health_skips_slow_wordbook_center_probe() {
    let app = spawn_test_server().await;
    let mut settings: SystemSettings = app.state.store().get_system_settings().unwrap();
    settings.wordbook_center_url = Some("https://10.255.255.1/slow-health".to_string());
    app.state.store().save_system_settings(&settings).unwrap();

    let started_at = tokio::time::Instant::now();
    let response = request(&app.app, Method::GET, "/health", None, &[]).await;
    let elapsed = started_at.elapsed();
    let (status, _, body) = response_json(response).await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        elapsed < Duration::from_secs(1),
        "public /health should stay fast, got {:?}",
        elapsed
    );
    assert_eq!(body["services"]["wordbookCenter"]["healthy"], true);
    assert_eq!(body["services"]["wordbookCenter"]["probeSkipped"], true);
}
