//! 覆盖 src/routes/admin/monitoring.rs + src/routes/admin/probe.rs 的集成测试。
//!
//! monitoring：/health、/database、/check-update（fallback + 缓存命中）、/workers
//! 各成功路径 + 401 未授权分支。
//! probe：list（filter / disabled / 401）、get（成功 / 404 / disabled）、
//! confirm（disabled / NotFound / Expired）、dispatch 的 PROBE_SCRIPT_EMPTY /
//! note trim / all_online 无在线设备分支。

mod common;

use axum::http::{Method, StatusCode};

use common::app::{spawn_test_app_with_probe_enabled, spawn_test_server};
use common::auth::{auth_header, setup_admin_and_get_token};
use common::http::{request, response_json};

use learning_backend::services::probe::ConfirmTicket;
use learning_backend::store::operations::probe::ProbeInsert;

fn admin_header(token: &str) -> (&'static str, String) {
    ("authorization", auth_header(token))
}

// ───────────────────────── monitoring：/health ─────────────────────────

#[tokio::test]
async fn monitoring_health_ok() {
    let test = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&test.app).await;

    let resp = request(
        &test.app,
        Method::GET,
        "/api/admin/monitoring/health",
        None,
        &[admin_header(&admin)],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);
    let data = &body["data"];
    // store probe 在测试 DB 上可用 → 状态非 down
    assert_eq!(data["storeProbeOk"], true);
    assert!(data["status"].is_string());
    assert_ne!(data["status"], "down");
    assert!(data["dbSizeBytes"].is_number());
    assert!(data["uptimeSecs"].is_number());
    assert!(data["version"].is_string());
    assert!(data["errorRate"].is_number());
    // services 子结构
    assert!(data["services"]["amas"]["healthy"].is_boolean());
    assert!(data["services"]["sse"]["activeConnections"].is_number());
    assert!(data["services"]["sse"]["activeDevices"].is_number());
    assert!(data["services"]["wordbookCenter"]["healthy"].is_boolean());
}

