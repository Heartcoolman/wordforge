//! 远程探针 admin REST 集成测试。
//!
//! 覆盖：
//!   - dispatch happy path（device_ids，仅落库，因 device 不在 active_sse 中也算
//!     'offline'，但 list 仍能查到）
//!   - dispatch rate limit（超 10/min → 429）
//!   - dispatch enabled=false → 503 PROBE_DISABLED（用普通 spawn_test_server）
//!   - confirm happy（直接调 service.issue_confirm 后通过端点 consume）
//!   - confirm wrong suffix → 400
//!   - list / get 端点
//!   - results 状态推进

mod common;

use axum::http::{Method, StatusCode};

use common::app::{spawn_test_app_with_probe_enabled, spawn_test_server};
use common::auth::{auth_header, setup_admin_and_get_token};
use common::http::{request, response_json};

fn admin_header(token: &str) -> (&'static str, String) {
    ("authorization", auth_header(token))
}

#[tokio::test]
async fn dispatch_happy_path_offline_device() {
    let test = spawn_test_app_with_probe_enabled().await;
    let admin = setup_admin_and_get_token(&test.app).await;

    let response = request(
        &test.app,
        Method::POST,
        "/api/admin/probe",
        Some(serde_json::json!({
            "targets": { "deviceIds": ["dev-offline-1"] },
            "script": "return 1;",
            "timeoutMs": 3000,
            "note": "整合测试",
        })),
        &[admin_header(&admin)],
    )
    .await;
    let (status, _, body) = response_json(response).await;
    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert!(data["batchId"].is_string());
    assert!(data["dispatched"].as_array().is_some_and(|a| a.is_empty()));
    assert_eq!(
        data["skippedOffline"].as_array().unwrap()[0],
        "dev-offline-1"
    );
}

#[tokio::test]
async fn dispatch_disabled_503() {
    // spawn_test_server 默认 probe.enabled=false
    let test = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&test.app).await;

    let response = request(
        &test.app,
        Method::POST,
        "/api/admin/probe",
        Some(serde_json::json!({
            "targets": { "deviceIds": ["d-1"] },
            "script": "return 1;",
        })),
        &[admin_header(&admin)],
    )
    .await;
    let (status, _, body) = response_json(response).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "PROBE_DISABLED");
}

#[tokio::test]
async fn dispatch_validates_script_size_and_timeout() {
    let test = spawn_test_app_with_probe_enabled().await;
    let admin = setup_admin_and_get_token(&test.app).await;

    // script 超 16KB
    let huge = "x".repeat(17 * 1024);
    let resp = request(
        &test.app,
        Method::POST,
        "/api/admin/probe",
        Some(serde_json::json!({
            "targets": { "deviceIds": ["d-1"] },
            "script": huge,
        })),
        &[admin_header(&admin)],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "PROBE_SCRIPT_TOO_LARGE");

    // timeout 超范围
    let resp2 = request(
        &test.app,
        Method::POST,
        "/api/admin/probe",
        Some(serde_json::json!({
            "targets": { "deviceIds": ["d-1"] },
            "script": "return 1;",
            "timeoutMs": 999_999,
        })),
        &[admin_header(&admin)],
    )
    .await;
    let (status2, _, body2) = response_json(resp2).await;
    assert_eq!(status2, StatusCode::BAD_REQUEST);
    assert_eq!(body2["code"], "PROBE_TIMEOUT_OUT_OF_RANGE");

    // 空 targets
    let resp3 = request(
        &test.app,
        Method::POST,
        "/api/admin/probe",
        Some(serde_json::json!({
            "targets": { "deviceIds": [] },
            "script": "return 1;",
        })),
        &[admin_header(&admin)],
    )
    .await;
    let (status3, _, body3) = response_json(resp3).await;
    assert_eq!(status3, StatusCode::BAD_REQUEST);
    assert_eq!(body3["code"], "PROBE_INVALID_TARGETS");
}

