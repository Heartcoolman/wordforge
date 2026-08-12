mod common;

use axum::http::{Method, StatusCode};

use common::app::{spawn_test_app_with_outbox_async, spawn_test_server};
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
/// 最终必须是 2。
///
/// 能力边界（如实声明）：本测试打真实路由+真实 DB，贴近线上并发场景，但对修复前竞态的捕获是
/// **概率性**的——tokio::join! 只保证两个请求并发发起，二者是否真的在"读快照→写回"窗口内交错
/// 取决于调度时序，单次运行未必命中（即回归时本测试可能仍偶然通过）。该竞态的**确定性**守卫在
/// store 层单测 create_record_with_updates_concurrent_stale_reads_still_increment_correctly
///（显式喂两份同一陈旧快照，必现旧行为）；本测试的价值是端到端串通路由/AMAS/DB 全链路。
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

/// S2（v1.3.0 起默认）：records_outbox_async=true 时单条上报走 202 异步受理——
/// 裸响应 `{accepted, async, clientRecordId}`（无 {success,data} 信封、无 record/amasResult），
/// 事件持久化进 outbox；outbox worker 消费后记录落库、AMAS 应用、幂等标记写入。
#[tokio::test]
async fn single_record_async_outbox_202_then_worker_persists() {
    let app = spawn_test_app_with_outbox_async().await;
    let (token, user_id) = register_user_with_id(&app.app).await;
    let hdr = [("authorization", auth_header(&token))];
    let store = app.state.store();

    let resp = request(
        &app.app,
        Method::POST,
        "/api/records",
        Some(serde_json::json!({
            "clientRecordId": "async-1",
            "wordId": "w-async",
            "isCorrect": true,
            "responseTimeMs": 800
        })),
        &hdr,
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::ACCEPTED, "异步路径应 202 受理: {body}");
    assert_eq!(body["accepted"], true);
    assert_eq!(body["async"], true);
    assert_eq!(body["clientRecordId"], "async-1");
    assert!(body.get("success").is_none(), "202 为裸响应，不走 {{success,data}} 信封");

    assert_eq!(store.outbox_stats().unwrap().pending, 1, "事件应已持久化进 outbox");
    assert!(
        store.get_user_record_by_id(&user_id, "async-1").unwrap().is_none(),
        "202 受理时记录行尚未落库"
    );

    // 消费一轮 outbox：记录落库 + AMAS 应用（幂等标记写入）。
    learning_backend::workers::outbox_processor::run(&app.state).await;

    assert_eq!(store.outbox_stats().unwrap().pending, 0);
    assert_eq!(store.outbox_stats().unwrap().dead_letter, 0);
    assert!(
        store.get_user_record_by_id(&user_id, "async-1").unwrap().is_some(),
        "worker 消费后记录行必须落库"
    );
    assert!(
        store.is_event_processed(&user_id, "async-1").unwrap(),
        "worker 消费后幂等标记必须写入（AMAS 已应用）"
    );
}

/// 注册新用户并返回 (access_token, user_id)：本文件需要 user_id 直查 store 断言引擎状态。
async fn register_user_with_id(app: &axum::Router) -> (String, String) {
    let email = format!("user-{}@test.com", uuid::Uuid::new_v4());
    let username = format!("user-{}", uuid::Uuid::new_v4().simple());
    let resp = request(
        app,
        Method::POST,
        "/api/auth/register",
        Some(serde_json::json!({
            "email": email,
            "username": username,
            "password": "Passw0rd!",
        })),
        &[],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert!(status.is_success(), "register failed: {body}");
    (
        body["data"]["accessToken"].as_str().unwrap().to_string(),
        body["data"]["user"]["id"].as_str().unwrap().to_string(),
    )
}

/// batch 部分失败 + 客户端重试不双累加 user 级状态（S2 收尾后的幂等账本短路语义守卫）：
/// 第二条记录用 selfRating=200 触发 learning_records 的 CHECK 约束（tx2 落库失败）——此时 AMAS
/// 已提交（totalEventCount 前移到 2）且幂等标记保留（「标记存在 ⟺ AMAS 已应用」不变式）；
/// 随后同 clientRecordId 重试（改合法 selfRating）命中幂等账本短路，仅补落裸记录行
///（duplicate=true、无 amasResult），AMAS 不被二次应用——totalEventCount 恒为 2 不再前移。
#[tokio::test]
async fn batch_partial_failure_retry_does_not_double_accumulate_user_state() {
    let app = spawn_test_server().await;
    let (token, user_id) = register_user_with_id(&app.app).await;
    let hdr = [("authorization", auth_header(&token))];
    let store = app.state.store();

    let total_event_count = |store: &learning_backend::store::Store| -> i64 {
        store
            .get_engine_user_state(&user_id)
            .unwrap()
            .and_then(|s| s["totalEventCount"].as_i64())
            .unwrap_or(0)
    };

    // 第一条合法、第二条 selfRating=200 在记录行 INSERT 时违反 CHECK(0..=3) → tx2 失败。
    let batch = request(
        &app.app,
        Method::POST,
        "/api/records/batch",
        Some(serde_json::json!({
            "records": [
                {
                    "clientRecordId": "b-ok-1",
                    "wordId": "w-batch-ok",
                    "isCorrect": true,
                    "responseTimeMs": 700
                },
                {
                    "clientRecordId": "b-fail-2",
                    "wordId": "w-batch-fail",
                    "isCorrect": true,
                    "responseTimeMs": 900,
                    "selfRating": 200
                }
            ]
        })),
        &hdr,
    )
    .await;
    let (batch_status, _, batch_body) = response_json(batch).await;
    assert_eq!(batch_status, StatusCode::OK, "部分失败应 200 partial: {batch_body}");
    assert_eq!(batch_body["data"]["partial"], true);
    assert_eq!(batch_body["data"]["count"], 1);
    assert_eq!(batch_body["data"]["failed"], 1);

    assert_eq!(
        total_event_count(store),
        2,
        "tx2 失败不再回滚 AMAS：两条事件都已应用（有界损失换取无陈旧快照覆盖风险）"
    );
    assert!(
        store.is_event_processed(&user_id, "b-fail-2").unwrap(),
        "失败条目的幂等标记必须保留（标记存在 ⟺ AMAS 已应用），重试据此短路不二次累加"
    );

    // 同 clientRecordId 重试（合法 selfRating）：命中幂等账本短路，补落裸记录行。
    let retry = request(
        &app.app,
        Method::POST,
        "/api/records/batch",
        Some(serde_json::json!({
            "records": [{
                "clientRecordId": "b-fail-2",
                "wordId": "w-batch-fail",
                "isCorrect": true,
                "responseTimeMs": 900,
                "selfRating": 2
            }]
        })),
        &hdr,
    )
    .await;
    let (retry_status, _, retry_body) = response_json(retry).await;
    assert_eq!(retry_status, StatusCode::CREATED, "重试应全量成功: {retry_body}");
    assert_eq!(
        retry_body["data"]["items"][0]["duplicate"], true,
        "重试命中幂等账本短路，按 duplicate 补落裸记录行而非重新应用 AMAS"
    );
    assert!(retry_body["data"]["items"][0]["amasResult"].is_null());

    // 记录行必须已补落（裸回放路径落 learning_records）。
    assert!(
        store.get_user_record_by_id(&user_id, "b-fail-2").unwrap().is_some(),
        "重试后失败条目的记录行必须已补落"
    );

    assert_eq!(
        total_event_count(store),
        2,
        "重试后 user 级状态不得前移；出现 3 即幂等账本短路失效（AMAS 双重累加回归）"
    );
}
