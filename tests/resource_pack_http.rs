//! v1.1-P0.10：资源包热更端到端集成测试。
//!
//! 覆盖：
//!   - GET /api/resource-packs/public-key（匿名）
//!   - POST /api/admin/resource-packs/:pack_id/versions（admin 上传，验 sha256 + signature 一致）
//!   - PUT /api/admin/resource-packs/:pack_id/channel/:channel/active（admin 切激活）
//!   - GET /api/resource-packs/:pack_id/manifest 200 / 304 / 404 / 409
//!   - GET /api/admin/resource-packs（列表）+ /stats（聚合）
//!   - DELETE /api/admin/resource-packs/:pack_id/versions/:version（软删除→manifest 404）
//!   - POST /api/telemetry/resource-pack-install（用户上报 → stats 可见）
//!
//! SSE 端到端送达验证（每帧解析）保持与 tests/realtime_sse_http.rs 同深度（不做），
//! 改测 5 分钟 dedup 的状态可观察性：第二次切同 pack×channel 时 try_mark_pack_broadcast=false。

mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token, setup_admin_and_get_token};
use common::http::{request, response_json};

use base64::Engine as _;
use learning_backend::services::resource_pack_signing::verify_base64;
use learning_backend::store::operations::resource_packs::ResourcePackChannel;
use sha2::{Digest, Sha256};

