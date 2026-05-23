//! v1.1-P1 S2：事件总线集成测试。
//!
//! 覆盖：
//! 1. emit → consumer 接收（单测：直接对 InMemoryEventBus）
//! 2. records POST → AppState event_bus 收到 RecordCreated
//! 3. NoopEventBus / 测试旁路：emit 不报错且 subscribe 返回 None
//! 4. 主路径 records → AMAS 同步通路仍生效（amasResult 仍在响应中）

mod common;

use std::sync::Arc;
use std::time::Duration;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token};
use common::http::{request, response_json};

use learning_backend::services::event_bus::{
    DomainEvent, EventBus, InMemoryEventBus, NoopEventBus,
};

#[tokio::test]
async fn records_post_emits_record_created_event() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    // 在发请求前先订阅，避免错过事件
    let mut rx = app
        .state
        .event_bus()
        .subscribe()
        .expect("event-bus feature 默认启用，subscribe 应成功");

    let create = request(
        &app.app,
        Method::POST,
        "/api/records",
        Some(serde_json::json!({
            "wordId": "w-bus-1",
            "isCorrect": true,
            "responseTimeMs": 800,
            "sessionId": "s-bus-1"
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;

    let (status, _, body) = response_json(create).await;
    assert_eq!(status, StatusCode::CREATED);
    let record_id = body["data"]["record"]["id"].as_str().unwrap().to_string();
    // 主路径 AMAS 同步通路保留：响应中仍有 amasResult
    assert!(
        body["data"]["amasResult"]["strategy"].is_object(),
        "AMAS 同步通路应保留并返回 amasResult"
    );

    // consumer 应在 1s 内收到事件
    let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("等待 RecordCreated 超时")
        .expect("RecordCreated 接收失败");

    let DomainEvent::RecordCreated {
        user_id,
        word_id,
        record_id: ev_record_id,
        is_correct,
        response_time_ms,
        session_id,
        ..
    } = event;
    assert!(!user_id.is_empty());
    assert_eq!(word_id, "w-bus-1");
    assert_eq!(ev_record_id, record_id);
    assert!(is_correct);
    assert_eq!(response_time_ms, 800);
    assert_eq!(session_id.as_deref(), Some("s-bus-1"));
}

#[tokio::test]
async fn records_batch_post_emits_one_event_per_record() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    let mut rx = app
        .state
        .event_bus()
        .subscribe()
        .expect("event-bus feature 默认启用");

    let resp = request(
        &app.app,
        Method::POST,
        "/api/records/batch",
        Some(serde_json::json!({
            "records": [
                {"wordId": "wb1", "isCorrect": true, "responseTimeMs": 600},
                {"wordId": "wb2", "isCorrect": false, "responseTimeMs": 1500},
                {"wordId": "wb3", "isCorrect": true, "responseTimeMs": 900},
            ]
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;

    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["data"]["count"], 3);

    let mut got = Vec::new();
    for _ in 0..3 {
        let ev = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("等待 batch 事件超时")
            .expect("接收失败");
        let DomainEvent::RecordCreated { word_id, .. } = ev;
        got.push(word_id);
    }
    got.sort();
    assert_eq!(got, vec!["wb1".to_string(), "wb2".into(), "wb3".into()]);
}

/// 验证 NoopEventBus 注入后 emit 不报错、subscribe 返回 None。
/// 这模拟 feature off 灰度回滚路径，handler 不应因此报错。
#[tokio::test]
async fn noop_bus_emits_silently_and_no_subscribe() {
    let app = spawn_test_server().await;
    // 在 TestApp 内换上 NoopBus（不重启 router；当前 router 持有的是替换前 state，
    // 这里只验证 EventBus trait 行为本身在 Noop 下满足契约。）
    let bus: Arc<dyn EventBus> = Arc::new(NoopEventBus);
    assert!(bus.subscribe().is_none(), "Noop 不可订阅");
    let n = bus.emit(DomainEvent::RecordCreated {
        user_id: "u".into(),
        word_id: "w".into(),
        record_id: "r".into(),
        is_correct: true,
        response_time_ms: 100,
        session_id: None,
        record_type: Default::default(),
        created_at: chrono::Utc::now(),
    });
    assert_eq!(n, 0);
    // 与此同时，AppState 默认 InMemoryEventBus 在 records POST 时不会被 Noop 干扰
    let token = login_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/records",
        Some(serde_json::json!({
            "wordId": "w-noop-coexist",
            "isCorrect": true,
            "responseTimeMs": 500
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::CREATED);
}

/// 直接对 InMemoryEventBus 测 emit 与多订阅者扇出
#[tokio::test]
async fn in_memory_bus_fanout_to_multiple_subscribers() {
    let bus = Arc::new(InMemoryEventBus::default()) as Arc<dyn EventBus>;
    let mut rx1 = bus.subscribe().unwrap();
    let mut rx2 = bus.subscribe().unwrap();
    let delivered = bus.emit(DomainEvent::RecordCreated {
        user_id: "u".into(),
        word_id: "w".into(),
        record_id: "r".into(),
        is_correct: true,
        response_time_ms: 100,
        session_id: None,
        record_type: Default::default(),
        created_at: chrono::Utc::now(),
    });
    assert_eq!(delivered, 2);
    assert!(rx1.recv().await.is_ok());
    assert!(rx2.recv().await.is_ok());
}
