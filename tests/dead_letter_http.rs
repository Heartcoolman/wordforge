//! W1-2：outbox 死信运维端点集成测试（列出 / 重投 / 丢弃）。

mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, setup_admin_and_get_token};
use common::http::{request, response_json};

/// 直接经 store 种一条死信（enqueue → claim → move_to_dead_letter）。
fn seed_dead_letter(store: &learning_backend::store::Store, payload: &str) -> i64 {
    store.enqueue_outbox_event("record_created", payload).unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    let ev = store.claim_due_outbox_events(&now, 1).unwrap().remove(0);
    store.move_outbox_to_dead_letter(&ev, 5, "exhausted").unwrap();
    store.list_dead_letter(1).unwrap()[0].id
}

#[tokio::test]
async fn dead_letter_list_requeue_and_purge() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    let store = app.state.store();

    let id_a = seed_dead_letter(store, "{\"user_id\":\"u-1\",\"k\":1}");
    let id_b = seed_dead_letter(store, "{\"user_id\":\"u-2\",\"k\":2}");

    // 列出：两条死信，user_id best-effort 解析。
    let list = request(
        &app.app,
        Method::GET,
        "/api/admin/monitoring/dead-letter",
        None,
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _, body) = response_json(list).await;
    assert_eq!(status, StatusCode::OK);
    let entries = body["data"]["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().any(|e| e["userId"] == "u-1"));

    // 重投 id_a：回 outbox（attempts 归零），死信只剩 id_b。
    let requeue = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/monitoring/dead-letter/{id_a}/requeue"),
        None,
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (rq_status, _, rq_body) = response_json(requeue).await;
    assert_eq!(rq_status, StatusCode::OK);
    assert_eq!(rq_body["data"]["requeued"], true);

    let stats = store.outbox_stats().unwrap();
    assert_eq!(stats.dead_letter, 1, "重投后死信剩 1 条");
    let due = store
        .claim_due_outbox_events(&chrono::Utc::now().to_rfc3339(), 10)
        .unwrap();
    assert_eq!(due.len(), 1, "重投的事件回到 outbox");
    assert_eq!(due[0].attempts, 0, "重投后 attempts 归零");

    // 丢弃 id_b：死信清空。
    let purge = request(
        &app.app,
        Method::DELETE,
        &format!("/api/admin/monitoring/dead-letter/{id_b}"),
        None,
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (pg_status, _, pg_body) = response_json(purge).await;
    assert_eq!(pg_status, StatusCode::OK);
    assert_eq!(pg_body["data"]["purged"], true);
    assert_eq!(store.outbox_stats().unwrap().dead_letter, 0);

    // 重投已不存在的 id → 404。
    let missing = request(
        &app.app,
        Method::POST,
        "/api/admin/monitoring/dead-letter/99999/requeue",
        None,
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (miss_status, _, _) = response_json(missing).await;
    assert_eq!(miss_status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn dead_letter_requires_admin_auth() {
    let app = spawn_test_server().await;
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/monitoring/dead-letter",
        None,
        &[],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
