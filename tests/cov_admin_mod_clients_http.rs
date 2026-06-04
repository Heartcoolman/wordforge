mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token, setup_admin_and_get_token};
use common::http::{request, response_json};

/// 创建一个普通用户并返回 (token, user_id, email)
async fn create_user(app: &axum::Router) -> (String, String, String) {
    let token = login_and_get_token(app).await;
    let resp = request(
        app,
        Method::GET,
        "/api/users/me",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let id = body["data"]["id"].as_str().unwrap().to_string();
    let email = body["data"]["email"].as_str().unwrap().to_string();
    (token, id, email)
}

// ─────────────────────────────────────────────────────────
// mod.rs：admin 路由聚合层 — 鉴权门控（无 token / 普通用户 token → 401）
// ─────────────────────────────────────────────────────────
#[tokio::test]
async fn it_admin_routes_require_token() {
    let app = spawn_test_server().await;

    // 这些 admin 业务端点在无 token 时必须 401
    let protected_get = [
        "/api/admin/users",
        "/api/admin/stats",
        "/api/admin/analytics/engagement",
        "/api/admin/analytics/learning",
        "/api/admin/monitoring/health",
        "/api/admin/monitoring/database",
        "/api/admin/settings",
        "/api/admin/clients",
        "/api/admin/telemetry/some-device",
    ];
    for path in protected_get {
        let resp = request(&app.app, Method::GET, path, None, &[]).await;
        let (status, _, _) = response_json(resp).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "no-token GET {path}");
    }

    // 普通用户 token 也不得访问 admin（鉴权门控）
    let user_token = login_and_get_token(&app.app).await;
    for path in protected_get {
        let resp = request(
            &app.app,
            Method::GET,
            path,
            None,
            &[("authorization", auth_header(&user_token))],
        )
        .await;
        let (status, _, _) = response_json(resp).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "user-token GET {path}");
    }

    // POST 类 admin 端点同样需鉴权
    let protected_post = [
        "/api/admin/users/abc/ban",
        "/api/admin/users/abc/unban",
        "/api/admin/users/abc/reset-password",
        "/api/admin/clients/dev-x/ban",
        "/api/admin/clients/dev-x/unban",
        "/api/admin/clients/dev-x/request-telemetry",
    ];
    for path in protected_post {
        let resp = request(&app.app, Method::POST, path, None, &[]).await;
        let (status, _, _) = response_json(resp).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "no-token POST {path}");
    }
}

// ─────────────────────────────────────────────────────────
// mod.rs：list_users 分页 / search / banned 过滤
// ─────────────────────────────────────────────────────────
#[tokio::test]
async fn it_admin_list_users_pagination_and_filters() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    // 预置三个普通用户
    let (_t1, id1, email1) = create_user(&app.app).await;
    let (_t2, _id2, _email2) = create_user(&app.app).await;
    let (_t3, _id3, _email3) = create_user(&app.app).await;

    // 默认列表
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/users",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["data"].is_array());
    assert!(body["data"]["total"].as_u64().unwrap() >= 3);

    // 显式分页（命中无过滤分支 count_users）
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/users?page=1&perPage=2",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["data"].as_array().unwrap().len(), 2);

    // search 过滤（命中 has_filter 分支）— 用 email 前缀片段
    let needle = &email1[0..6];
    let resp = request(
        &app.app,
        Method::GET,
        &format!("/api/admin/users?search={needle}"),
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    // search 为 user- 前缀，应至少匹配到上面创建的用户
    assert!(body["data"]["total"].as_u64().unwrap() >= 1);

    // banned=true 过滤（当前无封禁用户 → total 为 0）
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/users?banned=true",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["total"].as_u64().unwrap(), 0);

    // banned=false 过滤（命中 has_filter，且 banned_match 为 true 分支）
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/users?banned=false",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["total"].as_u64().unwrap() >= 3);

    // 空 search（trim 后为空被过滤掉 → 落回无过滤分支）
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/users?search=%20%20",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);

    let _ = id1;
}