const SAMPLE_PAYLOAD: &[u8] =
    br#"{"wordbooks":[{"id":"ielts-7000","name":"IELTS 7000"}],"version":"1.2.3"}"#;

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    let mut s = String::with_capacity(64);
    for b in out {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

/// 给 admin 直接构造 raw bytes POST（http helper 默认 JSON content-type）。
async fn upload_pack_payload(
    app: &axum::Router,
    admin_token: &str,
    pack_id: &str,
    version: &str,
    channel: &str,
    payload: &[u8],
    min_app_version: Option<&str>,
) -> axum::http::Response<axum::body::Body> {
    use axum::body::Body;
    use axum::http::Request;
    use tower::util::ServiceExt;

    let mut uri =
        format!("/api/admin/resource-packs/{pack_id}/versions?version={version}&channel={channel}");
    if let Some(min) = min_app_version {
        uri.push_str(&format!("&minAppVersion={min}"));
    }
    let req = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header("authorization", auth_header(admin_token))
        .header("content-type", "application/octet-stream")
        .body(Body::from(payload.to_vec()))
        .expect("request");
    app.clone().oneshot(req).await.expect("oneshot")
}

#[tokio::test]
async fn public_key_endpoint_returns_base64() {
    let app = spawn_test_server().await;
    let resp = request(
        &app.app,
        Method::GET,
        "/api/resource-packs/public-key",
        None,
        &[],
    )
    .await;
    let (status, _h, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    // public-key handler 直接返 Json(json!{...})，无 ok() 包裹
    assert_eq!(body["algorithm"], "ed25519");
    let pk = body["publicKey"].as_str().expect("publicKey");
    assert_eq!(pk.len(), 44, "Ed25519 公钥 base64 长度应为 44");
    base64::engine::general_purpose::STANDARD
        .decode(pk)
        .expect("公钥必须是合法 base64");
}

#[tokio::test]
async fn admin_upload_computes_sha256_and_valid_signature() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    // 拿公钥（用于 verify 上传后端返回的 signature）
    let pk_resp = request(
        &app.app,
        Method::GET,
        "/api/resource-packs/public-key",
        None,
        &[],
    )
    .await;
    let (_s, _h, pk_body) = response_json(pk_resp).await;
    let pk_b64 = pk_body["publicKey"].as_str().unwrap().to_string();

    let upload_resp = upload_pack_payload(
        &app.app,
        &admin,
        "wordbook-core",
        "1.2.3",
        "stable",
        SAMPLE_PAYLOAD,
        Some("1.0.0"),
    )
    .await;
    let (status, _h, body) = response_json(upload_resp).await;
    assert_eq!(status, StatusCode::OK, "upload failed: {body}");

    let returned_sha = body["data"]["sha256"].as_str().unwrap();
    assert_eq!(
        returned_sha,
        sha256_hex(SAMPLE_PAYLOAD),
        "sha256 必须与本地计算一致"
    );

    let sig_b64 = body["data"]["signature"].as_str().unwrap();
    assert_eq!(sig_b64.len(), 88, "Ed25519 base64 签名长度应为 88");
    verify_base64(SAMPLE_PAYLOAD, sig_b64, &pk_b64).expect("签名必须用同一公钥验证通过");

    assert_eq!(
        body["data"]["sizeBytes"].as_i64().unwrap(),
        SAMPLE_PAYLOAD.len() as i64
    );
    assert_eq!(body["data"]["channel"], "stable");
}

#[tokio::test]
async fn manifest_404_when_no_active_version() {
    let app = spawn_test_server().await;
    let resp = request(
        &app.app,
        Method::GET,
        "/api/resource-packs/unknown-pack/manifest?appVersion=1.0.0&locale=zh-Hans",
        None,
        &[],
    )
    .await;
    let (status, _h, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "RESOURCE_PACK_NOT_FOUND");
}

#[tokio::test]
async fn end_to_end_upload_activate_fetch_manifest() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    let upload = upload_pack_payload(
        &app.app,
        &admin,
        "wordbook-core",
        "1.2.3",
        "stable",
        SAMPLE_PAYLOAD,
        Some("1.0.0"),
    )
    .await;
    assert_eq!(upload.status(), StatusCode::OK);

    // 切激活
    let activate = request(
        &app.app,
        Method::PUT,
        "/api/admin/resource-packs/wordbook-core/channel/stable/active",
        Some(serde_json::json!({ "version": "1.2.3" })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _h, body) = response_json(activate).await;
    assert_eq!(status, StatusCode::OK, "activate: {body}");

    // 拉 manifest
    let manifest_resp = request(
        &app.app,
        Method::GET,
        "/api/resource-packs/wordbook-core/manifest?appVersion=1.0.0&locale=zh-Hans",
        None,
        &[("host", "api.example.com".to_string())],
    )
    .await;
    assert_eq!(manifest_resp.status(), StatusCode::OK);
    let etag = manifest_resp
        .headers()
        .get("etag")
        .map(|v| v.to_str().unwrap().to_string());
    let cache_control = manifest_resp
        .headers()
        .get("cache-control")
        .map(|v| v.to_str().unwrap().to_string());
    assert_eq!(etag.as_deref(), Some("\"1.2.3\""));
    assert_eq!(cache_control.as_deref(), Some("public, max-age=60"));

    let (_s, _h, body) = response_json(manifest_resp).await;
    assert_eq!(body["packId"], "wordbook-core");
    assert_eq!(body["version"], "1.2.3");
    assert_eq!(body["sha256"], sha256_hex(SAMPLE_PAYLOAD));
    assert_eq!(body["channel"], "stable");
    assert_eq!(body["signatureAlgorithm"], "ed25519");
    assert_eq!(body["minAppVersion"], "1.0.0");
    let download_url = body["downloadURL"].as_str().unwrap();
    assert!(
        download_url.ends_with("/packs/wordbook-core/1.2.3/payload.json"),
        "downloadURL: {download_url}"
    );

    // If-None-Match 命中 → 304
    let cached = request(
        &app.app,
        Method::GET,
        "/api/resource-packs/wordbook-core/manifest?appVersion=1.0.0&locale=zh-Hans",
        None,
        &[("if-none-match", "\"1.2.3\"".to_string())],
    )
    .await;
    assert_eq!(cached.status(), StatusCode::NOT_MODIFIED);
}

#[tokio::test]
async fn manifest_409_when_app_below_min() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    upload_pack_payload(
        &app.app,
        &admin,
        "wordbook-core",
        "2.0.0",
        "stable",
        SAMPLE_PAYLOAD,
        Some("2.0.0"),
    )
    .await;
    request(
        &app.app,
        Method::PUT,
        "/api/admin/resource-packs/wordbook-core/channel/stable/active",
        Some(serde_json::json!({ "version": "2.0.0" })),
        &[("authorization", auth_header(&admin))],
    )
    .await;

    let resp = request(
        &app.app,
        Method::GET,
        "/api/resource-packs/wordbook-core/manifest?appVersion=1.5.0&locale=zh-Hans",
        None,
        &[],
    )
    .await;
    let (status, _h, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "RESOURCE_PACK_APP_VERSION_TOO_LOW");
}

