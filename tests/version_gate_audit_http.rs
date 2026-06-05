//! W4-4：客户端版本门控变更补 admin 审计留痕。两条入口（version-gate 专用端点 +
//! update_settings 通用端点）改门控字段后均应在 update_audit_log 落行。

mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, setup_admin_and_get_token};
use common::http::{request, response_json};

#[tokio::test]
async fn set_version_gate_writes_audit() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        Method::PUT,
        "/api/admin/settings/version-gate",
        Some(serde_json::json!({ "enabled": true, "minClientVersion": "1.2.0" })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);

    let audits = app
        .state
        .store()
        .list_admin_audit_for_target("settings", "version_gate", 10)
        .unwrap();
    let a = audits
        .iter()
        .find(|a| a.action == "set_version_gate")
        .expect("门控变更应落审计行");
    let meta = a.metadata_json.as_deref().unwrap_or("");
    assert!(meta.contains("1.2.0"), "审计应记 new min_client_version: {meta}");
    assert!(meta.contains("newEnabled"), "审计应记 enabled 翻转: {meta}");
}

#[tokio::test]
async fn update_settings_gate_change_writes_audit_only_when_touched() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    let store = app.state.store();

    // 不碰门控字段的 settings 更新 → 不写门控审计。
    let r1 = request(
        &app.app,
        Method::PUT,
        "/api/admin/settings",
        Some(serde_json::json!({ "maxUsers": 9999 })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    assert_eq!(response_json(r1).await.0, StatusCode::OK);
    assert_eq!(
        store
            .list_admin_audit_for_target("settings", "version_gate", 10)
            .unwrap()
            .len(),
        0,
        "无关 settings 更新不应刷门控审计"
    );

    // 改门控字段 → 写审计。
    let r2 = request(
        &app.app,
        Method::PUT,
        "/api/admin/settings",
        Some(serde_json::json!({ "versionGateEnabled": true, "minClientVersion": "2.0.0" })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    assert_eq!(response_json(r2).await.0, StatusCode::OK);
    let audits = store
        .list_admin_audit_for_target("settings", "version_gate", 10)
        .unwrap();
    assert_eq!(audits.len(), 1, "改门控字段应落一行审计");
    assert!(audits[0].metadata_json.as_deref().unwrap_or("").contains("2.0.0"));
}
