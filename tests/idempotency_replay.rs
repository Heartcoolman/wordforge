//! W1-1：records→AMAS 领域事件幂等账本（processed_events）集成测试。
//!
//! 核心不变式：当一条记录的 AMAS 状态已在先前（可能崩溃的）尝试中应用过（processed_events
//! 标记存在），重放/重试**不得二次累加** AMAS 状态与 ELO，同时记录行仍被补落（不丢）。

mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token};
use common::http::{request, response_json};

async fn post_record(
    app: &axum::Router,
    token: &str,
    client_record_id: &str,
    word_id: &str,
) -> (StatusCode, serde_json::Value) {
    let resp = request(
        app,
        Method::POST,
        "/api/records",
        Some(serde_json::json!({
            "clientRecordId": client_record_id,
            "wordId": word_id,
            "isCorrect": true,
            "responseTimeMs": 1000,
        })),
        &[("authorization", auth_header(token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    (status, body)
}

/// 崩溃窗口重放不二次累加 AMAS/ELO，且记录不丢。
#[tokio::test]
async fn replay_with_existing_idempotency_marker_does_not_double_apply() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    // 首次正常上报：AMAS 应用 + ELO 推进 + 记录落库 + 幂等标记写入。
    let (status, body) = post_record(&app.app, &token, "rec-A", "w-A").await;
    assert_eq!(status, StatusCode::CREATED);
    let user_id = body["data"]["record"]["userId"]
        .as_str()
        .expect("record.userId")
        .to_string();

    let store = app.state.store();
    assert!(
        store.is_event_processed(&user_id, "rec-A").unwrap(),
        "首次上报应原子写入幂等标记"
    );
    let elo_after_first = store.get_user_elo(&user_id).unwrap();
    let amas_state_after_first = store.get_engine_user_state(&user_id).unwrap();

    // 模拟"AMAS 已应用 + 标记已写,但记录行未落库(崩溃)"的窗口：预置 rec-B 的幂等标记。
    store.mark_event_processed(&user_id, "rec-B").unwrap();

    // 重放 rec-B：check_duplicate 未命中(无记录行)→ is_event_processed 命中 → 裸记录路径,
    // 不再调 process_event。
    let (replay_status, _replay_body) = post_record(&app.app, &token, "rec-B", "w-B").await;
    assert!(
        replay_status == StatusCode::OK || replay_status == StatusCode::CREATED,
        "重放应成功返回,实际 {replay_status}"
    );

    // 记录行已补落（不丢失学习历史）。
    assert!(
        store
            .get_user_record_by_id(&user_id, "rec-B")
            .unwrap()
            .is_some(),
        "命中幂等标记时裸记录仍应补落"
    );

    // AMAS 状态与 user ELO 未被 rec-B 二次累加（games 计数不变、状态 JSON 不变）。
    let elo_after_replay = store.get_user_elo(&user_id).unwrap();
    let amas_state_after_replay = store.get_engine_user_state(&user_id).unwrap();
    assert_eq!(
        elo_after_first.games, elo_after_replay.games,
        "命中幂等标记的重放不得二次累加 user ELO"
    );
    assert_eq!(
        amas_state_after_first, amas_state_after_replay,
        "命中幂等标记的重放不得二次累加 AMAS 状态"
    );
}

/// 普通重复上报（同 clientRecordId、记录行已存在）：check_duplicate 短路，返回 duplicate，
/// AMAS/ELO 不二次累加。
#[tokio::test]
async fn duplicate_client_record_id_short_circuits_without_double_apply() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    let (status, body) = post_record(&app.app, &token, "rec-dup", "w-dup").await;
    assert_eq!(status, StatusCode::CREATED);
    let user_id = body["data"]["record"]["userId"]
        .as_str()
        .unwrap()
        .to_string();
    let store = app.state.store();
    let elo_first = store.get_user_elo(&user_id).unwrap();

    // 同 clientRecordId 再次上报 → 200 OK + duplicate=true。
    let (dup_status, dup_body) = post_record(&app.app, &token, "rec-dup", "w-dup").await;
    assert_eq!(dup_status, StatusCode::OK);
    assert_eq!(dup_body["data"]["duplicate"], true);

    let elo_second = store.get_user_elo(&user_id).unwrap();
    assert_eq!(elo_first.games, elo_second.games, "重复上报不得二次累加 ELO");
}