#[tokio::test]
async fn deactivate_removes_from_manifest() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    upload_pack_payload(
        &app.app,
        &admin,
        "wordbook-core",
        "1.0.0",
        "stable",
        SAMPLE_PAYLOAD,
        None,
    )
    .await;
    request(
        &app.app,
        Method::PUT,
        "/api/admin/resource-packs/wordbook-core/channel/stable/active",
        Some(serde_json::json!({ "version": "1.0.0" })),
        &[("authorization", auth_header(&admin))],
    )
    .await;

    // 激活后能拉到
    let active = request(
        &app.app,
        Method::GET,
        "/api/resource-packs/wordbook-core/manifest?appVersion=1.0.0&locale=zh-Hans",
        None,
        &[],
    )
    .await;
    assert_eq!(active.status(), StatusCode::OK);

    // 软删（admin 端点）
    let del = request(
        &app.app,
        Method::DELETE,
        "/api/admin/resource-packs/wordbook-core/versions/1.0.0",
        None,
        &[("authorization", auth_header(&admin))],
    )
    .await;
    assert_eq!(del.status(), StatusCode::OK);

    // 软删后 manifest 404
    let after = request(
        &app.app,
        Method::GET,
        "/api/resource-packs/wordbook-core/manifest?appVersion=1.0.0&locale=zh-Hans",
        None,
        &[],
    )
    .await;
    assert_eq!(after.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn telemetry_install_log_aggregates_in_admin_stats() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    let user = login_and_get_token(&app.app).await;

    upload_pack_payload(
        &app.app,
        &admin,
        "wordbook-core",
        "1.0.0",
        "stable",
        SAMPLE_PAYLOAD,
        None,
    )
    .await;

    // report_resource_pack_install 与 submit_telemetry 同口径做设备归属校验：
    // 未注册设备会被 403 DEVICE_NOT_REGISTERED 拒绝。此处 seed 一台未认领设备
    // （user_id NULL），命中归属校验的"未认领→放行"分支，模拟真实客户端登录后设备已登记。
    {
        let conn = app.state.store().connection().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO client_devices (device_id, platform, first_seen_at, last_seen_at)
             VALUES (?1, 'web', datetime('now'), datetime('now'))",
            rusqlite::params!["dev-test"],
        )
        .unwrap();
    }

    // 客户端上报：2 installed + 1 verify_failed + 各 1 个 m070 新增本地失败态
    for outcome in &[
        "installed",
        "installed",
        "verify_failed",
        "download_failed",
        "apply_failed",
    ] {
        let resp = request(
            &app.app,
            Method::POST,
            "/api/telemetry/resource-pack-install",
            Some(serde_json::json!({
                "packId": "wordbook-core",
                "version": "1.0.0",
                "outcome": outcome,
                "appVersion": "1.0.0",
            })),
            &[
                ("authorization", auth_header(&user)),
                ("x-device-id", "dev-test".to_string()),
            ],
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK, "outcome={outcome}");
    }

    // admin 查 stats，应当看到 2 installed + 1 verify_failed
    let stats_resp = request(
        &app.app,
        Method::GET,
        "/api/admin/resource-packs/wordbook-core/stats",
        None,
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (s, _h, body) = response_json(stats_resp).await;
    assert_eq!(s, StatusCode::OK, "stats failed: {body}");
    let stats = body["data"]["stats"].as_array().unwrap();
    let installed = stats
        .iter()
        .find(|s| s["outcome"] == "installed")
        .expect("installed bucket")["count"]
        .as_i64()
        .unwrap();
    let failed = stats
        .iter()
        .find(|s| s["outcome"] == "verify_failed")
        .expect("verify_failed bucket")["count"]
        .as_i64()
        .unwrap();
    assert_eq!(installed, 2);
    assert_eq!(failed, 1);
    for bucket in ["download_failed", "apply_failed"] {
        let n = stats
            .iter()
            .find(|s| s["outcome"] == bucket)
            .unwrap_or_else(|| panic!("{bucket} bucket"))["count"]
            .as_i64()
            .unwrap();
        assert_eq!(n, 1, "outcome={bucket}");
    }
}

#[tokio::test]
async fn sse_broadcast_dedup_within_5_minutes() {
    let app = spawn_test_server().await;
    // 第一次：未广播过，应返 true
    assert!(app.state.try_mark_pack_broadcast("wb-core", "stable"));
    // 第二次：在 dedup 窗口内，应返 false
    assert!(!app.state.try_mark_pack_broadcast("wb-core", "stable"));
    // 不同 channel 互不影响
    assert!(app.state.try_mark_pack_broadcast("wb-core", "beta"));
    // 不同 pack 互不影响
    assert!(app
        .state
        .try_mark_pack_broadcast("homepage-banners", "stable"));
}

