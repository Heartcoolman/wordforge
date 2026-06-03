//! W2-2：定时广播队列端点集成测试（排程 → 列出 → 取消 → 409）。

mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, setup_admin_and_get_token};
use common::http::{request, response_json};

#[tokio::test]
async fn schedule_list_and_cancel_flow() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    // 排程一条未来广播。
    let future = (chrono::Utc::now() + chrono::Duration::hours(3)).to_rfc3339();
    let create = request(
        &app.app,
        Method::POST,
        "/api/admin/broadcast",
        Some(serde_json::json!({
            "title": "维护通知",
            "message": "今晚 02:00 例行维护",
            "scheduledAt": future,
        })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _, body) = response_json(create).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["scheduled"], true);
    let id = body["data"]["broadcastId"].as_str().unwrap().to_string();

    // 列出待发：一条。
    let list = request(
        &app.app,
        Method::GET,
        "/api/admin/broadcast/scheduled",
        None,
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (ls, _, lb) = response_json(list).await;
    assert_eq!(ls, StatusCode::OK);
    let items = lb["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], id);
    assert_eq!(items[0]["title"], "维护通知");

    // 取消：成功。
    let cancel = request(
        &app.app,
        Method::DELETE,
        &format!("/api/admin/broadcast/scheduled/{id}"),
        None,
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (cs, _, cb) = response_json(cancel).await;
    assert_eq!(cs, StatusCode::OK);
    assert_eq!(cb["data"]["canceled"], true);

    // 列表清空。
    let list2 = request(
        &app.app,
        Method::GET,
        "/api/admin/broadcast/scheduled",
        None,
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (_, _, lb2) = response_json(list2).await;
    assert!(lb2["data"]["items"].as_array().unwrap().is_empty());

    // 再次取消 → 409（已 canceled，非 pending）。
    let cancel2 = request(
        &app.app,
        Method::DELETE,
        &format!("/api/admin/broadcast/scheduled/{id}"),
        None,
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (cs2, _, _) = response_json(cancel2).await;
    assert_eq!(cs2, StatusCode::CONFLICT);
}
