//! AMAS 配置 JSON Schema 导出测试
//!
//! 双重职责：
//! 1. 验证 `GET /api/admin/amas/config/schema` endpoint 行为正确（admin 鉴权、含关键字段）。
//! 2. 把当前 schema 离线写入 `admin-ui/src/types/amas.schema.json`，
//!    供前端 codegen 在无后端运行时也能生成 `amas.generated.ts`，
//!    并通过 CI `git diff --exit-code` 防止后端 struct 与前端类型漂移。

mod common;

use std::path::PathBuf;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, setup_admin_and_get_token};
use common::http::{request, response_json};

#[tokio::test]
async fn schema_endpoint_returns_full_amas_config_shape() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let response = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/config/schema",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(response).await;
    assert_eq!(status, StatusCode::OK, "schema endpoint must be 200: {body}");

    // 响应包装：{ data: <schema> }
    let schema = &body["data"];
    assert!(schema.is_object(), "schema 必须是对象");

    // 根 schema 应描述 AMASConfig
    let title = schema["title"].as_str().unwrap_or_default();
    assert_eq!(title, "AMASConfig", "schema title 必须为 AMASConfig");

    // 关键顶层字段（camelCase）
    let props = &schema["properties"];
    for required_key in [
        "featureFlags",
        "ensemble",
        "modeling",
        "constraints",
        "monitoring",
        "coldStart",
        "objectiveWeights",
        "memoryModel",
        "ssp",
        "wordSelector",
    ] {
        assert!(
            props.get(required_key).is_some(),
            "schema.properties 缺少字段 {required_key}: {schema}"
        );
    }

    // definitions 内应至少包含 MemoryModelConfig（FSRS w[19] 数组）
    let defs = &schema["definitions"];
    let mem = &defs["MemoryModelConfig"];
    assert!(
        mem.is_object(),
        "definitions.MemoryModelConfig 不存在: {schema}"
    );
    let w = &mem["properties"]["w"];
    assert!(
        w.is_object(),
        "MemoryModelConfig.properties.w 应存在（FSRS-5 权重数组）"
    );
}

#[tokio::test]
async fn schema_endpoint_requires_admin_auth() {
    let app = spawn_test_server().await;
    // 无 token
    let response = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/config/schema",
        None,
        &[],
    )
    .await;
    let (status, _, _) = response_json(response).await;
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN,
        "未鉴权访问应被拒绝，实际：{status}"
    );
}

/// 把当前 schemars 生成的 schema 写入 `admin-ui/src/types/amas.schema.json`。
///
/// 这一步把"运行后端"解耦为"运行 cargo test"——前端 codegen 不需要在线后端。
/// CI 在 `cargo test` 后跑 `pnpm gen:amas-types`，再 `git diff --exit-code`
/// 验证后端结构体未与前端 TS 类型漂移。
#[test]
fn export_amas_config_schema_to_admin_ui() {
    let schema = schemars::schema_for!(learning_backend::amas::config::AMASConfig);
    let pretty = serde_json::to_string_pretty(&schema).expect("schema serialize");

    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("admin-ui/src/types/amas.schema.json");

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create types dir");
    }
    std::fs::write(&path, format!("{pretty}\n")).expect("write schema file");
}