#[tokio::test]
async fn admin_list_returns_uploaded_packs() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    upload_pack_payload(
        &app.app,
        &admin,
        "wordbook-core",
        "1.0.0",
        "stable",
        SAMPLE_PAYLOAD,
        None,
    )
    .await;
    upload_pack_payload(
        &app.app,
        &admin,
        "homepage-banners",
        "2.0.0",
        "beta",
        SAMPLE_PAYLOAD,
        None,
    )
    .await;

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/resource-packs",
        None,
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (s, _h, body) = response_json(resp).await;
    assert_eq!(s, StatusCode::OK);
    let arr = body["data"].as_array().unwrap();
    assert_eq!(arr.len(), 2);
    let pack_ids: Vec<&str> = arr.iter().map(|e| e["packId"].as_str().unwrap()).collect();
    assert!(pack_ids.contains(&"wordbook-core"));
    assert!(pack_ids.contains(&"homepage-banners"));
}

/// v1.1-P2.10：upload / set_active / deactivate 三个 admin 资源包 handler 都应写
/// admin audit。通过 /api/admin/updates/history 反查，验证 action / targetType /
/// targetId / metadata 都已落库。
#[tokio::test]
async fn admin_resource_pack_handlers_write_audit_log() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    // upload → set_active → deactivate
    upload_pack_payload(
        &app.app,
        &admin,
        "wordbook-core",
        "1.2.3",
        "stable",
        SAMPLE_PAYLOAD,
        None,
    )
    .await;
    let _ = request(
        &app.app,
        Method::PUT,
        "/api/admin/resource-packs/wordbook-core/channel/stable/active",
        Some(serde_json::json!({ "version": "1.2.3" })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let _ = request(
        &app.app,
        Method::DELETE,
        "/api/admin/resource-packs/wordbook-core/versions/1.2.3",
        None,
        &[("authorization", auth_header(&admin))],
    )
    .await;

    // 反查审计：resource_pack 目标审计经 list_admin_audit_for_target 落库。/updates/history 自
    // beta.3 起仅含升级/回滚（self_update/rollback），不再混入资源包等 admin 操作，故此处直查
    // store（与实际审计查询路径一致），而非已收窄的升级历史端点。
    let audits = app
        .state
        .store()
        .list_admin_audit_for_target("resource_pack", "wordbook-core", 50)
        .expect("list resource_pack audit");
    let actions: Vec<&str> = audits.iter().map(|e| e.action.as_str()).collect();
    assert!(
        actions.contains(&"resource_pack.upload"),
        "缺 upload audit：{actions:?}"
    );
    assert!(
        actions.contains(&"resource_pack.set_active"),
        "缺 set_active audit：{actions:?}"
    );
    assert!(
        actions.contains(&"resource_pack.deactivate"),
        "缺 deactivate audit：{actions:?}"
    );

    // 任意 resource_pack 条目都应 targetId=wordbook-core / outcome=success，metadata 可反解
    for e in &audits {
        assert_eq!(e.target_id.as_deref(), Some("wordbook-core"));
        assert_eq!(e.outcome, "success");
        let m = e.metadata_json.as_deref().expect("metadataJson");
        let _: serde_json::Value = serde_json::from_str(m).expect("metadata 必须合法 JSON");
    }
}

/// web-app 版本若以非 tarball 工件（默认 json）上传，激活 stable 必须被拒（400），
/// 以守住「current 切换成功才前移 webTargetVersion」不变式——否则 DB 指针前移但托管根未切。
#[tokio::test]
async fn webapp_activate_rejects_non_tarball_artifact() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    // 以默认 artifactType=json 上传 web-app 版本（落 payload.json，非 .tar.gz）
    let upload = upload_pack_payload(
        &app.app,
        &admin,
        "web-app",
        "9.9.9",
        "stable",
        SAMPLE_PAYLOAD,
        None,
    )
    .await;
    assert_eq!(upload.status(), StatusCode::OK, "上传应成功");

    // 激活 stable → 应被拒绝（非 tarball 工件无法切 current）
    let activate = request(
        &app.app,
        Method::PUT,
        "/api/admin/resource-packs/web-app/channel/stable/active",
        Some(serde_json::json!({ "version": "9.9.9" })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _h, body) = response_json(activate).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "web-app 非 tarball 激活应 400：{body}"
    );

    // 不变式：激活被拒后 webTargetVersion 不应前移到 9.9.9（DB 指针未翻转）
    let st = request(&app.app, Method::GET, "/api/status", None, &[]).await;
    let (st_status, _h2, st_body) = response_json(st).await;
    assert_eq!(st_status, StatusCode::OK);
    assert_ne!(
        st_body["data"]["webTargetVersion"], "9.9.9",
        "激活被拒后 webTargetVersion 不应前移：{st_body}"
    );
}

