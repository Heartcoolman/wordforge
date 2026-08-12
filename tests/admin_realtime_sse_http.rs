//! 契约 B：admin 专用 SSE 端点 `/api/admin/realtime/events` 的集成测试。
//!
//! 覆盖：
//! 1. admin token 建连成功（200 + text/event-stream）；
//! 2. user token / 无 token 被拒（401）；
//! 3. 事件路由：worker_missed 只投 admin 通道（不出现在用户连接）；
//!    incident 等双通道事件同时到达用户与 admin 连接。

mod common;

use std::time::Instant;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token, setup_admin_and_get_token};
use common::http::request;

use learning_backend::state::{SseClientInfo, SseEvent, SSE_CONN_CHANNEL_CAP};

#[tokio::test]
async fn it_admin_sse_accepts_admin_token() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let response = request(
        &app.app,
        Method::GET,
        "/api/admin/realtime/events",
        None,
        &[("authorization", auth_header(&admin_token))],
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

#[tokio::test]
async fn it_admin_sse_rejects_user_token() {
    let app = spawn_test_server().await;
    let user_token = login_and_get_token(&app.app).await;

    let response = request(
        &app.app,
        Method::GET,
        "/api/admin/realtime/events",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn it_admin_sse_rejects_missing_token() {
    let app = spawn_test_server().await;

    let response = request(
        &app.app,
        Method::GET,
        "/api/admin/realtime/events",
        None,
        &[],
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

fn make_conn(
    conn_id: &str,
    subject_id: &str,
) -> (SseClientInfo, tokio::sync::mpsc::Receiver<SseEvent>) {
    let (tx, rx) = tokio::sync::mpsc::channel(SSE_CONN_CHANNEL_CAP);
    (
        SseClientInfo {
            conn_id: conn_id.to_string(),
            user_id: subject_id.to_string(),
            platform: "test".to_string(),
            connected_at: Instant::now(),
            tx,
        },
        rx,
    )
}

/// worker_missed 走 broadcast_to_admin_sse：admin 连接收到，用户连接绝不出现。
#[tokio::test]
async fn it_worker_missed_only_reaches_admin_channel() {
    let app = spawn_test_server().await;
    let state = &app.state;

    let (user_conn, mut user_rx) = make_conn("user-conn", "user-1");
    state
        .active_sse()
        .entry("user-device".to_string())
        .or_default()
        .push(user_conn);

    let (admin_conn, mut admin_rx) = make_conn("admin-conn", "admin-1");
    state
        .admin_sse()
        .entry("admin-device".to_string())
        .or_default()
        .push(admin_conn);

    state.broadcast_to_admin_sse(SseEvent::WorkerMissed {
        worker_name: "delayed_reward".to_string(),
        miss_count: 3,
    });

    match admin_rx.try_recv().expect("admin 连接应收到 worker_missed") {
        SseEvent::WorkerMissed {
            worker_name,
            miss_count,
        } => {
            assert_eq!(worker_name, "delayed_reward");
            assert_eq!(miss_count, 3);
        }
        other => panic!("expected WorkerMissed, got {other:?}"),
    }
    assert!(
        user_rx.try_recv().is_err(),
        "worker_missed 不应出现在用户 SSE 连接"
    );
}

/// incident / llm_budget_exceeded 等双通道事件：broadcast_to_all_sse 同时投递
/// 用户通道与 admin 通道（iOS 用户端消费它们做横幅/置灰，不能断）。
#[tokio::test]
async fn it_incident_reaches_both_channels() {
    let app = spawn_test_server().await;
    let state = &app.state;

    let (user_conn, mut user_rx) = make_conn("user-conn-2", "user-2");
    state
        .active_sse()
        .entry("user-device-2".to_string())
        .or_default()
        .push(user_conn);

    let (admin_conn, mut admin_rx) = make_conn("admin-conn-2", "admin-2");
    state
        .admin_sse()
        .entry("admin-device-2".to_string())
        .or_default()
        .push(admin_conn);

    state.broadcast_to_all_sse(SseEvent::Incident {
        error_rate: 0.05,
        window_secs: 300,
    });

    assert!(
        matches!(
            user_rx.try_recv(),
            Ok(SseEvent::Incident { window_secs: 300, .. })
        ),
        "incident 应继续到达用户连接"
    );
    assert!(
        matches!(
            admin_rx.try_recv(),
            Ok(SseEvent::Incident { window_secs: 300, .. })
        ),
        "incident 应同时到达 admin 连接"
    );
}

/// 契约 A：upgrade_cleared 序列化必须携带 origin 字段（gate / targeted）。
#[test]
fn upgrade_cleared_serializes_origin() {
    let gate = serde_json::to_value(SseEvent::UpgradeCleared {
        origin: learning_backend::state::UpgradeClearedOrigin::Gate,
    })
    .unwrap();
    assert_eq!(gate["type"], "upgrade_cleared");
    assert_eq!(gate["origin"], "gate");

    let targeted = serde_json::to_value(SseEvent::UpgradeCleared {
        origin: learning_backend::state::UpgradeClearedOrigin::Targeted,
    })
    .unwrap();
    assert_eq!(targeted["origin"], "targeted");
}
