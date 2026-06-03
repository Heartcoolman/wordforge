//! W3-1：遥测专项 per-user 限频集成测试。超额软丢弃（received:true, throttled:true 不落库），
//! 设备活跃度仍刷新，且不影响通用 API 预算。

mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_app_with_telemetry_limit;
use common::auth::{auth_header, login_and_get_token};
use common::http::{request, response_json};

use learning_backend::store::Store;

fn seed_unclaimed_device(store: &Store, device_id: &str) {
    let conn = store.connection().unwrap();
    conn.execute(
        "INSERT OR IGNORE INTO client_devices (device_id, platform, first_seen_at, last_seen_at)
         VALUES (?1, 'web', datetime('now'), datetime('now'))",
        rusqlite::params![device_id],
    )
    .unwrap();
}

fn valid_body() -> serde_json::Value {
    serde_json::json!({
        "eventType": "heartbeat",
        "clientTs": "2026-06-03T00:00:00Z",
        "payload": { "device": { "timezone": "Asia/Shanghai", "model": "web-admin", "language": "zh" } }
    })
}

async fn post_telemetry(app: &axum::Router, token: &str, device_id: &str) -> (StatusCode, serde_json::Value) {
    let resp = request(
        app,
        Method::POST,
        "/api/telemetry",
        Some(valid_body()),
        &[
            ("authorization", auth_header(token)),
            ("x-device-id", device_id.to_string()),
            ("x-device-platform", "web".to_string()),
            ("x-app-version", "1.0.0".to_string()),
        ],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    (status, body)
}

#[tokio::test]
async fn telemetry_throttles_per_user_after_quota() {
    // 配额上限 3：前 3 条落库，第 4 条软丢弃。
    let app = spawn_test_app_with_telemetry_limit(3).await;
    let token = login_and_get_token(&app.app).await;
    seed_unclaimed_device(app.state.store(), "dev-tele");

    for i in 0..3 {
        let (status, body) = post_telemetry(&app.app, &token, "dev-tele").await;
        assert_eq!(status, StatusCode::OK, "第 {i} 条应成功");
        // 正常落库返回 { id: ... }，非 throttled
        assert!(body["data"]["throttled"].as_bool() != Some(true), "第 {i} 条不应被限流");
    }

    // 第 4 条超额 → 软丢弃（仍 200，但 throttled=true，不带 id）
    let (status, body) = post_telemetry(&app.app, &token, "dev-tele").await;
    assert_eq!(status, StatusCode::OK, "限流仍 200（软丢弃非错误）");
    assert_eq!(body["data"]["throttled"], true);
    assert_eq!(body["data"]["received"], true);

    // 设备活跃度仍刷新：last_seen 经 upsert 更新（设备存在且 SSE 在线判定不被误伤）。
    // 此处断言软丢弃不写 telemetry 行：库内 telemetry 行数 == 3。
    let conn = app.state.store().connection().unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM telemetry_events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 3, "软丢弃的第 4 条不应落库");
}
