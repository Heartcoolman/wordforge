mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token, setup_admin_and_get_token};
use common::http::{request, response_json};

// 覆盖 src/routes/admin/settings.rs 全部端点：
//   GET  /api/admin/settings              get_settings
//   PUT  /api/admin/settings              update_settings（含各 validate 分支）
//   POST /api/admin/settings/maintenance  set_maintenance
//   POST /api/admin/settings/reload-amas  reload_amas_config
// 以及 store::operations::system_settings 的读/写/双表落盘路径。

#[tokio::test]
async fn it_get_settings_returns_defaults_for_admin() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/settings",
        None,
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);
    // 默认值断言（来自 SystemSettings::default）
    assert!(body["data"]["maxUsers"].is_number());
    assert_eq!(body["data"]["registrationEnabled"], true);
    assert_eq!(body["data"]["maintenanceMode"], false);
    assert!(body["data"]["defaultDailyWords"].is_number());
    assert!(body["data"]["wordbookCenterUrl"].is_string());
    assert_eq!(body["data"]["amasAutoApplyEnabled"], false);
}

#[tokio::test]
async fn it_get_settings_rejects_non_admin() {
    let app = spawn_test_server().await;
    let user = login_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/settings",
        None,
        &[("authorization", auth_header(&user))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn it_get_settings_rejects_missing_token() {
    let app = spawn_test_server().await;

    let resp = request(&app.app, Method::GET, "/api/admin/settings", None, &[]).await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn it_update_settings_persists_all_sections() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    // 全字段写入：覆盖 update_settings 中每个 if let Some(...) 分支以及双表落盘。
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/admin/settings",
        Some(serde_json::json!({
            "maxUsers": 12345,
            "registrationEnabled": false,
            "maintenanceMode": true,
            "defaultDailyWords": 42,
            "wordbookCenterUrl": "https://example.com/center",
            "amasAutoApplyEnabled": true,
            "amasAutoApplyMaxPerDay": 5,
            "amasAutoApplyMinConfidence": 0.66,
            "llmAdvisorMaxCostPerMonthYuan": 250.0
        })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["maxUsers"], 12345);
    assert_eq!(body["data"]["registrationEnabled"], false);
    assert_eq!(body["data"]["maintenanceMode"], true);
    assert_eq!(body["data"]["defaultDailyWords"], 42);
    assert_eq!(body["data"]["wordbookCenterUrl"], "https://example.com/center");
    assert_eq!(body["data"]["amasAutoApplyEnabled"], true);
    assert_eq!(body["data"]["amasAutoApplyMaxPerDay"], 5);

    // maintenanceMode=true 应同步到内存状态
    assert!(app.state.is_maintenance());

    // 二次 GET 确认持久化（save_system_settings -> get_system_settings 回环）
    let resp2 = request(
        &app.app,
        Method::GET,
        "/api/admin/settings",
        None,
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status2, _, body2) = response_json(resp2).await;
    assert_eq!(status2, StatusCode::OK);
    assert_eq!(body2["data"]["maxUsers"], 12345);
    assert_eq!(body2["data"]["defaultDailyWords"], 42);
    assert_eq!(body2["data"]["maintenanceMode"], true);
}

#[tokio::test]
async fn it_update_settings_empty_wordbook_url_clears_to_default() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    // wordbookCenterUrl 传空串 -> 落库 None -> get 时被替换为默认 URL
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/admin/settings",
        Some(serde_json::json!({ "wordbookCenterUrl": "" })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);

    let resp2 = request(
        &app.app,
        Method::GET,
        "/api/admin/settings",
        None,
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status2, _, body2) = response_json(resp2).await;
    assert_eq!(status2, StatusCode::OK);
    // get_system_settings 在 None 时回填默认 jsdelivr URL
    assert!(body2["data"]["wordbookCenterUrl"]
        .as_str()
        .unwrap()
        .starts_with("https://"));
}

#[tokio::test]
async fn it_update_settings_partial_does_not_touch_maintenance() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    // 仅传 registrationEnabled：maintenance_mode_changed 应为 false，不触发 set_maintenance。
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/admin/settings",
        Some(serde_json::json!({ "registrationEnabled": false })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["registrationEnabled"], false);
    // 未传 maintenanceMode，内存状态保持初始 false
    assert!(!app.state.is_maintenance());
}

#[tokio::test]
async fn it_update_settings_validation_branches_return_400() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    // 逐个触发 UpdateSystemSettings::validate 的每条错误分支。
    let cases: Vec<(serde_json::Value, &str)> = vec![
        (serde_json::json!({ "maxUsers": 0 }), "INVALID_MAX_USERS"),
        (
            serde_json::json!({ "maxUsers": 1_000_001 }),
            "INVALID_MAX_USERS",
        ),
        (
            serde_json::json!({ "defaultDailyWords": 0 }),
            "INVALID_DAILY_WORDS",
        ),
        (
            serde_json::json!({ "defaultDailyWords": 501 }),
            "INVALID_DAILY_WORDS",
        ),
        (
            serde_json::json!({ "amasAutoApplyMaxPerDay": 21 }),
            "INVALID_AUTO_APPLY_LIMIT",
        ),
        (
            serde_json::json!({ "amasAutoApplyMinConfidence": 1.5 }),
            "INVALID_CONFIDENCE",
        ),
        (
            serde_json::json!({ "amasAutoApplyMinConfidence": -0.1 }),
            "INVALID_CONFIDENCE",
        ),
        (
            serde_json::json!({ "llmAdvisorMaxCostPerMonthYuan": 0.5 }),
            "INVALID_LLM_BUDGET",
        ),
        (
            serde_json::json!({ "llmAdvisorMaxCostPerMonthYuan": 10_001.0 }),
            "INVALID_LLM_BUDGET",
        ),
    ];

    for (payload, expected_code) in cases {
        let resp = request(
            &app.app,
            Method::PUT,
            "/api/admin/settings",
            Some(payload.clone()),
            &[("authorization", auth_header(&admin))],
        )
        .await;
        let (status, _, body) = response_json(resp).await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "payload {payload} should be rejected"
        );
        assert_eq!(body["code"], expected_code, "payload {payload}");
    }
}

