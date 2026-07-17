mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token};
use common::http::{request, response_json};

#[tokio::test]
async fn it_record_create_and_query() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    let create = request(
        &app.app,
        Method::POST,
        "/api/records",
        Some(serde_json::json!({
            "wordId": "w-test",
            "isCorrect": true,
            "responseTimeMs": 1200,
            "sessionId": "s-1"
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;

    let (status, _, body) = response_json(create).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(body["data"]["record"]["id"].is_string());
    assert!(body["data"]["amasResult"]["strategy"].is_object());

    let list = request(
        &app.app,
        Method::GET,
        "/api/records?limit=50",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;

    let (list_status, _, list_body) = response_json(list).await;
    assert_eq!(list_status, StatusCode::OK);
    assert!(list_body["data"]["data"].is_array());
    assert!(!list_body["data"]["data"].as_array().unwrap().is_empty());
}

/// Phase 1 集成检查点（去重不变式，server 侧）：两个不同 clientRecordId、同一 wordId 的真实
/// HTTP 并发请求（tokio::join!，非顺序 await），命中 create_record_with_updates 里
/// word_learning_states 的写路径。修复前：两个请求各自在 tx 外读到 total_attempts=0，都算出
/// "1"，后写者用自己的"1"覆盖前一次的"1"，最终停在 1；修复后：SQL 相对自增让两次都真实计入，
/// 最终必须是 2。这条测试打真实路由+真实 DB，比 store::operations::records 模块内的单元测试更
/// 贴近线上并发场景（客户端两个请求同时抵达网关），是本轮四仓库审查唯一能在自动化里做到的
/// 端到端级验证——iOS/Android/web 三端的等效验证需要真机/模拟器网络层面模拟进程杀死时机，
/// 不在本仓库自动化测试范围内。
#[tokio::test]
async fn concurrent_records_same_word_do_not_lose_attempt_increments() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    let hdr = [("authorization", auth_header(&token))];

    let body_a = serde_json::json!({
        "wordId": "w-concurrent-integration",
        "isCorrect": true,
        "responseTimeMs": 900,
        "clientRecordId": "cr-integration-a"
    });
    let body_b = serde_json::json!({
        "wordId": "w-concurrent-integration",
        "isCorrect": true,
        "responseTimeMs": 1100,
        "clientRecordId": "cr-integration-b"
    });

    let (resp_a, resp_b) = tokio::join!(
        request(&app.app, Method::POST, "/api/records", Some(body_a), &hdr),
        request(&app.app, Method::POST, "/api/records", Some(body_b), &hdr),
    );

    let (status_a, _, json_a) = response_json(resp_a).await;
    let (status_b, _, json_b) = response_json(resp_b).await;
    assert_eq!(status_a, StatusCode::CREATED);
    assert_eq!(status_b, StatusCode::CREATED);
    assert_ne!(
        json_a["data"]["record"]["id"], json_b["data"]["record"]["id"],
        "两个不同 clientRecordId 必须各自建立独立记录，不能被误判为同一事件的重复提交"
    );

    let state = request(
        &app.app,
        Method::GET,
        "/api/word-states/w-concurrent-integration",
        None,
        &hdr,
    )
    .await;
    let (state_status, _, state_json) = response_json(state).await;
    assert_eq!(state_status, StatusCode::OK);
    assert_eq!(
        state_json["data"]["totalAttempts"], 2,
        "两次真实并发提交都必须计入 total_attempts；停在 1 就是本轮修复要消除的丢增量 bug"
    );
}