// ─────────────────────────────────────────────────────────
// mod.rs：ban/unban 用户成功 + 404（用户不存在）
// ─────────────────────────────────────────────────────────
#[tokio::test]
async fn it_admin_ban_unban_user_success_and_not_found() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;
    let (_user_token, user_id, _email) = create_user(&app.app).await;

    // 封禁成功
    let resp = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/users/{user_id}/ban"),
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["banned"], true);
    assert_eq!(body["data"]["userId"], user_id);

    // 解封成功
    let resp = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/users/{user_id}/unban"),
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["banned"], false);

    // 封禁不存在用户 → 404
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users/nonexistent-user/ban",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["success"], false);

    // 解封不存在用户 → 404
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users/nonexistent-user/unban",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ─────────────────────────────────────────────────────────
// mod.rs：reset-password / set-password 成功与错误分支
// ─────────────────────────────────────────────────────────
#[tokio::test]
async fn it_admin_reset_and_set_password() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;
    let (_user_token, user_id, email) = create_user(&app.app).await;

    // 生成密码重置密钥成功
    let resp = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/users/{user_id}/reset-password"),
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["resetCreated"], true);
    assert!(body["data"]["resetKey"].is_string());
    assert_eq!(body["data"]["expiresInHours"], 4);

    // reset-password 不存在用户 → 404
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users/nope/reset-password",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // set-password 弱密码 → 400 AUTH_WEAK_PASSWORD
    let resp = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/users/{user_id}/set-password"),
        Some(serde_json::json!({ "newPassword": "123" })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "AUTH_WEAK_PASSWORD");

    // set-password 用户不存在 → 404（密码合法但目标不存在）
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users/nope/set-password",
        Some(serde_json::json!({ "newPassword": "NewPassw0rd!" })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // set-password 成功 → 旧密码失效、新密码可登录
    let resp = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/users/{user_id}/set-password"),
        Some(serde_json::json!({ "newPassword": "NewPassw0rd!" })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["passwordReset"], true);

    // 新密码可登录
    let resp = request(
        &app.app,
        Method::POST,
        "/api/auth/login",
        Some(serde_json::json!({ "email": email, "password": "NewPassw0rd!" })),
        &[],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);

    // 缺字段（newPassword 缺失）→ 反序列化失败 400 INVALID_REQUEST_BODY
    let resp = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/users/{user_id}/set-password"),
        Some(serde_json::json!({ "wrong": "field" })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_REQUEST_BODY");
}

// ─────────────────────────────────────────────────────────
// mod.rs：admin_stats（含 trend 字段结构）
// ─────────────────────────────────────────────────────────
#[tokio::test]
async fn it_admin_stats_shape() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;
    // 预置一些数据让计数非零
    let _ = create_user(&app.app).await;

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/stats",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["users"].is_number());
    assert!(body["data"]["words"].is_number());
    assert!(body["data"]["records"].is_number());
    assert!(body["data"]["trend"]["users"]["value"].is_number());
    assert_eq!(body["data"]["trend"]["users"]["label"], "较昨日");
    assert_eq!(body["data"]["trend"]["records"]["label"], "较昨日");
}

// ─────────────────────────────────────────────────────────
// clients.rs：list_clients（空状态 + 含设备的状态）
// ─────────────────────────────────────────────────────────
#[tokio::test]
async fn it_admin_clients_list() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    // 空状态
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/clients",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["sseLive"].is_array());
    assert!(body["data"]["recentlyActive"].is_array());
    assert_eq!(body["data"]["recentlyActive"].as_array().unwrap().len(), 0);

    // 预置近期活跃设备（直接写 store）
    app.state
        .store()
        .upsert_client_device("dev-recent", "ios", "user-1")
        .expect("seed device");

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/clients",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let recent = body["data"]["recentlyActive"].as_array().unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0]["deviceId"], "dev-recent");
    assert_eq!(recent[0]["platform"], "ios");
    assert_eq!(recent[0]["isBanned"], false);
    assert!(recent[0]["dataChannels"].is_object());
}

// ─────────────────────────────────────────────────────────
// clients.rs：ban/unban 设备成功 + 404（设备不存在）
// ─────────────────────────────────────────────────────────
#[tokio::test]
async fn it_admin_clients_ban_unban() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    app.state
        .store()
        .upsert_client_device("dev-ban", "android", "user-7")
        .expect("seed device");

    // 封禁设备成功（带 reason body，命中 reason 截断 / filter 分支）
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/clients/dev-ban/ban",
        Some(serde_json::json!({ "reason": "abuse" })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["banned"], true);
    assert_eq!(body["data"]["deviceId"], "dev-ban");
    assert!(app.state.store().is_device_banned("dev-ban").unwrap());

    // 列表中该设备 isBanned=true（命中 recently_active 含 banned 分支）
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/clients",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (_status, _, body) = response_json(resp).await;
    let recent = body["data"]["recentlyActive"].as_array().unwrap();
    assert!(recent
        .iter()
        .any(|d| d["deviceId"] == "dev-ban" && d["isBanned"] == true));

    // 解封设备成功
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/clients/dev-ban/unban",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["banned"], false);
    assert!(!app.state.store().is_device_banned("dev-ban").unwrap());

    // 封禁不存在设备 → 404
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/clients/no-such-device/ban",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["success"], false);

    // 解封不存在设备 → 404
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/clients/no-such-device/unban",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 封禁时不带 body（命中 body=None 分支）+ 设备存在
    app.state
        .store()
        .upsert_client_device("dev-ban2", "web", "user-8")
        .expect("seed device 2");
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/clients/dev-ban2/ban",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
}

// ─────────────────────────────────────────────────────────
// clients.rs：request-telemetry 离线设备 → 422 DEVICE_OFFLINE
// ─────────────────────────────────────────────────────────
#[tokio::test]
async fn it_admin_clients_request_telemetry_offline() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    // 无活跃 SSE 连接的设备 → 422
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/clients/offline-device/request-telemetry",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "DEVICE_OFFLINE");
}

// ─────────────────────────────────────────────────────────
// clients.rs：get_telemetry 成功（含分页参数）+ 404（设备不存在）
// ─────────────────────────────────────────────────────────
#[tokio::test]
async fn it_admin_get_telemetry() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    // 不存在设备 → 404
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/telemetry/missing-device",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["success"], false);

    // 预置设备 → 成功（无遥测记录，total=0）
    app.state
        .store()
        .upsert_client_device("dev-tel", "ios", "user-9")
        .expect("seed device");

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/telemetry/dev-tel?limit=10&offset=0",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["records"].is_array());
    assert_eq!(body["data"]["total"].as_u64().unwrap(), 0);

    // limit 超过上限 200（命中 .min(200) 分支）+ 默认 offset
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/telemetry/dev-tel?limit=9999",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
}