#[tokio::test]
async fn monitoring_health_requires_admin_token() {
    let test = spawn_test_server().await;
    let resp = request(
        &test.app,
        Method::GET,
        "/api/admin/monitoring/health",
        None,
        &[],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ───────────────────────── monitoring：/database ─────────────────────────

#[tokio::test]
async fn monitoring_database_stats_ok() {
    let test = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&test.app).await;

    let resp = request(
        &test.app,
        Method::GET,
        "/api/admin/monitoring/database",
        None,
        &[admin_header(&admin)],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    // 迁移后至少存在若干业务表
    let table_count = data["tableCount"].as_u64().expect("tableCount");
    assert!(table_count > 0, "tableCount should be > 0");
    assert_eq!(data["tables"].as_array().unwrap().len() as u64, table_count);
    assert!(data["sizeOnDisk"].is_number());
    assert!(data["pageSize"].as_u64().unwrap() > 0);
    assert!(data["pageCount"].as_u64().unwrap() > 0);
    assert!(data["walEnabled"].is_boolean());
}

#[tokio::test]
async fn monitoring_database_requires_admin_token() {
    let test = spawn_test_server().await;
    let resp = request(
        &test.app,
        Method::GET,
        "/api/admin/monitoring/database",
        None,
        &[("authorization", auth_header("not-a-real-token"))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ─────────────────── monitoring：/check-update（fallback + 缓存） ───────────────────

#[tokio::test]
async fn monitoring_check_update_fallback_when_api_url_empty() {
    // 测试 Config 的 update_check.api_url 为空串 → 走 fallback 分支。
    let test = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&test.app).await;

    let resp = request(
        &test.app,
        Method::GET,
        "/api/admin/monitoring/check-update",
        None,
        &[admin_header(&admin)],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert_eq!(data["hasUpdate"], false);
    assert!(data["currentVersion"].is_string());
    assert!(data["latestVersion"].is_string());
    assert!(data["releaseUrl"].is_null());
    assert!(data["releaseNotes"].is_null());
}

#[tokio::test]
async fn monitoring_check_update_cache_hit_on_second_call() {
    // 首次写缓存，二次命中（cache_ttl_secs=3600）→ 仍返回 fallback 数据。
    let test = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&test.app).await;

    let first = request(
        &test.app,
        Method::GET,
        "/api/admin/monitoring/check-update",
        None,
        &[admin_header(&admin)],
    )
    .await;
    let (s1, _, b1) = response_json(first).await;
    assert_eq!(s1, StatusCode::OK);

    let second = request(
        &test.app,
        Method::GET,
        "/api/admin/monitoring/check-update",
        None,
        &[admin_header(&admin)],
    )
    .await;
    let (s2, _, b2) = response_json(second).await;
    assert_eq!(s2, StatusCode::OK);
    // 两次返回内容一致（缓存命中）
    assert_eq!(b1["data"]["currentVersion"], b2["data"]["currentVersion"]);
    assert_eq!(b2["data"]["hasUpdate"], false);
}

#[tokio::test]
async fn monitoring_check_update_requires_admin_token() {
    let test = spawn_test_server().await;
    let resp = request(
        &test.app,
        Method::GET,
        "/api/admin/monitoring/check-update",
        None,
        &[],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ───────────────────────── monitoring：/workers ─────────────────────────

#[tokio::test]
async fn monitoring_workers_ok_empty() {
    // 测试环境不启动 worker，故 list_worker_last_run 返回空数组也算成功。
    let test = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&test.app).await;

    let resp = request(
        &test.app,
        Method::GET,
        "/api/admin/monitoring/workers",
        None,
        &[admin_header(&admin)],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["workers"].is_array());
}

#[tokio::test]
async fn monitoring_workers_reflects_recorded_run() {
    // 直接通过 store 写入一条 worker 执行记录，再断言端点能返回。
    let test = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&test.app).await;

    test.state
        .store()
        .upsert_worker_last_run("cov_test_worker", 1_700_000_000, 123, "success", None)
        .expect("record worker run");

    let resp = request(
        &test.app,
        Method::GET,
        "/api/admin/monitoring/workers",
        None,
        &[admin_header(&admin)],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let workers = body["data"]["workers"].as_array().expect("workers array");
    let found = workers
        .iter()
        .find(|w| w["workerName"] == "cov_test_worker")
        .expect("recorded worker present");
    assert_eq!(found["lastDurationMs"], 123);
    assert_eq!(found["lastOutcome"], "success");
    assert!(found["lastError"].is_null());
}

#[tokio::test]
async fn monitoring_workers_requires_admin_token() {
    let test = spawn_test_server().await;
    let resp = request(
        &test.app,
        Method::GET,
        "/api/admin/monitoring/workers",
        None,
        &[],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

// ───────────────────────── probe：list ─────────────────────────

#[tokio::test]
async fn probe_list_disabled_503() {
    let test = spawn_test_server().await; // probe.enabled = false
    let admin = setup_admin_and_get_token(&test.app).await;

    let resp = request(
        &test.app,
        Method::GET,
        "/api/admin/probe",
        None,
        &[admin_header(&admin)],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "PROBE_DISABLED");
}

#[tokio::test]
async fn probe_list_requires_admin_token() {
    let test = spawn_test_app_with_probe_enabled().await;
    let resp = request(&test.app, Method::GET, "/api/admin/probe", None, &[]).await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn probe_list_with_status_and_device_filters() {
    let test = spawn_test_app_with_probe_enabled().await;
    let admin = setup_admin_and_get_token(&test.app).await;

    // 注入两条不同 status/device 的行
    test.state
        .store()
        .insert_probe_executions(&[
            ProbeInsert {
                id: "cov-req-ok",
                batch_id: "cov-batch-1",
                device_id: "cov-dev-ok",
                admin_id: "adm",
                admin_username: "adm",
                script_body: "return 1;",
                script_sha256: "h1",
                has_cmd_call: false,
                note: None,
                timeout_ms: 3000,
                status: "ok",
                dispatched_at: "2026-05-19T00:00:01Z",
            },
            ProbeInsert {
                id: "cov-req-err",
                batch_id: "cov-batch-1",
                device_id: "cov-dev-err",
                admin_id: "adm",
                admin_username: "adm",
                script_body: "return 2;",
                script_sha256: "h2",
                has_cmd_call: false,
                note: None,
                timeout_ms: 3000,
                status: "error",
                dispatched_at: "2026-05-19T00:00:02Z",
            },
        ])
        .unwrap();

    // status 过滤
    let resp = request(
        &test.app,
        Method::GET,
        "/api/admin/probe?status=ok&limit=50",
        None,
        &[admin_header(&admin)],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let rows = body["data"]["rows"].as_array().unwrap();
    assert!(rows.iter().all(|r| r["status"] == "ok"));
    assert!(rows.iter().any(|r| r["id"] == "cov-req-ok"));

    // device 过滤
    let resp2 = request(
        &test.app,
        Method::GET,
        "/api/admin/probe?deviceId=cov-dev-err",
        None,
        &[admin_header(&admin)],
    )
    .await;
    let (status2, _, body2) = response_json(resp2).await;
    assert_eq!(status2, StatusCode::OK);
    let rows2 = body2["data"]["rows"].as_array().unwrap();
    assert_eq!(rows2.len(), 1);
    assert_eq!(rows2[0]["id"], "cov-req-err");
    assert_eq!(body2["data"]["total"], 1);

    // batch 过滤 → 两条
    let resp3 = request(
        &test.app,
        Method::GET,
        "/api/admin/probe?batchId=cov-batch-1",
        None,
        &[admin_header(&admin)],
    )
    .await;
    let (status3, _, body3) = response_json(resp3).await;
    assert_eq!(status3, StatusCode::OK);
    assert_eq!(body3["data"]["total"], 2);
}

// ───────────────────────── probe：get by-id ─────────────────────────

#[tokio::test]
async fn probe_get_by_id_ok() {
    let test = spawn_test_app_with_probe_enabled().await;
    let admin = setup_admin_and_get_token(&test.app).await;

    test.state
        .store()
        .insert_probe_executions(&[ProbeInsert {
            id: "cov-get-1",
            batch_id: "cov-batch-get",
            device_id: "cov-dev-get",
            admin_id: "adm",
            admin_username: "adm",
            script_body: "return 1;",
            script_sha256: "h",
            has_cmd_call: true,
            note: Some("一条备注"),
            timeout_ms: 5000,
            status: "pending",
            dispatched_at: "2026-05-19T00:00:00Z",
        }])
        .unwrap();

    let resp = request(
        &test.app,
        Method::GET,
        "/api/admin/probe/by-id/cov-get-1",
        None,
        &[admin_header(&admin)],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert_eq!(data["id"], "cov-get-1");
    assert_eq!(data["deviceId"], "cov-dev-get");
    assert_eq!(data["hasCmdCall"], true);
    assert_eq!(data["note"], "一条备注");
    assert_eq!(data["timeoutMs"], 5000);
    assert_eq!(data["status"], "pending");
}

#[tokio::test]
async fn probe_get_by_id_not_found_404() {
    let test = spawn_test_app_with_probe_enabled().await;
    let admin = setup_admin_and_get_token(&test.app).await;

    let resp = request(
        &test.app,
        Method::GET,
        "/api/admin/probe/by-id/does-not-exist",
        None,
        &[admin_header(&admin)],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["success"], false);
}

#[tokio::test]
async fn probe_get_by_id_disabled_503() {
    let test = spawn_test_server().await; // probe disabled
    let admin = setup_admin_and_get_token(&test.app).await;

    let resp = request(
        &test.app,
        Method::GET,
        "/api/admin/probe/by-id/whatever",
        None,
        &[admin_header(&admin)],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "PROBE_DISABLED");
}

// ───────────────────────── probe：confirm 错误分支 ─────────────────────────

#[tokio::test]
async fn probe_confirm_disabled_503() {
    let test = spawn_test_server().await; // probe disabled
    let admin = setup_admin_and_get_token(&test.app).await;

    let resp = request(
        &test.app,
        Method::POST,
        "/api/admin/probe/req-x/confirm",
        Some(serde_json::json!({ "deviceIdSuffix": "12345" })),
        &[admin_header(&admin)],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "PROBE_DISABLED");
}

#[tokio::test]
async fn probe_confirm_not_found_404() {
    let test = spawn_test_app_with_probe_enabled().await;
    let admin = setup_admin_and_get_token(&test.app).await;

    // 未 issue_confirm → ticket 不存在 → NotFound → 404
    let resp = request(
        &test.app,
        Method::POST,
        "/api/admin/probe/no-such-ticket/confirm",
        Some(serde_json::json!({ "deviceIdSuffix": "12345" })),
        &[admin_header(&admin)],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["success"], false);
}

#[tokio::test]
async fn probe_confirm_expired_400() {
    let test = spawn_test_app_with_probe_enabled().await;
    let admin = setup_admin_and_get_token(&test.app).await;

    // issue 一个已过期 ticket（expires_at 在过去）→ Expired → 400
    test.state.probe_service().issue_confirm(
        "cov-expired-req",
        ConfirmTicket {
            token: "tok-expired".to_string(),
            device_id: "device-99999".to_string(),
            expires_at: std::time::Instant::now() - std::time::Duration::from_secs(1),
        },
    );

    let resp = request(
        &test.app,
        Method::POST,
        "/api/admin/probe/cov-expired-req/confirm",
        Some(serde_json::json!({ "deviceIdSuffix": "99999" })),
        &[admin_header(&admin)],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "PROBE_CONFIRM_EXPIRED");
}

// ───────────────────────── probe：dispatch 额外错误分支 ─────────────────────────

#[tokio::test]
async fn probe_dispatch_empty_script_400() {
    let test = spawn_test_app_with_probe_enabled().await;
    let admin = setup_admin_and_get_token(&test.app).await;

    let resp = request(
        &test.app,
        Method::POST,
        "/api/admin/probe",
        Some(serde_json::json!({
            "targets": { "deviceIds": ["d-1"] },
            "script": "   ",
        })),
        &[admin_header(&admin)],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "PROBE_SCRIPT_EMPTY");
}

#[tokio::test]
async fn probe_dispatch_all_online_no_devices_400() {
    let test = spawn_test_app_with_probe_enabled().await;
    let admin = setup_admin_and_get_token(&test.app).await;

    // allOnline=true 但当前无在线设备 → device_ids 为空 → PROBE_INVALID_TARGETS
    let resp = request(
        &test.app,
        Method::POST,
        "/api/admin/probe",
        Some(serde_json::json!({
            "targets": { "allOnline": true },
            "script": "return 1;",
        })),
        &[admin_header(&admin)],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "PROBE_INVALID_TARGETS");
}

#[tokio::test]
async fn probe_dispatch_trims_note_and_blank_device_ids() {
    let test = spawn_test_app_with_probe_enabled().await;
    let admin = setup_admin_and_get_token(&test.app).await;

    // 含空白 deviceId（被过滤）+ 带前后空格的 note（被 trim）
    let resp = request(
        &test.app,
        Method::POST,
        "/api/admin/probe",
        Some(serde_json::json!({
            "targets": { "deviceIds": ["  ", "  cov-trim-dev  "] },
            "script": "return 1;",
            "note": "  备注前后空格  ",
        })),
        &[admin_header(&admin)],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    // 空白 deviceId 被过滤，只剩 1 个 offline 目标
    let skipped = body["data"]["skippedOffline"].as_array().unwrap();
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0], "cov-trim-dev");

    // 验证 note 已 trim 落库
    let list = request(
        &test.app,
        Method::GET,
        "/api/admin/probe?deviceId=cov-trim-dev",
        None,
        &[admin_header(&admin)],
    )
    .await;
    let (_, _, list_body) = response_json(list).await;
    let rows = list_body["data"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["note"], "备注前后空格");
    assert_eq!(rows[0]["status"], "offline");
}

#[tokio::test]
async fn probe_dispatch_requires_admin_token() {
    let test = spawn_test_app_with_probe_enabled().await;
    let resp = request(
        &test.app,
        Method::POST,
        "/api/admin/probe",
        Some(serde_json::json!({
            "targets": { "deviceIds": ["d-1"] },
            "script": "return 1;",
        })),
        &[],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
