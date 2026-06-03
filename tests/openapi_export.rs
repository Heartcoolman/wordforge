/// M0-C2：导出 OpenAPI 3.1 规格到 docs/openapi.yaml。
///
/// CI 通过 `git diff --exit-code docs/openapi.yaml` 防止规格漂移（drift）：
/// 每次修改 src/openapi.rs 后必须重新运行此测试更新文档，否则 CI 失败。
///
/// 本地手动运行：
///   cargo test --test openapi_export
#[test]
fn export_openapi_yaml() {
    let api = learning_backend::openapi::build();

    // utoipa yaml feature 在 OpenApi 上提供 .to_yaml() 方法（serde_norway 集成）
    let yaml = api.to_yaml().expect("序列化 OpenAPI 文档为 YAML 失败");

    // 写入 docs/openapi.yaml（路径相对于项目根目录）
    let out_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("docs")
        .join("openapi.yaml");
    std::fs::write(&out_path, &yaml).unwrap_or_else(|e| panic!("写入 {out_path:?} 失败：{e}"));

    // 基本内容断言，防止导出空文档
    assert!(yaml.contains("WordForge API"), "info.title 缺失");
    // W4-1：version 跟随 CARGO_PKG_VERSION（与 Cargo.toml 一致），勿写死旧版本。
    assert!(
        yaml.contains(env!("CARGO_PKG_VERSION")),
        "info.version 应为 CARGO_PKG_VERSION"
    );
    assert!(yaml.contains("/auth/login"), "auth/login 端点缺失");
    assert!(yaml.contains("/auth/register"), "auth/register 端点缺失");
    assert!(yaml.contains("/records"), "records 端点缺失");
    assert!(yaml.contains("/word-states/"), "word-states 端点缺失");
    assert!(yaml.contains("/word-favorites"), "word-favorites 端点缺失");
    assert!(yaml.contains("/realtime/events"), "SSE 端点缺失");
    assert!(yaml.contains("/health"), "health 端点缺失");
    assert!(yaml.contains("bearerAuth"), "安全方案缺失");
    assert!(
        yaml.contains("new/learning/reviewing"),
        "WordState lowercase 枚举缺失"
    );
    assert!(yaml.contains("data.data"), "favorites paginated 说明缺失");

    println!("openapi.yaml 已导出至 {out_path:?}（{} bytes）", yaml.len());
}
