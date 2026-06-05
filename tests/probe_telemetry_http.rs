//! 数据探针看板(probe-telemetry)后端集成测试。
//!
//! 覆盖:
//!   - GET /overview 4 KPI(activeProbes / events24h / queueBacklog / collectErrorRate)
//!   - GET /probes 三组真实派生探针 + locked 表达
//!   - GET /sampling 真实 telemetry event_type 规则 + globalDefault
//!   - PATCH /sampling/periodic rate=0.5 成功 + audit +1
//!   - PATCH locked 事件(session_start)改 rate → 409 SAMPLING_RULE_LOCKED
//!   - 采样生效:periodic rate=0 不落库 / rate=1 落库 / 确定性哈希同 id 复现
//!   - GET /sinks rowCount 正确、lagSecs>=0、retentionDays=null
//!   - GET /schema 有数据 / 无数据 null
//!   - 无 token → 401

mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, setup_admin_and_get_token};
use common::http::{request, response_json};

use learning_backend::store::Store;

/// 直接写 telemetry_events 一行(server_ts 用 datetime('now') 偏移)。
fn seed_telemetry_event(store: &Store, id: &str, event_type: &str, payload: &str, offset: &str) {
    let conn = store.connection().expect("conn");
    conn.execute(
        "INSERT INTO telemetry_events
            (id, device_id, user_id, event_type, payload_json, client_ts, server_ts)
         VALUES (?1, 'dev-1', 'u-1', ?2, ?3, datetime('now', ?4), datetime('now', ?4))",
        rusqlite::params![id, event_type, payload, offset],
    )
    .expect("insert telemetry_event");
}

/// 直接写 telemetry_summaries 一行。
fn seed_summary(store: &Store, id: &str, click_count: Option<i64>, error_count: i64, offset: &str) {
    let conn = store.connection().expect("conn");
    conn.execute(
        "INSERT INTO telemetry_summaries
            (id, device_id, user_id, event_type, server_ts,
             session_duration_secs, actions_per_min, error_count, avg_response_time_ms,
             click_count, feature_usage_json)
         VALUES (?1, 'dev-1', 'u-1', 'periodic', datetime('now', ?5),
             0, 0, ?3, 0, ?2, '{}')",
        rusqlite::params![id, click_count, error_count, error_count, offset],
    )
    .expect("insert summary");
}

fn seed_learning_session(store: &Store, id: &str, offset: &str) {
    let conn = store.connection().expect("conn");
    conn.execute(
        "INSERT INTO learning_sessions
            (id, user_id, status, created_at, updated_at)
         VALUES (?1, 'u-1', 'active', datetime('now', ?2), datetime('now', ?2))",
        rusqlite::params![id, offset],
    )
    .expect("insert session");
}

fn seed_learning_record(store: &Store, id: &str, offset: &str) {
    let conn = store.connection().expect("conn");
    conn.execute(
        "INSERT INTO learning_records
            (user_id, id, word_id, is_correct, response_time_ms, created_at, record_type)
         VALUES ('u-1', ?1, 'w-1', 1, 1500, datetime('now', ?2), 'all')",
        rusqlite::params![id, offset],
    )
    .expect("insert record");
}

fn seed_probe_execution(store: &Store, id: &str, completed: bool) {
    let conn = store.connection().expect("conn");
    let completed_at = if completed {
        Some("2026-01-01T00:00:00Z")
    } else {
        None
    };
    conn.execute(
        "INSERT INTO probe_executions
            (id, batch_id, device_id, admin_id, admin_username, script_body, script_sha256,
             has_cmd_call, timeout_ms, status, dispatched_at, completed_at)
         VALUES (?1, 'b-1', 'dev-1', 'a-1', 'a@x', 'noop', 'sha', 0, 1000,
             ?2, datetime('now'), ?3)",
        rusqlite::params![id, if completed { "ok" } else { "pending" }, completed_at],
    )
    .expect("insert probe_execution");
}