#[tokio::test]
async fn it_update_settings_boundary_values_accepted() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    // 边界合法值应被接受（区间端点）。
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/admin/settings",
        Some(serde_json::json!({
            "maxUsers": 1,
            "defaultDailyWords": 500,
            "amasAutoApplyMaxPerDay": 20,
            "amasAutoApplyMinConfidence": 1.0,
            "llmAdvisorMaxCostPerMonthYuan": 10_000.0
        })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["maxUsers"], 1);
    assert_eq!(body["data"]["defaultDailyWords"], 500);
    assert_eq!(body["data"]["amasAutoApplyMaxPerDay"], 20);
}

#[tokio::test]
async fn it_update_settings_invalid_body_returns_400() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    // 字段类型错误 -> JsonBody 反序列化失败 -> INVALID_REQUEST_BODY
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/admin/settings",
        Some(serde_json::json!({ "maxUsers": "not-a-number" })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_REQUEST_BODY");
}

#[tokio::test]
async fn it_update_settings_rejects_non_admin() {
    let app = spawn_test_server().await;
    let user = login_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        Method::PUT,
        "/api/admin/settings",
        Some(serde_json::json!({ "maxUsers": 100 })),
        &[("authorization", auth_header(&user))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn it_set_maintenance_toggles_state() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    // 开启维护
    let resp_on = request(
        &app.app,
        Method::POST,
        "/api/admin/settings/maintenance",
        Some(serde_json::json!({ "active": true })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status_on, _, body_on) = response_json(resp_on).await;
    assert_eq!(status_on, StatusCode::OK);
    assert_eq!(body_on["data"]["active"], true);
    assert!(app.state.is_maintenance());

    // GET 确认已落库
    let resp_get = request(
        &app.app,
        Method::GET,
        "/api/admin/settings",
        None,
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (_, _, body_get) = response_json(resp_get).await;
    assert_eq!(body_get["data"]["maintenanceMode"], true);

    // 关闭维护
    let resp_off = request(
        &app.app,
        Method::POST,
        "/api/admin/settings/maintenance",
        Some(serde_json::json!({ "active": false })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status_off, _, body_off) = response_json(resp_off).await;
    assert_eq!(status_off, StatusCode::OK);
    assert_eq!(body_off["data"]["active"], false);
    assert!(!app.state.is_maintenance());
}

#[tokio::test]
async fn it_set_maintenance_invalid_body_returns_400() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    // 缺失必填字段 active -> 反序列化失败
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/settings/maintenance",
        Some(serde_json::json!({})),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_REQUEST_BODY");
}

#[tokio::test]
async fn it_set_maintenance_rejects_non_admin() {
    let app = spawn_test_server().await;
    let user = login_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/settings/maintenance",
        Some(serde_json::json!({ "active": true })),
        &[("authorization", auth_header(&user))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn it_reload_amas_config_accepts_valid_config() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    // 取出当前引擎里的有效配置作为 body，保证 validate 通过。
    let current = app.state.amas().get_config();
    let body = serde_json::to_value(&current).expect("serialize amas config");

    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/settings/reload-amas",
        Some(body),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _, json) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["success"], true);
    // 返回的是 get_config() 结果，应含 monitoring 字段
    assert!(json["data"].is_object());
}

#[tokio::test]
async fn it_reload_amas_config_rejects_invalid_config() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    // 取有效配置，把 monitoring.sampleRate 改成越界值 -> validate 失败 -> INVALID_AMAS_CONFIG
    // 注意：AMASConfig 序列化为 camelCase，字段名是 sampleRate 而非 sample_rate。
    let current = app.state.amas().get_config();
    let mut body = serde_json::to_value(&current).expect("serialize amas config");
    body["monitoring"]["sampleRate"] = serde_json::json!(2.0);

    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/settings/reload-amas",
        Some(body),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _, json) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "INVALID_AMAS_CONFIG");
}

#[tokio::test]
async fn it_reload_amas_config_malformed_body_returns_400() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    // 完全不符合 AMASConfig 结构 -> JsonBody 反序列化失败
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/settings/reload-amas",
        Some(serde_json::json!({ "totally": "wrong" })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_REQUEST_BODY");
}

#[tokio::test]
async fn it_reload_amas_config_rejects_non_admin() {
    let app = spawn_test_server().await;
    let user = login_and_get_token(&app.app).await;

    let current = app.state.amas().get_config();
    let body = serde_json::to_value(&current).expect("serialize amas config");

    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/settings/reload-amas",
        Some(body),
        &[("authorization", auth_header(&user))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