/// 通用 pack（非 web-app）的频道校验：beta 版本向 stable 激活必须 400 PACK_CHANNEL_MISMATCH，
/// 且 stable 原激活指针不得被覆盖（此前该误操作静默覆盖旧指针致 stable 通道 404）。
#[tokio::test]
async fn generic_pack_activate_rejects_channel_mismatch_and_keeps_pointer() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    // 先建立 stable 的正常激活指针：1.0.0@stable
    upload_pack_payload(
        &app.app,
        &admin,
        "wordbook-core",
        "1.0.0",
        "stable",
        SAMPLE_PAYLOAD,
        None,
    )
    .await;
    let act1 = request(
        &app.app,
        Method::PUT,
        "/api/admin/resource-packs/wordbook-core/channel/stable/active",
        Some(serde_json::json!({ "version": "1.0.0" })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    assert_eq!(act1.status(), StatusCode::OK);

    // 2.0.0 发布在 beta 通道
    upload_pack_payload(
        &app.app,
        &admin,
        "wordbook-core",
        "2.0.0",
        "beta",
        SAMPLE_PAYLOAD,
        None,
    )
    .await;

    // 把 beta 版本往 stable 激活 → 400 PACK_CHANNEL_MISMATCH
    let act2 = request(
        &app.app,
        Method::PUT,
        "/api/admin/resource-packs/wordbook-core/channel/stable/active",
        Some(serde_json::json!({ "version": "2.0.0" })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _h, body) = response_json(act2).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "频道不符应 400：{body}");
    assert_eq!(body["code"], "PACK_CHANNEL_MISMATCH");

    // stable 原激活指针未被覆盖：manifest 仍返回 1.0.0
    let manifest = request(
        &app.app,
        Method::GET,
        "/api/resource-packs/wordbook-core/manifest?appVersion=1.0.0&locale=zh-Hans",
        None,
        &[],
    )
    .await;
    let (ms, _h2, mbody) = response_json(manifest).await;
    assert_eq!(ms, StatusCode::OK);
    assert_eq!(
        mbody["version"], "1.0.0",
        "stable 激活指针不得被频道不符的激活请求覆盖：{mbody}"
    );
}

/// 并发上传同一 (pack, version)：UPLOAD_LOCK 串行化下恰好一个 200、一个 409
/// （谁的字节落盘 = 谁的 sha256 进 DB，绝无交错）。
#[tokio::test]
async fn concurrent_upload_same_version_yields_one_ok_one_conflict() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    let (r1, r2) = tokio::join!(
        upload_pack_payload(
            &app.app,
            &admin,
            "wordbook-core",
            "3.3.3",
            "stable",
            SAMPLE_PAYLOAD,
            None,
        ),
        upload_pack_payload(
            &app.app,
            &admin,
            "wordbook-core",
            "3.3.3",
            "stable",
            SAMPLE_PAYLOAD,
            None,
        )
    );
    let statuses = [r1.status(), r2.status()];
    let ok_count = statuses.iter().filter(|s| **s == StatusCode::OK).count();
    let conflict_count = statuses
        .iter()
        .filter(|s| **s == StatusCode::CONFLICT)
        .count();
    assert_eq!(
        (ok_count, conflict_count),
        (1, 1),
        "并发上传同 (pack,version) 应恰好一 200 一 409，实际：{statuses:?}"
    );
}