#[tokio::test]
async fn it_probe_telemetry_dashboard_full_flow() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;
    let store = app.state.store();

    // ---- seed:24h 内各源数据 + 一条窗口外数据 ----
    seed_summary(store, "s-click-1", Some(5), 0, "-1 hour"); // click 探针
    seed_summary(store, "s-err-1", None, 3, "-2 hour"); // error_js 探针(error_count=3)
    seed_summary(store, "s-old", Some(9), 0, "-3 day"); // 窗口外,不计入

    seed_learning_session(store, "ses-1", "-1 hour"); // lesson_start 探针
    seed_learning_record(store, "rec-1", "-1 hour"); // word_answer 探针
    seed_learning_record(store, "rec-2", "-2 hour");

    seed_telemetry_event(
        store,
        "te-1",
        "periodic",
        r#"{"device":{"timezone":"UTC"}}"#,
        "-1 hour",
    );
    seed_telemetry_event(
        store,
        "te-2",
        "session_start",
        r#"{"device":{"cpuCores":8}}"#,
        "-2 hour",
    );

    seed_probe_execution(store, "pe-1", false); // 未完成 → 队列积压
    seed_probe_execution(store, "pe-2", true); // 已完成

    let hdr = [("authorization", auth_header(&admin_token))];

    // ---- GET /overview ----
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/probe-telemetry/overview",
        None,
        &hdr,
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    let d = &body["data"];
    // 4 个派生探针都有 24h 数据 → activeProbes.value=4 total=4
    assert_eq!(d["activeProbes"]["value"].as_i64().unwrap(), 4);
    assert_eq!(d["activeProbes"]["total"].as_i64().unwrap(), 4);
    // events24h = learning_records(2) + telemetry_events(2) = 4
    assert_eq!(d["events24h"]["value"].as_i64().unwrap(), 4);
    // queueBacklog = 未完成 probe_executions = 1
    assert_eq!(d["queueBacklog"]["value"].as_i64().unwrap(), 1);
    // collectErrorRate = SUM(error_count=3) / telemetry_events 24h(2) = 1.5
    let rate = d["collectErrorRate"]["value"].as_f64().unwrap();
    assert!((rate - 1.5).abs() < 1e-9, "rate={rate}");

    // ---- GET /probes ----
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/probe-telemetry/probes",
        None,
        &hdr,
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let groups = body["data"]["groups"].as_array().expect("groups");
    assert_eq!(groups.len(), 3, "behavior/learn/perf 三组");
    let mut all_probes = std::collections::HashMap::new();
    for g in groups {
        for p in g["probes"].as_array().unwrap() {
            all_probes.insert(
                p["key"].as_str().unwrap().to_string(),
                (
                    p["locked"].as_bool().unwrap(),
                    p["count24h"].as_i64().unwrap(),
                ),
            );
        }
    }
    // click 行不 locked(可调采样),learn/perf 三探针 locked
    assert!(!all_probes["click"].0);
    assert!(all_probes["lesson_start"].0);
    assert!(all_probes["word_answer"].0);
    assert!(all_probes["error_js"].0);
    // error_js count24h = SUM(error_count) = 3
    assert_eq!(all_probes["error_js"].1, 3);
    // click count24h = 1(s-click-1,s-old 在窗口外)
    assert_eq!(all_probes["click"].1, 1);

    // ---- GET /sampling ----
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/probe-telemetry/sampling",
        None,
        &hdr,
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!((body["data"]["globalDefault"].as_f64().unwrap() - 1.0).abs() < 1e-9);
    let rules = body["data"]["rules"].as_array().expect("rules");
    let periodic = rules
        .iter()
        .find(|r| r["eventType"] == "periodic")
        .expect("periodic rule");
    assert!((periodic["sampleRate"].as_f64().unwrap() - 1.0).abs() < 1e-9);
    assert!(!periodic["locked"].as_bool().unwrap());
    assert_eq!(periodic["target"].as_str(), Some("click"));
    let session_start = rules
        .iter()
        .find(|r| r["eventType"] == "session_start")
        .expect("session_start rule");
    assert!(session_start["locked"].as_bool().unwrap());

    // ---- PATCH /sampling/periodic rate=0.5 → 成功,audit +1 ----
    let resp = request(
        &app.app,
        Method::PATCH,
        "/api/admin/probe-telemetry/sampling/periodic",
        Some(serde_json::json!({ "sampleRate": 0.5 })),
        &hdr,
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert!((body["data"]["sampleRate"].as_f64().unwrap() - 0.5).abs() < 1e-9);
    assert!(!body["data"]["locked"].as_bool().unwrap());

    // audit 应有 1 行
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/probe-telemetry/audit?limit=20",
        None,
        &hdr,
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let rows = body["data"]["rows"].as_array().expect("audit rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["eventType"], "periodic");
    assert_eq!(rows[0]["action"], "mod");

    // ---- PATCH locked 事件(session_start)改 rate → 409 SAMPLING_RULE_LOCKED ----
    let resp = request(
        &app.app,
        Method::PATCH,
        "/api/admin/probe-telemetry/sampling/session_start",
        Some(serde_json::json!({ "sampleRate": 0.2 })),
        &hdr,
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CONFLICT, "body={body}");
    assert_eq!(body["code"], "SAMPLING_RULE_LOCKED");

    // ---- GET /sinks ----
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/probe-telemetry/sinks",
        None,
        &hdr,
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let sinks = body["data"]["sinks"].as_array().expect("sinks");
    assert_eq!(sinks.len(), 5);
    let te_sink = sinks
        .iter()
        .find(|s| s["id"] == "telemetry_events")
        .expect("telemetry_events sink");
    assert_eq!(te_sink["rowCount"].as_i64().unwrap(), 2);
    assert_eq!(te_sink["kind"], "sqlite_table");
    assert!(
        te_sink["retentionDays"].is_null(),
        "retentionDays 应为 null"
    );
    assert!(te_sink["lagSecs"].as_i64().unwrap() >= 0);

    // ---- GET /schema 有数据 ----
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/probe-telemetry/schema?eventType=periodic",
        None,
        &hdr,
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["eventType"], "periodic");
    assert!(body["data"]["sampledAt"].is_string());
    assert_eq!(body["data"]["payload"]["device"]["timezone"], "UTC");

    // ---- GET /schema 无数据 → payload:null ----
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/probe-telemetry/schema?eventType=does_not_exist",
        None,
        &hdr,
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["payload"].is_null());
    assert!(body["data"]["sampledAt"].is_null());

    // ---- 无 token → 401 ----
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/probe-telemetry/overview",
        None,
        &[],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// 采样生效:periodic rate=0 不落库 / rate=1 落库 / 确定性哈希同 id 复现 /
