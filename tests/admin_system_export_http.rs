mod common;

use axum::body::{to_bytes, Body};
use axum::http::{Method, StatusCode};
use axum::response::Response;
use std::collections::HashSet;

use common::app::spawn_test_server;
use common::auth::{auth_header, setup_admin_and_get_token};
use common::http::{request, response_json};

const EXPORT: &str = "/api/admin/system/export";
const ADMINS: &str = "/api/admin/settings/admins";

fn h(token: &str) -> [(&'static str, String); 1] {
    [("authorization", auth_header(token))]
}

/// 读取 NDJSON 响应体，逐行解析为 JSON。
async fn collect_lines(resp: Response) -> Vec<serde_json::Value> {
    let bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read ndjson body");
    let text = String::from_utf8(bytes.to_vec()).expect("utf8 body");
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("each line is json"))
        .collect()
}

#[tokio::test]
async fn super_admin_exports_all_tables_as_ndjson() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await; // 首个 admin = super_admin

    let resp = request(&app.app, Method::GET, EXPORT, None, &h(&token)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("ndjson"), "content-type = {ct}");

    let lines = collect_lines(resp).await;

    // 首行 _meta 携带完整表清单（含空表），使 dump 自描述。
    let meta = &lines[0];
    assert_eq!(meta["table"], "_meta", "首行应为 _meta");
    let meta_tables: HashSet<String> = meta["data"]["tables"]
        .as_array()
        .expect("_meta.data.tables 数组")
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    for t in ["users", "admins", "schema_version"] {
        assert!(meta_tables.contains(t), "表清单缺少 {t}；实际: {meta_tables:?}");
    }

    // 有数据的表应产出数据行（admins / schema_version 在 setup 后必非空）。
    let data_tables: HashSet<String> = lines
        .iter()
        .filter_map(|v| v["table"].as_str())
        .filter(|t| *t != "_meta")
        .map(|t| t.to_string())
        .collect();
    for t in ["admins", "schema_version"] {
        assert!(data_tables.contains(t), "导出缺少表 {t} 的数据行；实际: {data_tables:?}");
    }
}

#[tokio::test]
async fn regular_admin_forbidden() {
    let app = spawn_test_server().await;
    let super_token = setup_admin_and_get_token(&app.app).await;

    // super_admin 邀请一个 role=admin 的普通管理员
    let email = format!("ra-{}@test.com", uuid::Uuid::new_v4());
    let pw = "Regular123!";
    let resp = request(
        &app.app,
        Method::POST,
        ADMINS,
        Some(serde_json::json!({ "email": email, "password": pw, "role": "admin" })),
        &h(&super_token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CREATED, "invite failed: {body}");

    // 普通 admin 登录拿 token
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/login",
        Some(serde_json::json!({ "email": email, "password": pw })),
        &[],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "login failed: {body}");
    let regular_token = body["data"]["token"].as_str().expect("login token").to_string();

    let resp = request(&app.app, Method::GET, EXPORT, None, &h(&regular_token)).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn missing_token_unauthorized() {
    let app = spawn_test_server().await;
    let req = axum::http::Request::builder()
        .method(Method::GET)
        .uri(EXPORT)
        .body(Body::empty())
        .unwrap();
    let resp = tower::util::ServiceExt::oneshot(app.app.clone(), req)
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