/// web-app 版本若发布在 beta 通道，却在 stable 通道激活，必须被拒（400）——
/// 守住「stable 托管根只服务 stable 工件」不变式（resource_pack_active FK 不约束 channel）。
#[tokio::test]
async fn webapp_activate_rejects_channel_mismatch() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    // 发布到 beta 通道
    let upload = upload_pack_payload(
        &app.app,
        &admin,
        "web-app",
        "8.8.8",
        "beta",
        SAMPLE_PAYLOAD,
        None,
    )
    .await;
    assert_eq!(upload.status(), StatusCode::OK, "上传应成功");

    // 在 stable 通道激活该 beta 版本 → 应被拒（通道不符，先于 tarball 校验命中）
    let activate = request(
        &app.app,
        Method::PUT,
        "/api/admin/resource-packs/web-app/channel/stable/active",
        Some(serde_json::json!({ "version": "8.8.8" })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _h, body) = response_json(activate).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "通道不符激活应 400：{body}"
    );
}

/// 软删除（下架）的 web-app 版本不能再激活：否则 current 会指向它而 status 读出 null（脱钩）。
#[tokio::test]
async fn webapp_activate_rejects_deactivated_version() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    let up = upload_pack_payload(
        &app.app,
        &admin,
        "web-app",
        "7.7.7",
        "stable",
        SAMPLE_PAYLOAD,
        None,
    )
    .await;
    assert_eq!(up.status(), StatusCode::OK, "上传应成功");

    // 软删除该版本
    let del = request(
        &app.app,
        Method::DELETE,
        "/api/admin/resource-packs/web-app/versions/7.7.7",
        None,
        &[("authorization", auth_header(&admin))],
    )
    .await;
    assert_eq!(del.status(), StatusCode::OK, "软删除应成功");

    // 激活已下架版本 → 应被拒（400，先于 channel/tarball 校验命中）
    let activate = request(
        &app.app,
        Method::PUT,
        "/api/admin/resource-packs/web-app/channel/stable/active",
        Some(serde_json::json!({ "version": "7.7.7" })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _h, body) = response_json(activate).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "下架版本激活应 400：{body}"
    );
}

/// web-app 当前激活版本不能直接软删：status.webTargetVersion 会变 null，但 current 符号链接
/// 仍在物理服务这份"已下线"构建，两者失配。须先切换到其它版本再删，与激活路径的保护对称。
/// 非 web-app pack 删除当前激活版本的既有放行语义（软删=manifest 摘除，文件保留）不受影响。
#[tokio::test]
async fn webapp_deactivate_rejects_active_version() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    let up = upload_pack_payload(
        &app.app,
        &admin,
        "web-app",
        "9.9.9",
        "stable",
        SAMPLE_PAYLOAD,
        None,
    )
    .await;
    assert_eq!(up.status(), StatusCode::OK, "上传应成功");

    // web-app 的 HTTP 激活端点额外校验工件必须是真 tarball（见 webapp_activate_rejects_
    // non_tarball_artifact），与本测试要验证的"删除激活版本"guard 无关；直接写 store 的激活
    // 指针，绕开这层无关校验，只覆盖本测试关心的 deactivate 路径。
    app.state
        .store()
        .set_active_pack_version("web-app", ResourcePackChannel::Stable, "9.9.9", None)
        .expect("直接置激活指针应成功");

    // 删除当前激活版本 → 应被拒（409，要求先切换）
    let del = request(
        &app.app,
        Method::DELETE,
        "/api/admin/resource-packs/web-app/versions/9.9.9",
        None,
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _h, body) = response_json(del).await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "删除当前激活版本应 409：{body}"
    );

    // 非 web-app pack：删除其当前激活版本仍应放行（既有语义不变）
    let up2 = upload_pack_payload(
        &app.app,
        &admin,
        "wordbook-core",
        "9.9.9",
        "stable",
        SAMPLE_PAYLOAD,
        None,
    )
    .await;
    assert_eq!(up2.status(), StatusCode::OK, "上传应成功");
    let activate2 = request(
        &app.app,
        Method::PUT,
        "/api/admin/resource-packs/wordbook-core/channel/stable/active",
        Some(serde_json::json!({ "version": "9.9.9" })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    assert_eq!(activate2.status(), StatusCode::OK, "激活应成功");
    let del2 = request(
        &app.app,
        Method::DELETE,
        "/api/admin/resource-packs/wordbook-core/versions/9.9.9",
        None,
        &[("authorization", auth_header(&admin))],
    )
    .await;
    assert_eq!(
        del2.status(),
        StatusCode::OK,
        "非 web-app 删除当前激活版本应保持既有放行语义"
    );
}