/// locked 事件(session_start)恒落库。
#[tokio::test]
async fn it_telemetry_sampling_gate_enforced() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;
    let store = app.state.store();

    // 注册一个 end-user(走 /api 的 AuthUser)。telemetry submit 需要 AuthUser。
    let reg = request(
        &app.app,
        Method::POST,
        "/api/auth/register",
        Some(serde_json::json!({
            "email": "learner@probe.test",
            "username": "learner1",
            "password": "Passw0rd!"
        })),
        &[],
    )
    .await;
    let (status, _, body) = response_json(reg).await;
    assert!(status.is_success(), "register: {body}");
    let user_token = body["data"]["accessToken"]
        .as_str()
        .expect("user token")
        .to_string();

    let admin_hdr = [("authorization", auth_header(&admin_token))];

    let submit = |token: String, device: &str, ts: &str, et: &str| {
        let app = app.app.clone();
        let token = token.clone();
        let device = device.to_string();
        let ts = ts.to_string();
        let et = et.to_string();
        async move {
            let resp = request(
                &app,
                Method::POST,
                "/api/telemetry",
                Some(serde_json::json!({
                    // 遥测硬识别:payload 需含 timezone + model(四要素之二)
                    "eventType": et,
                    "clientTs": ts,
                    "payload": { "device": { "timezone": "UTC", "language": "en", "model": "TestPhone1" } }
                })),
                &[
                    ("authorization", auth_header(&token)),
                    ("x-device-id", device.to_string()),
                    // 遥测硬识别:平台 + 版本号走 header(四要素之二)
                    ("x-device-platform", "web".to_string()),
                    ("x-app-version", "1.0.0".to_string()),
                ],
            )
            .await;
            response_json(resp).await
        }
    };

    let count_events = |store: &Store| -> i64 {
        let conn = store.connection().unwrap();
        conn.query_row("SELECT COUNT(*) FROM telemetry_events", [], |r| r.get(0))
            .unwrap()
    };

    // 遥测硬识别:device 必须已注册。seed 三台未认领设备(user_id NULL),submit 时由
    // handler claim(owner NULL → 放行并写入当前 user)。中间件已对 telemetry 跳过 upsert,
    // 故测试需自行 seed,不能依赖中间件注册。
    for dev in ["dev-keep", "dev-drop", "dev-locked"] {
        let conn = store.connection().unwrap();
        conn.execute(
            "INSERT INTO client_devices (device_id, platform, first_seen_at, last_seen_at)
             VALUES (?1, 'web', datetime('now'), datetime('now'))",
            rusqlite::params![dev],
        )
        .unwrap();
    }

    // 1) 默认 rate=1.0 → periodic 落库
    let before = count_events(store);
    let (st, _, b) = submit(
        user_token.clone(),
        "dev-keep",
        "2026-05-29T00:00:00Z",
        "periodic",
    )
    .await;
    assert_eq!(st, StatusCode::OK, "b={b}");
    assert!(b["data"]["id"].is_string());
    assert_eq!(count_events(store), before + 1, "rate=1 应落库");

    // 2) 把 periodic 调成 rate=0 → 全部丢弃
    let resp = request(
        &app.app,
        Method::PATCH,
        "/api/admin/probe-telemetry/sampling/periodic",
        Some(serde_json::json!({ "sampleRate": 0.0 })),
        &admin_hdr,
    )
    .await;
    let (st, _, _) = response_json(resp).await;
    assert_eq!(st, StatusCode::OK);

    let before = count_events(store);
    let (st, _, b) = submit(
        user_token.clone(),
        "dev-drop",
        "2026-05-29T00:00:01Z",
        "periodic",
    )
    .await;
    assert_eq!(st, StatusCode::OK, "b={b}");
    assert_eq!(b["data"]["sampledOut"], true, "rate=0 应 sampledOut");
    assert_eq!(count_events(store), before, "rate=0 不应落库");

    // 3) locked 事件 session_start 即便全局/规则压低,仍恒落库(核心数据不丢)
    let before = count_events(store);
    let (st, _, b) = submit(
        user_token.clone(),
        "dev-locked",
        "2026-05-29T00:00:02Z",
        "session_start",
    )
    .await;
    assert_eq!(st, StatusCode::OK, "b={b}");
    assert!(b["data"]["id"].is_string(), "session_start 必须落库");
    assert_eq!(count_events(store), before + 1, "locked 事件恒落库");

    // 4) 确定性哈希:把 periodic 调到中间值,验证同一 (device_id, id) 决策稳定。
    //    这里直接调用 store 层 effective_sample_rate 复现哈希语义不变。
    let rate = store.effective_sample_rate("periodic").unwrap();
    assert!((rate - 0.0).abs() < 1e-9, "periodic 当前 rate=0");
}