#[tokio::test]
async fn rate_limit_blocks_after_10_per_min() {
    let test = spawn_test_app_with_probe_enabled().await;
    let admin = setup_admin_and_get_token(&test.app).await;

    for _ in 0..10 {
        let resp = request(
            &test.app,
            Method::POST,
            "/api/admin/probe",
            Some(serde_json::json!({
                "targets": { "deviceIds": ["d-x"] },
                "script": "return 1;",
            })),
            &[admin_header(&admin)],
        )
        .await;
        let (status, _, _) = response_json(resp).await;
        assert_eq!(status, StatusCode::OK);
    }

    let resp = request(
        &test.app,
        Method::POST,
        "/api/admin/probe",
        Some(serde_json::json!({
            "targets": { "deviceIds": ["d-x"] },
            "script": "return 1;",
        })),
        &[admin_header(&admin)],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn list_returns_recent_batches() {
    let test = spawn_test_app_with_probe_enabled().await;
    let admin = setup_admin_and_get_token(&test.app).await;

    // 下发 3 次
    for i in 0..3 {
        let _ = request(
            &test.app,
            Method::POST,
            "/api/admin/probe",
            Some(serde_json::json!({
                "targets": { "deviceIds": [format!("dev-{}", i)] },
                "script": format!("return {};", i),
            })),
            &[admin_header(&admin)],
        )
        .await;
    }

    let resp = request(
        &test.app,
        Method::GET,
        "/api/admin/probe?limit=10",
        None,
        &[admin_header(&admin)],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let total = body["data"]["total"].as_u64().unwrap();
    assert!(total >= 3);
    let rows = body["data"]["rows"].as_array().unwrap();
    assert!(rows.len() >= 3);
}

#[tokio::test]
async fn confirm_endpoint_wrong_suffix_returns_400() {
    let test = spawn_test_app_with_probe_enabled().await;
    let admin = setup_admin_and_get_token(&test.app).await;

    // 手动 issue_confirm 一个 ticket
    use learning_backend::services::probe::ConfirmTicket;
    test.state.probe_service().issue_confirm(
        "req-abc",
        ConfirmTicket {
            token: "tok-xyz".to_string(),
            device_id: "device-12345".to_string(),
            expires_at: std::time::Instant::now() + std::time::Duration::from_secs(60),
        },
    );

    let resp = request(
        &test.app,
        Method::POST,
        "/api/admin/probe/req-abc/confirm",
        Some(serde_json::json!({ "deviceIdSuffix": "wrong" })),
        &[admin_header(&admin)],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "PROBE_CONFIRM_SUFFIX_MISMATCH");
}

#[tokio::test]
async fn confirm_endpoint_happy_path() {
    let test = spawn_test_app_with_probe_enabled().await;
    let admin = setup_admin_and_get_token(&test.app).await;

    // 先 dispatch 一次（落库，确保 confirmed_at UPDATE 不报 row not found）
    let dispatch_resp = request(
        &test.app,
        Method::POST,
        "/api/admin/probe",
        Some(serde_json::json!({
            "targets": { "deviceIds": ["dev-XYZAB"] },
            "script": "return 1;",
        })),
        &[admin_header(&admin)],
    )
    .await;
    let (_, _, body) = response_json(dispatch_resp).await;
    let batch_id = body["data"]["batchId"].as_str().unwrap();
    let _ = batch_id;
    // 取出 request_id 不直接可得（dispatched 为空因 device offline），改从 list 拉
    let list_resp = request(
        &test.app,
        Method::GET,
        "/api/admin/probe?limit=1",
        None,
        &[admin_header(&admin)],
    )
    .await;
    let (_, _, list_body) = response_json(list_resp).await;
    let request_id = list_body["data"]["rows"][0]["id"]
        .as_str()
        .expect("request_id")
        .to_string();

    use learning_backend::services::probe::ConfirmTicket;
    test.state.probe_service().issue_confirm(
        &request_id,
        ConfirmTicket {
            token: "tok-1".to_string(),
            device_id: "dev-XYZAB".to_string(),
            expires_at: std::time::Instant::now() + std::time::Duration::from_secs(60),
        },
    );

    let resp = request(
        &test.app,
        Method::POST,
        &format!("/api/admin/probe/{request_id}/confirm"),
        Some(serde_json::json!({ "deviceIdSuffix": "XYZAB" })),
        &[admin_header(&admin)],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["data"]["confirmed"], true);
}

#[tokio::test]
async fn results_endpoint_advances_status() {
    let test = spawn_test_app_with_probe_enabled().await;
    let admin = setup_admin_and_get_token(&test.app).await;
    let (user_token, _) = common::auth::login_and_get_tokens(&test.app).await;

    // 1. admin dispatch（device offline → row 状态 'offline'；不能用 offline 测
    //    results 推进，因为 results 端点拒绝终态行）
    // 2. 改路径：直接通过 store::insert_probe_executions 注入一行 'pending'
    use learning_backend::store::operations::probe::ProbeInsert;
    test.state
        .store()
        .insert_probe_executions(&[ProbeInsert {
            id: "req-direct",
            batch_id: "b-direct",
            device_id: "dev-direct",
            admin_id: "adm-test",
            admin_username: "adm-test",
            script_body: "return 1;",
            script_sha256: "sha",
            has_cmd_call: false,
            note: None,
            timeout_ms: 3000,
            status: "pending",
            dispatched_at: "2026-05-19T00:00:00Z",
        }])
        .unwrap();

    let _ = admin;
    let resp = request(
        &test.app,
        Method::POST,
        "/api/probe/results",
        Some(serde_json::json!({
            "requestId": "req-direct",
            "status": "ok",
            "resultJson": { "answer": 42 },
            "durationMs": 100,
            "truncated": false,
        })),
        &[
            ("authorization", auth_header(&user_token)),
            ("x-device-id", "dev-direct".to_string()),
            ("x-device-platform", "test".to_string()),
        ],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["data"]["ok"], true);

    // 验证 DB 中状态已推进
    let row = test
        .state
        .store()
        .get_probe_execution("req-direct")
        .unwrap()
        .expect("row");
    assert_eq!(row.status, "ok");
    assert_eq!(row.duration_ms, Some(100));
}

#[tokio::test]
async fn unsupported_ctx_version_recorded() {
    let test = spawn_test_app_with_probe_enabled().await;
    let (user_token, _) = common::auth::login_and_get_tokens(&test.app).await;

    use learning_backend::store::operations::probe::ProbeInsert;
    test.state
        .store()
        .insert_probe_executions(&[ProbeInsert {
            id: "req-old-ctx",
            batch_id: "b-old",
            device_id: "dev-old",
            admin_id: "adm",
            admin_username: "adm",
            script_body: "return 1;",
            script_sha256: "x",
            has_cmd_call: false,
            note: None,
            timeout_ms: 3000,
            status: "pending",
            dispatched_at: "2026-05-19T00:00:00Z",
        }])
        .unwrap();

    let resp = request(
        &test.app,
        Method::POST,
        "/api/probe/results",
        Some(serde_json::json!({
            "requestId": "req-old-ctx",
            "status": "unsupported_ctx_version",
            "stderr": "client v1, server v2",
            "durationMs": 0,
            "truncated": false,
        })),
        &[
            ("authorization", auth_header(&user_token)),
            ("x-device-id", "dev-old".to_string()),
            ("x-device-platform", "test".to_string()),
        ],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);

    let row = test
        .state
        .store()
        .get_probe_execution("req-old-ctx")
        .unwrap()
        .unwrap();
    assert_eq!(row.status, "unsupported_ctx_version");
}
