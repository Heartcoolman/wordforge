mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token, login_and_get_tokens, setup_admin_and_get_token};
use common::http::{request, response_json};
use tower::util::ServiceExt;

// ============================================================
// words.rs
// ============================================================

async fn create_word(app: &axum::Router, admin: &str, text: &str, meaning: &str) -> String {
    let resp = request(
        app,
        Method::POST,
        "/api/words",
        Some(serde_json::json!({ "text": text, "meaning": meaning, "difficulty": 0.4 })),
        &[("authorization", auth_header(admin))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CREATED, "create_word body: {body}");
    body["data"]["id"].as_str().expect("word id").to_string()
}

#[tokio::test]
async fn words_create_requires_admin_and_validates() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    let user = login_and_get_token(&app.app).await;

    // 成功创建 (admin)
    let id = create_word(&app.app, &admin, "alpha", "释义A").await;
    assert!(!id.is_empty());

    // 401：缺 token
    let resp = request(
        &app.app,
        Method::POST,
        "/api/words",
        Some(serde_json::json!({ "text": "x", "meaning": "y" })),
        &[],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // 401：普通用户 token 不是 admin（admin secret 校验失败）
    let resp = request(
        &app.app,
        Method::POST,
        "/api/words",
        Some(serde_json::json!({ "text": "x", "meaning": "y" })),
        &[("authorization", auth_header(&user))],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // 400：空 text/meaning
    let resp = request(
        &app.app,
        Method::POST,
        "/api/words",
        Some(serde_json::json!({ "text": "   ", "meaning": "" })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "WORDS_INVALID_PAYLOAD");

    // 400：无效请求体（缺必填字段）
    let resp = request(
        &app.app,
        Method::POST,
        "/api/words",
        Some(serde_json::json!({ "meaning": "只给释义" })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn words_list_count_and_search() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    let user = login_and_get_token(&app.app).await;

    create_word(&app.app, &admin, "apple", "苹果").await;
    create_word(&app.app, &admin, "banana", "香蕉").await;

    // list 成功 + 分页参数
    let resp = request(
        &app.app,
        Method::GET,
        "/api/words?page=1&perPage=1",
        None,
        &[("authorization", auth_header(&user))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["data"].as_array().unwrap().len(), 1);
    assert!(body["data"]["total"].as_u64().unwrap() >= 2);

    // list 带 search
    let resp = request(
        &app.app,
        Method::GET,
        "/api/words?search=apple",
        None,
        &[("authorization", auth_header(&user))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["total"].as_u64().unwrap() >= 1);

    // list 带空白 search（走非 search 分支）
    let resp = request(
        &app.app,
        Method::GET,
        "/api/words?search=%20",
        None,
        &[("authorization", auth_header(&user))],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // count 成功
    let resp = request(
        &app.app,
        Method::GET,
        "/api/words/count",
        None,
        &[("authorization", auth_header(&user))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["total"].as_u64().unwrap() >= 2);

    // list 401：缺 token
    let resp = request(&app.app, Method::GET, "/api/words", None, &[]).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn words_get_update_delete_lifecycle() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    let user = login_and_get_token(&app.app).await;

    let id = create_word(&app.app, &admin, "gamma", "释义G").await;

    // get 成功
    let resp = request(
        &app.app,
        Method::GET,
        &format!("/api/words/{id}"),
        None,
        &[("authorization", auth_header(&user))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["text"], "gamma");

    // get 404
    let resp = request(
        &app.app,
        Method::GET,
        "/api/words/does-not-exist",
        None,
        &[("authorization", auth_header(&user))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "NOT_FOUND");

    // update 成功（仅改 meaning；空 text 回退保留原值）
    let resp = request(
        &app.app,
        Method::PUT,
        &format!("/api/words/{id}"),
        Some(serde_json::json!({ "text": "  ", "meaning": "新释义", "difficulty": 0.9 })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["text"], "gamma");
    assert_eq!(body["data"]["meaning"], "新释义");

    // update 404
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/words/missing-id",
        Some(serde_json::json!({ "text": "z", "meaning": "z" })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // delete 403：普通用户不是 admin
    let resp = request(
        &app.app,
        Method::DELETE,
        &format!("/api/words/{id}"),
        None,
        &[("authorization", auth_header(&user))],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // delete 成功
    let resp = request(
        &app.app,
        Method::DELETE,
        &format!("/api/words/{id}"),
        None,
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["deleted"], true);

    // delete 404：已删除
    let resp = request(
        &app.app,
        Method::DELETE,
        &format!("/api/words/{id}"),
        None,
        &[("authorization", auth_header(&admin))],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn words_batch_get_and_batch_create() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;
    let user = login_and_get_token(&app.app).await;

    let id1 = create_word(&app.app, &admin, "delta", "释义D").await;

    // batch-get 成功（含一个不存在 id，被过滤）
    let resp = request(
        &app.app,
        Method::POST,
        "/api/words/batch-get",
        Some(serde_json::json!({ "ids": [id1, "nope"] })),
        &[("authorization", auth_header(&user))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"].as_array().unwrap().len(), 1);

    // batch-get 400：超过上限
    let too_many: Vec<String> = (0..(app.config.limits.max_batch_size + 1))
        .map(|i| i.to_string())
        .collect();
    let resp = request(
        &app.app,
        Method::POST,
        "/api/words/batch-get",
        Some(serde_json::json!({ "ids": too_many })),
        &[("authorization", auth_header(&user))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "BATCH_TOO_LARGE");

    // batch-create 成功（含一个空条目被 skip）
    let resp = request(
        &app.app,
        Method::POST,
        "/api/words/batch",
        Some(serde_json::json!({
            "words": [
                { "text": "epsilon", "meaning": "释义E" },
                { "text": "  ", "meaning": "释义空" }
            ]
        })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["data"]["count"], 1);
    assert_eq!(body["data"]["skipped"].as_array().unwrap(), &vec![serde_json::json!(1)]);

    // batch-create 400：超过上限
    let too_many_words: Vec<serde_json::Value> = (0..(app.config.limits.max_batch_size + 1))
        .map(|i| serde_json::json!({ "text": format!("w{i}"), "meaning": "m" }))
        .collect();
    let resp = request(
        &app.app,
        Method::POST,
        "/api/words/batch",
        Some(serde_json::json!({ "words": too_many_words })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "BATCH_TOO_LARGE");

    // batch-create 401：普通用户
    let resp = request(
        &app.app,
        Method::POST,
        "/api/words/batch",
        Some(serde_json::json!({ "words": [] })),
        &[("authorization", auth_header(&user))],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn words_import_url_ssrf_guards() {
    let app = spawn_test_server().await;
    let admin = setup_admin_and_get_token(&app.app).await;

    // 400：非法 URL（非 http/https）
    let resp = request(
        &app.app,
        Method::POST,
        "/api/words/import-url",
        Some(serde_json::json!({ "url": "ftp://example.com/words.txt" })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "IMPORT_INVALID_URL");

    // 400：内网 IP 被拦截
    let resp = request(
        &app.app,
        Method::POST,
        "/api/words/import-url",
        Some(serde_json::json!({ "url": "http://127.0.0.1/words.txt" })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "IMPORT_BLOCKED_URL");

    // 400：localhost 被拦截
    let resp = request(
        &app.app,
        Method::POST,
        "/api/words/import-url",
        Some(serde_json::json!({ "url": "http://localhost/words.txt" })),
        &[("authorization", auth_header(&admin))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "IMPORT_BLOCKED_URL");

    // 401：缺 token
    let resp = request(
        &app.app,
        Method::POST,
        "/api/words/import-url",
        Some(serde_json::json!({ "url": "https://example.com/words.txt" })),
        &[],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ============================================================
// auth.rs
// ============================================================

#[tokio::test]
async fn auth_register_validation_and_duplicate() {
    let app = spawn_test_server().await;
    let email = format!("reg-{}@test.com", uuid::Uuid::new_v4());
    let username = format!("reg-{}", uuid::Uuid::new_v4().simple());

    // 成功注册
    let resp = request(
        &app.app,
        Method::POST,
        "/api/auth/register",
        Some(serde_json::json!({ "email": email, "username": username, "password": "Passw0rd!" })),
        &[],
    )
    .await;
    let (status, headers, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(body["data"]["accessToken"].as_str().is_some());
    assert!(body["data"]["refreshToken"].as_str().is_some());
    // 下发 Set-Cookie
    assert!(headers.get_all("set-cookie").iter().count() >= 2);

    // 409：邮箱重复
    let resp = request(
        &app.app,
        Method::POST,
        "/api/auth/register",
        Some(serde_json::json!({ "email": email, "username": "other-name", "password": "Passw0rd!" })),
        &[],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "AUTH_EMAIL_EXISTS");

    // 400：邮箱格式无效
    let resp = request(
        &app.app,
        Method::POST,
        "/api/auth/register",
        Some(serde_json::json!({ "email": "not-an-email", "username": "valid-name", "password": "Passw0rd!" })),
        &[],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "AUTH_INVALID_EMAIL");

    // 400：用户名无效（太短）
    let resp = request(
        &app.app,
        Method::POST,
        "/api/auth/register",
        Some(serde_json::json!({ "email": format!("u-{}@test.com", uuid::Uuid::new_v4()), "username": "a", "password": "Passw0rd!" })),
        &[],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "AUTH_INVALID_USERNAME");

    // 400：弱密码（无大写/数字）
    let resp = request(
        &app.app,
        Method::POST,
        "/api/auth/register",
        Some(serde_json::json!({ "email": format!("u-{}@test.com", uuid::Uuid::new_v4()), "username": "valid-name", "password": "weakpass" })),
        &[],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "AUTH_WEAK_PASSWORD");

    // 400：缺字段
    let resp = request(
        &app.app,
        Method::POST,
        "/api/auth/register",
        Some(serde_json::json!({ "email": "x@y.com" })),
        &[],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn auth_login_success_and_failures() {
    let app = spawn_test_server().await;
    let email = format!("login-{}@test.com", uuid::Uuid::new_v4());
    let username = format!("login-{}", uuid::Uuid::new_v4().simple());
    let password = "Passw0rd!";

    // 先注册
    let resp = request(
        &app.app,
        Method::POST,
        "/api/auth/register",
        Some(serde_json::json!({ "email": email, "username": username, "password": password })),
        &[],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    // 登录成功
    let resp = request(
        &app.app,
        Method::POST,
        "/api/auth/login",
        Some(serde_json::json!({ "email": email, "password": password })),
        &[],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["accessToken"].as_str().is_some());
    assert_eq!(body["data"]["user"]["email"], email.to_lowercase());

    // 401：密码错误
    let resp = request(
        &app.app,
        Method::POST,
        "/api/auth/login",
        Some(serde_json::json!({ "email": email, "password": "WrongPass1!" })),
        &[],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "AUTH_UNAUTHORIZED");

    // 401：用户不存在
    let resp = request(
        &app.app,
        Method::POST,
        "/api/auth/login",
        Some(serde_json::json!({ "email": "ghost@test.com", "password": "Passw0rd!" })),
        &[],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // 400：缺字段
    let resp = request(
        &app.app,
        Method::POST,
        "/api/auth/login",
        Some(serde_json::json!({ "email": email })),
        &[],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn auth_refresh_logout_flow() {
    let app = spawn_test_server().await;
    let (access, refresh) = login_and_get_tokens(&app.app).await;

    // refresh 成功（用 refresh token via Authorization header）
    let resp = request(
        &app.app,
        Method::POST,
        "/api/auth/refresh",
        None,
        &[("authorization", auth_header(&refresh))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "refresh body: {body}");
    assert!(body["data"]["accessToken"].as_str().is_some());

    // refresh 401：复用已消费的 refresh token（已被原子删除）
    let resp = request(
        &app.app,
        Method::POST,
        "/api/auth/refresh",
        None,
        &[("authorization", auth_header(&refresh))],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // refresh 401：缺 token
    let resp = request(&app.app, Method::POST, "/api/auth/refresh", None, &[]).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // refresh 401：用 access token 当 refresh（token_type 不符 / 签名密钥不符）
    let resp = request(
        &app.app,
        Method::POST,
        "/api/auth/refresh",
        None,
        &[("authorization", auth_header(&access))],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // logout 成功
    let resp = request(
        &app.app,
        Method::POST,
        "/api/auth/logout",
        None,
        &[("authorization", auth_header(&access))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["loggedOut"], true);

    // logout 401：缺 token
    let resp = request(&app.app, Method::POST, "/api/auth/logout", None, &[]).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_password_reset_endpoints() {
    let app = spawn_test_server().await;

    // forgot-password：未知邮箱也返回 200（防枚举）
    let resp = request(
        &app.app,
        Method::POST,
        "/api/auth/forgot-password",
        Some(serde_json::json!({ "email": "unknown@test.com" })),
        &[],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["emailSent"], true);

    // reset-password 400：弱密码（先于 token 校验）
    let resp = request(
        &app.app,
        Method::POST,
        "/api/auth/reset-password",
        Some(serde_json::json!({ "token": "whatever", "newPassword": "weak" })),
        &[],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "AUTH_WEAK_PASSWORD");

    // reset-password 400：合法密码但 token 无效
    let resp = request(
        &app.app,
        Method::POST,
        "/api/auth/reset-password",
        Some(serde_json::json!({ "token": "nonexistent-token", "newPassword": "Passw0rd!" })),
        &[],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "AUTH_INVALID_RESET_TOKEN");

    // verify-reset-token：不存在的 token 返回 200 valid:false
    let resp = request(
        &app.app,
        Method::POST,
        "/api/auth/verify-reset-token",
        Some(serde_json::json!({ "token": "nope" })),
        &[],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["valid"], false);
}

// ============================================================
// user_profile.rs
// ============================================================

#[tokio::test]
async fn profile_reward_preference() {
    let app = spawn_test_server().await;
    let user = login_and_get_token(&app.app).await;

    // get 默认值 standard
    let resp = request(
        &app.app,
        Method::GET,
        "/api/user-profile/reward",
        None,
        &[("authorization", auth_header(&user))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["rewardType"], "standard");

    // put 成功
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/user-profile/reward",
        Some(serde_json::json!({ "rewardType": "explorer" })),
        &[("authorization", auth_header(&user))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["rewardType"], "explorer");

    // get 反映已写入
    let resp = request(
        &app.app,
        Method::GET,
        "/api/user-profile/reward",
        None,
        &[("authorization", auth_header(&user))],
    )
    .await;
    let (_, _, body) = response_json(resp).await;
    assert_eq!(body["data"]["rewardType"], "explorer");

    // put 400：非法类型
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/user-profile/reward",
        Some(serde_json::json!({ "rewardType": "wizard" })),
        &[("authorization", auth_header(&user))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_REWARD_TYPE");

    // 401：缺 token
    let resp = request(&app.app, Method::GET, "/api/user-profile/reward", None, &[]).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn profile_cognitive_style_chronotype() {
    let app = spawn_test_server().await;
    let user = login_and_get_token(&app.app).await;

    for path in ["/api/user-profile/cognitive", "/api/user-profile/learning-style", "/api/user-profile/chronotype"] {
        let resp = request(
            &app.app,
            Method::GET,
            path,
            None,
            &[("authorization", auth_header(&user))],
        )
        .await;
        let (status, _, body) = response_json(resp).await;
        assert_eq!(status, StatusCode::OK, "path {path} body: {body}");
        assert_eq!(body["success"], true);

        // 401：缺 token
        let resp = request(&app.app, Method::GET, path, None, &[]).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "path {path} should require auth");
    }
}

#[tokio::test]
async fn profile_habit_get_set_and_validation() {
    let app = spawn_test_server().await;
    let user = login_and_get_token(&app.app).await;

    // get 默认值（无持久化时走 amas 回退）
    let resp = request(
        &app.app,
        Method::GET,
        "/api/user-profile/habit",
        None,
        &[("authorization", auth_header(&user))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["preferredHours"].is_array());

    // post 成功
    let resp = request(
        &app.app,
        Method::POST,
        "/api/user-profile/habit",
        Some(serde_json::json!({ "preferredHours": [8, 21], "sessionsPerDay": 3.0, "medianSessionLengthMins": 20.0 })),
        &[("authorization", auth_header(&user))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["sessionsPerDay"], 3.0);

    // post 400：preferredHours 越界
    let resp = request(
        &app.app,
        Method::POST,
        "/api/user-profile/habit",
        Some(serde_json::json!({ "preferredHours": [25] })),
        &[("authorization", auth_header(&user))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_PREFERRED_HOURS");

    // post 400：sessionsPerDay 越界
    let resp = request(
        &app.app,
        Method::POST,
        "/api/user-profile/habit",
        Some(serde_json::json!({ "sessionsPerDay": 99.0 })),
        &[("authorization", auth_header(&user))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_SESSIONS_PER_DAY");

    // post 400：medianSessionLengthMins 越界
    let resp = request(
        &app.app,
        Method::POST,
        "/api/user-profile/habit",
        Some(serde_json::json!({ "medianSessionLengthMins": 999.0 })),
        &[("authorization", auth_header(&user))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_SESSION_LENGTH");

    // 401：缺 token
    let resp = request(&app.app, Method::GET, "/api/user-profile/habit", None, &[]).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn profile_avatar_upload() {
    let app = spawn_test_server().await;
    let user = login_and_get_token(&app.app).await;

    // 400：空 body
    let resp = request(
        &app.app,
        Method::POST,
        "/api/user-profile/avatar",
        None,
        &[("authorization", auth_header(&user))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "AVATAR_EMPTY");

    // 400：非图片格式（用裸文本绕过 JSON content-type）
    let resp = app
        .app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::POST)
                .uri("/api/user-profile/avatar")
                .header("authorization", auth_header(&user))
                .header("content-type", "application/octet-stream")
                .body(axum::body::Body::from(vec![0u8, 1, 2, 3, 4, 5]))
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "AVATAR_INVALID_TYPE");

    // 200：合法 PNG 魔术字节
    let png: Vec<u8> = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0];
    let resp = app
        .app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(Method::POST)
                .uri("/api/user-profile/avatar")
                .header("authorization", auth_header(&user))
                .header("content-type", "application/octet-stream")
                .body(axum::body::Body::from(png))
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "avatar upload body: {body}");
    assert!(body["data"]["avatarUrl"].as_str().unwrap().ends_with(".png"));

    // 401：缺 token
    let resp = request(&app.app, Method::POST, "/api/user-profile/avatar", None, &[]).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
