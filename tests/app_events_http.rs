//! /api/telemetry/app-events（m073 埋点事件流）集成测试：
//!   - 合法批部分成功（逐条错误不整批 400）
//!   - (deviceId, clientEventId) 跨请求幂等重放 → duplicates
//!   - 超批 400 APP_EVENTS_TOO_LARGE / 缺头 400 + 拒绝码留痕 / owner 403
//!   - 采样门（app_behavior rate=0 丢弃、error 恒留）
//!   - clientTsMs 钳制与 event_day 落库

mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token};
use common::http::{request, response_json};

use learning_backend::store::Store;

fn hard_headers(token: &str, device_id: &str) -> [(&'static str, String); 4] {
    [
        ("authorization", auth_header(token)),
        ("x-device-id", device_id.to_string()),
        ("x-device-platform", "web".to_string()),
        ("x-app-version", "1.0.0".to_string()),
    ]
}

fn seed_unclaimed_device(store: &Store, device_id: &str) {
    let conn = store.connection().unwrap();
    conn.execute(
        "INSERT OR IGNORE INTO client_devices (device_id, platform, first_seen_at, last_seen_at)
         VALUES (?1, 'web', datetime('now'), datetime('now'))",
        rusqlite::params![device_id],
    )
    .unwrap();
}

fn seed_owned_device(store: &Store, device_id: &str, owner_user_id: &str) {
    let conn = store.connection().unwrap();
    conn.execute(
        "INSERT OR IGNORE INTO client_devices (device_id, platform, user_id, first_seen_at, last_seen_at)
         VALUES (?1, 'web', ?2, datetime('now'), datetime('now'))",
        rusqlite::params![device_id, owner_user_id],
    )
    .unwrap();
}

fn event(id: &str, name: &str, category: &str) -> serde_json::Value {
    serde_json::json!({
        "clientEventId": id,
        "name": name,
        "category": category,
        "clientTsMs": chrono::Utc::now().timestamp_millis(),
    })
}

#[tokio::test]
async fn it_app_events_valid_batch_partial_success() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    seed_unclaimed_device(app.state.store(), "dev-ae-1");

    let mut ok_event = event("e1", "screen_view", "behavior");
    ok_event["props"] = serde_json::json!({"screen": "home"});
    let resp = request(
        &app.app,
        Method::POST,
        "/api/telemetry/app-events",
        Some(serde_json::json!({ "events": [
            ok_event,
            event("e2", "BadName", "behavior"),       // 大写 → APP_EVENT_INVALID_NAME
            event("e3", "screen_view", "banana"),     // 非法 category
            event("e1", "screen_view", "behavior"),   // 请求内重复 id
        ]})),
        &hard_headers(&token, "dev-ae-1"),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let data = &body["data"];
    assert_eq!(data["accepted"], 1);
    assert_eq!(data["failed"], 3);
    let codes: Vec<&str> = data["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"APP_EVENT_INVALID_NAME"));
    assert!(codes.contains(&"APP_EVENT_INVALID_CATEGORY"));
    assert!(codes.contains(&"APP_EVENT_DUPLICATE_ID"));

    // 落库校验：event_day 由钳制后 clientTsMs 计算
    let conn = app.state.store().connection().unwrap();
    let (n, day): (i64, String) = conn
        .query_row(
            "SELECT COUNT(*), MAX(event_day) FROM app_events WHERE device_id='dev-ae-1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(n, 1);
    assert_eq!(day, chrono::Utc::now().format("%Y-%m-%d").to_string());
}

#[tokio::test]
async fn it_app_events_replay_counts_duplicates() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    seed_unclaimed_device(app.state.store(), "dev-ae-2");

    let batch = serde_json::json!({ "events": [
        event("r1", "session_start", "behavior"),
        event("r2", "word_lookup", "behavior"),
    ]});
    let resp = request(
        &app.app,
        Method::POST,
        "/api/telemetry/app-events",
        Some(batch.clone()),
        &hard_headers(&token, "dev-ae-2"),
    )
    .await;
    let (_, _, body) = response_json(resp).await;
    assert_eq!(body["data"]["accepted"], 2);

    // 同批重放 → 全部 duplicates，零新行
    let resp = request(
        &app.app,
        Method::POST,
        "/api/telemetry/app-events",
        Some(batch),
        &hard_headers(&token, "dev-ae-2"),
    )
    .await;
    let (_, _, body) = response_json(resp).await;
    assert_eq!(body["data"]["accepted"], 0);
    assert_eq!(body["data"]["duplicates"], 2);
}

#[tokio::test]
async fn it_app_events_oversize_batch_rejected() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    let events: Vec<serde_json::Value> = (0..51)
        .map(|i| event(&format!("b{i}"), "screen_view", "behavior"))
        .collect();
    let resp = request(
        &app.app,
        Method::POST,
        "/api/telemetry/app-events",
        Some(serde_json::json!({ "events": events })),
        &hard_headers(&token, "dev-ae-3"),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "APP_EVENTS_TOO_LARGE");
}

#[tokio::test]
async fn it_app_events_missing_headers_rejected_with_audit() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    // 缺 x-device-id
    let resp = request(
        &app.app,
        Method::POST,
        "/api/telemetry/app-events",
        Some(serde_json::json!({ "events": [] })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "MISSING_DEVICE_ID");

    // 缺 x-device-platform → MISSING_OS + 拒绝码留痕（fire-and-forget，轮询等待）
    let resp = request(
        &app.app,
        Method::POST,
        "/api/telemetry/app-events",
        Some(serde_json::json!({ "events": [] })),
        &[
            ("authorization", auth_header(&token)),
            ("x-device-id", "dev-ae-4".to_string()),
            ("x-app-version", "1.0.0".to_string()),
        ],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "MISSING_OS");
    let mut found = false;
    for _ in 0..50 {
        let n: i64 = app
            .state
            .store()
            .connection()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM telemetry_ingest_rejections
                  WHERE code='MISSING_OS' AND device_id='dev-ae-4'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        if n > 0 {
            found = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(found, "MISSING_OS 拒绝码应留痕");
}

#[tokio::test]
async fn it_app_events_device_ownership_enforced() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;

    // 未注册设备 → 403 DEVICE_NOT_REGISTERED
    let resp = request(
        &app.app,
        Method::POST,
        "/api/telemetry/app-events",
        Some(serde_json::json!({ "events": [event("o1", "session_start", "behavior")] })),
        &hard_headers(&token, "dev-ae-unreg"),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "DEVICE_NOT_REGISTERED");

    // 归属他人 → 403 DEVICE_OWNERSHIP_MISMATCH
    seed_owned_device(app.state.store(), "dev-ae-other", "someone-else");
    let resp = request(
        &app.app,
        Method::POST,
        "/api/telemetry/app-events",
        Some(serde_json::json!({ "events": [event("o2", "session_start", "behavior")] })),
        &hard_headers(&token, "dev-ae-other"),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "DEVICE_OWNERSHIP_MISMATCH");
}

#[tokio::test]
async fn it_app_events_sampling_gate_behavior_only() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    seed_unclaimed_device(app.state.store(), "dev-ae-5");

    // app_behavior 采样率归零（error 恒不采样，不受影响）
    app.state
        .store()
        .connection()
        .unwrap()
        .execute(
            "UPDATE probe_sampling_config SET sample_rate = 0.0 WHERE event_type = 'app_behavior'",
            [],
        )
        .unwrap();

    let mut err_event = event("s2", "app_error", "error");
    err_event["props"] = serde_json::json!({"signature": "abc123", "kind": "js_error"});
    let resp = request(
        &app.app,
        Method::POST,
        "/api/telemetry/app-events",
        Some(serde_json::json!({ "events": [
            event("s1", "screen_view", "behavior"),
            err_event,
        ]})),
        &hard_headers(&token, "dev-ae-5"),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["sampledOut"], 1);
    assert_eq!(body["data"]["accepted"], 1);
    let cat: String = app
        .state
        .store()
        .connection()
        .unwrap()
        .query_row(
            "SELECT category FROM app_events WHERE device_id='dev-ae-5'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cat, "error");
}

#[tokio::test]
async fn it_app_events_client_ts_clamped() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    seed_unclaimed_device(app.state.store(), "dev-ae-6");

    // 30 天前的时间戳 → 钳到 now-7d 的 event_day
    let ancient = chrono::Utc::now().timestamp_millis() - 30 * 86_400_000;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/telemetry/app-events",
        Some(serde_json::json!({ "events": [{
            "clientEventId": "t1",
            "name": "session_start",
            "category": "behavior",
            "clientTsMs": ancient,
        }]})),
        &hard_headers(&token, "dev-ae-6"),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["accepted"], 1);
    let day: String = app
        .state
        .store()
        .connection()
        .unwrap()
        .query_row(
            "SELECT event_day FROM app_events WHERE device_id='dev-ae-6'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let expect = (chrono::Utc::now() - chrono::Duration::days(7))
        .format("%Y-%m-%d")
        .to_string();
    assert_eq!(day, expect);
}
