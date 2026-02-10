mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::http::{request, response_json};

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

    let db = request(&app.app, Method::GET, "/health/database", None, &[]).await;
    let (status, _, body) = response_json(db).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["healthy"], true);
}
