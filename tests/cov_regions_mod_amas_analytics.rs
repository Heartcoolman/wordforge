mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token, setup_admin_and_get_token};
use common::http::{request, response_json};

/// admin token header helper.
fn h(token: &str) -> [(&'static str, String); 1] {
    [("authorization", auth_header(token))]
}

// ════════════════════════════════════════════════════════════════════════
// 1) 鉴权门控（admin extractor → 401）
//    无 token / 用户 token 访问 admin 端点都映射为 401（见 src/auth.rs:241）
// ════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn admin_endpoints_require_auth_no_token() {
    let app = spawn_test_server().await;
    // 覆盖 mod.rs / analytics.rs / amas.rs 多 nest 子路由的鉴权臂
    for path in [
        "/api/admin/users",
        "/api/admin/users/facets",
        "/api/admin/stats",
        "/api/admin/analytics/engagement",
        "/api/admin/analytics/learning",
        "/api/admin/analytics/daily-active-users",
        "/api/admin/analytics/study-overview",
        "/api/admin/analytics/kpi-summary",
        "/api/admin/amas/config",
        "/api/admin/amas/metrics",
        "/api/admin/amas/suggestions",
    ] {
        let resp = request(&app.app, Method::GET, path, None, &[]).await;
        let (status, _, _) = response_json(resp).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "no-token path: {path}");
    }
}

#[tokio::test]
async fn admin_endpoints_reject_user_token() {
    let app = spawn_test_server().await;
    let user_token = login_and_get_token(&app.app).await;
    // 用户 token（token_type=user）打 admin 端点 → 401（令牌类型无效）
    for path in [
        "/api/admin/users",
        "/api/admin/analytics/funnel",
        "/api/admin/amas/config",
        "/api/admin/amas/advisor/whitelist",
    ] {
        let resp = request(&app.app, Method::GET, path, None, &h(&user_token)).await;
        let (status, _, _) = response_json(resp).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "user-token path: {path}");
    }
}

// ════════════════════════════════════════════════════════════════════════
// 2) mod.rs：用户管理 404 / 400 / 409 错误臂
// ════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn ban_unknown_user_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users/no-such-user/ban",
        None,
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn unban_unknown_user_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users/no-such-user/unban",
        None,
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn user_profile_unknown_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/users/no-such-user/profile",
        None,
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn user_extras_unknown_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/users/no-such-user/extras",
        None,
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn reset_password_unknown_user_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users/no-such-user/reset-password",
        None,
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn set_password_unknown_user_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users/no-such-user/set-password",
        Some(serde_json::json!({ "newPassword": "Valid123!" })),
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn set_password_weak_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // validate_password 在用户存在性检查之前，故弱密码命中 400 不依赖用户存在
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users/whatever/set-password",
        Some(serde_json::json!({ "newPassword": "weak" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "AUTH_WEAK_PASSWORD");
}

#[tokio::test]
async fn create_user_invalid_email_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users",
        Some(serde_json::json!({
            "email": "not-an-email",
            "username": "validname",
            "password": "Valid123!"
        })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "AUTH_INVALID_EMAIL");
}

#[tokio::test]
async fn create_user_weak_password_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users",
        Some(serde_json::json!({
            "email": "good@test.com",
            "username": "validname",
            "password": "weak"
        })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "AUTH_WEAK_PASSWORD");
}

#[tokio::test]
async fn create_user_invalid_role_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users",
        Some(serde_json::json!({
            "email": "good2@test.com",
            "username": "validname",
            "password": "Valid123!",
            "role": "king"
        })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "USER_INVALID_ROLE");
}

#[tokio::test]
async fn create_user_duplicate_email_409() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let email = format!("dup-{}@test.com", uuid::Uuid::new_v4());
    let body = serde_json::json!({
        "email": email,
        "username": "validname",
        "password": "Valid123!"
    });
    // 第一次创建成功
    let resp = request(&app.app, Method::POST, "/api/admin/users", Some(body.clone()), &h(&token)).await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::CREATED);
    // 第二次同邮箱 → 409
    let resp = request(&app.app, Method::POST, "/api/admin/users", Some(body), &h(&token)).await;
    let (status, _, dup_body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(dup_body["code"], "AUTH_EMAIL_EXISTS");
}

#[tokio::test]
async fn patch_user_role_invalid_value_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::PATCH,
        "/api/admin/users/whatever/role",
        Some(serde_json::json!({ "role": "bogus" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "USER_INVALID_ROLE");
}

#[tokio::test]
async fn patch_user_role_unknown_user_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // role 合法 → 越过 normalize_role，落到用户不存在 404
    let resp = request(
        &app.app,
        Method::PATCH,
        "/api/admin/users/no-such-user/role",
        Some(serde_json::json!({ "role": "staff" })),
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ── 批量端点：空数组 / 超限 → 400 BULK_SIZE ──

#[tokio::test]
async fn bulk_ban_empty_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users/bulk-ban",
        Some(serde_json::json!({ "userIds": [] })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "BULK_SIZE");
}

#[tokio::test]
async fn bulk_unban_empty_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users/bulk-unban",
        Some(serde_json::json!({ "userIds": [] })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "BULK_SIZE");
}

#[tokio::test]
async fn bulk_role_empty_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users/bulk-role",
        Some(serde_json::json!({ "userIds": [], "role": "staff" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "BULK_SIZE");
}

#[tokio::test]
async fn bulk_role_invalid_role_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // 非空 userIds 越过 BULK_SIZE，落到 normalize_role 的 USER_INVALID_ROLE
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users/bulk-role",
        Some(serde_json::json!({ "userIds": ["x"], "role": "emperor" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "USER_INVALID_ROLE");
}

#[tokio::test]
async fn bulk_reset_password_empty_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users/bulk-reset-password",
        Some(serde_json::json!({ "userIds": [] })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "BULK_SIZE");
}

#[tokio::test]
async fn bulk_delete_empty_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users/bulk-delete",
        Some(serde_json::json!({ "userIds": [] })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "BULK_SIZE");
}

#[tokio::test]
async fn bulk_delete_over_limit_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // 删除上限 50；51 个 → 400
    let ids: Vec<String> = (0..51).map(|i| format!("u-{i}")).collect();
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users/bulk-delete",
        Some(serde_json::json!({ "userIds": ids })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "BULK_SIZE");
}

#[tokio::test]
async fn bulk_ban_partial_unknown_user_marked_failed() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // 非空但全为不存在 id → 端点本身 200，但个体 result success=false
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users/bulk-ban",
        Some(serde_json::json!({ "userIds": ["ghost-1", "ghost-2"] })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["succeeded"], 0);
    assert_eq!(body["data"]["failed"], 2);
}

#[tokio::test]
async fn device_ban_unknown_device_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/users/some-user/devices/ghost-device/ban",
        None,
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ════════════════════════════════════════════════════════════════════════
// 3) analytics.rs：非法参数 / 边界 range → 400
// ════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn analytics_days_out_of_range_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // ensure_days: 1..=30；0 与 31 都越界
    for path in [
        "/api/admin/analytics/daily-active-users?days=0",
        "/api/admin/analytics/daily-active-users?days=31",
        "/api/admin/analytics/daily-records?days=0",
        "/api/admin/analytics/record-types?days=99",
        "/api/admin/analytics/study-overview?days=31",
    ] {
        let resp = request(&app.app, Method::GET, path, None, &h(&token)).await;
        let (status, _, body) = response_json(resp).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "path: {path}");
        assert_eq!(body["code"], "INVALID_DAYS", "path: {path}");
    }
}

#[tokio::test]
async fn analytics_extended_days_out_of_range_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // ensure_days_extended: 1..=60
    for path in [
        "/api/admin/analytics/hourly?days=0",
        "/api/admin/analytics/hourly?days=61",
        "/api/admin/analytics/wordbook-rank?days=61",
    ] {
        let resp = request(&app.app, Method::GET, path, None, &h(&token)).await;
        let (status, _, body) = response_json(resp).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "path: {path}");
        assert_eq!(body["code"], "INVALID_DAYS", "path: {path}");
    }
}

#[tokio::test]
async fn analytics_invalid_category_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    for path in [
        "/api/admin/analytics/study-overview?category=bogus",
        "/api/admin/analytics/word-states?category=bogus",
        "/api/admin/analytics/retention-curve?category=bogus",
    ] {
        let resp = request(&app.app, Method::GET, path, None, &h(&token)).await;
        let (status, _, body) = response_json(resp).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "path: {path}");
        assert_eq!(body["code"], "INVALID_CATEGORY", "path: {path}");
    }
}

#[tokio::test]
async fn retention_cohort_invalid_max_days_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // max_days: 1..=90
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/analytics/retention-cohort?maxDays=0",
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_MAX_DAYS");

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/analytics/retention-cohort?maxDays=91",
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_MAX_DAYS");
}

#[tokio::test]
async fn retention_cohort_invalid_unit_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // max_days 合法 → 越过 INVALID_MAX_DAYS，落到 cohort 单位枚举校验
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/analytics/retention-cohort?cohort=monthly&maxDays=30",
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_COHORT_UNIT");
}

#[tokio::test]
async fn retention_cohort_daily_ok() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // 覆盖 cohort="daily" 合法臂
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/analytics/retention-cohort?cohort=daily&maxDays=14",
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["cohortUnit"], "daily");
}

#[tokio::test]
async fn window_invalid_days_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // resolve_window: days 只允许 7/14/30/90
    for path in [
        "/api/admin/analytics/kpi-summary?days=5",
        "/api/admin/analytics/funnel?days=13",
        "/api/admin/analytics/question-distribution?days=100",
        "/api/admin/analytics/insights?days=8",
    ] {
        let resp = request(&app.app, Method::GET, path, None, &h(&token)).await;
        let (status, _, body) = response_json(resp).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "path: {path}");
        assert_eq!(body["code"], "INVALID_DAYS", "path: {path}");
    }
}

#[tokio::test]
async fn window_invalid_date_format_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/analytics/kpi-summary?from=notadate&to=2026-01-01",
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_DATE");
}

#[tokio::test]
async fn window_from_after_to_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/analytics/funnel?from=2026-02-01&to=2026-01-01",
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_RANGE");
}

#[tokio::test]
async fn word_frequency_invalid_sort_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // sort 枚举校验在 resolve_window 之后；days 用合法 7
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/analytics/word-frequency?days=7&sort=bogus",
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_SORT");
}

#[tokio::test]
async fn word_frequency_window_invalid_days_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // word_frequency 走 resolve_window：days 必须 7/14/30/90，5 越界且早于 sort 校验
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/analytics/word-frequency?days=5&sort=count",
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_DAYS");
}

// ── analytics happy-path（空数据兜底分支）──

#[tokio::test]
async fn analytics_happy_paths_empty_db() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    for path in [
        "/api/admin/analytics/engagement",
        "/api/admin/analytics/learning",
        "/api/admin/analytics/daily-active-users?days=7",
        "/api/admin/analytics/daily-records?days=7",
        "/api/admin/analytics/study-overview?days=7&category=all",
        "/api/admin/analytics/record-types?days=7",
        "/api/admin/analytics/word-states",
        "/api/admin/analytics/retention-curve",
        "/api/admin/analytics/hourly?days=7",
        "/api/admin/analytics/wordbook-rank?days=30&limit=10",
        "/api/admin/analytics/retention-cohort?cohort=weekly&maxDays=30",
        "/api/admin/analytics/kpi-summary?days=7",
        "/api/admin/analytics/funnel?days=7",
        "/api/admin/analytics/retention-matrix?weeks=4",
        "/api/admin/analytics/question-distribution?days=7",
        "/api/admin/analytics/word-frequency?days=7&sort=count",
        "/api/admin/analytics/insights?days=7",
    ] {
        let resp = request(&app.app, Method::GET, path, None, &h(&token)).await;
        let (status, _, body) = response_json(resp).await;
        assert_eq!(status, StatusCode::OK, "path: {path} body={body}");
        assert_eq!(body["success"], true, "path: {path}");
    }
}

// ════════════════════════════════════════════════════════════════════════
// 4) amas.rs：非法输入 / 边界 / 未授权分支
// ════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn amas_config_invalid_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // 拿当前 config，破坏 monitoring.sampleRate > 1 → validate 失败 400
    let resp = request(&app.app, Method::GET, "/api/admin/amas/config", None, &h(&token)).await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let mut cfg = body["data"].clone();
    // 配置结构体统一 camelCase（rename_all），故必须用 sampleRate；> 1.0 触发 validate 失败
    cfg["monitoring"]["sampleRate"] = serde_json::json!(5.0);
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/admin/amas/config",
        Some(cfg),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "AMAS_INVALID_CONFIG");
}

#[tokio::test]
async fn amas_parse_toml_bad_syntax_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/amas/config/parse-toml",
        Some(serde_json::json!({ "toml": "this is = = not valid toml [[" })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "TOML_PARSE_ERROR");
}

#[tokio::test]
async fn amas_get_version_unknown_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/config/versions/deadbeef-nonexistent",
        None,
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn amas_restore_version_unknown_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/amas/config/versions/deadbeef-nonexistent/restore",
        Some(serde_json::json!({})),
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn amas_set_canary_percent_over_100_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/admin/amas/config/canary",
        Some(serde_json::json!({ "versionHash": "whatever", "percent": 150 })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_PERCENT");
}

#[tokio::test]
async fn amas_set_canary_unknown_version_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // percent 合法 → 越过 INVALID_PERCENT，落到 version_hash 不存在校验
    let resp = request(
        &app.app,
        Method::PUT,
        "/api/admin/amas/config/canary",
        Some(serde_json::json!({ "versionHash": "no-such-version", "percent": 10 })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "VERSION_HASH_NOT_FOUND");
}

#[tokio::test]
async fn amas_get_canary_empty_ok() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // 无 active canary → 200 且 canary=null
    let resp = request(&app.app, Method::GET, "/api/admin/amas/config/canary", None, &h(&token)).await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["canary"].is_null());
}

#[tokio::test]
async fn amas_disable_canary_when_none_ok() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/amas/config/canary/disable",
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["disabled"], false);
}

#[tokio::test]
async fn amas_list_suggestions_bad_status_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/suggestions?status=bogus",
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "BAD_STATUS");
}

#[tokio::test]
async fn amas_export_csv_bad_status_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/suggestions/export.csv?status=bogus",
        None,
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn amas_get_suggestion_unknown_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/suggestions/999999",
        None,
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn amas_approve_unknown_suggestion_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/amas/suggestions/999999/approve",
        Some(serde_json::json!({})),
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn amas_approve_non_pending_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // 直接落一条 Rejected 状态的 suggestion → approve 应 400 BAD_STATUS
    let sid = app
        .state
        .store()
        .insert_amas_suggestion(
            &learning_backend::store::operations::amas_suggestions::InsertSuggestion {
                based_on_version_hash: "h".into(),
                patch_json: r#"{"memoryModel.baseDesiredRetention":0.85}"#.into(),
                rationale: "non-pending".into(),
                evidence_json: "{}".into(),
                cost_usd: Some(0.0),
                tokens_input: Some(0),
                tokens_output: Some(0),
                confidence: Some(0.5),
                initial_status:
                    learning_backend::store::operations::amas_suggestions::SuggestionStatus::Rejected,
                decided_by: None,
                decision_note: None,
                base_values_json: None,
            },
        )
        .expect("insert suggestion");

    let resp = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/amas/suggestions/{sid}/approve"),
        Some(serde_json::json!({})),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "BAD_STATUS");
}

#[tokio::test]
async fn amas_explain_param_llm_disabled_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // test config llm.enabled=false → LLM_DISABLED
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/amas/suggestions/explain",
        Some(serde_json::json!({
            "path": "memoryModel.baseDesiredRetention",
            "currentValue": 0.85
        })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "LLM_DISABLED");
}

#[tokio::test]
async fn amas_add_whitelist_bad_path_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/amas/advisor/whitelist",
        Some(serde_json::json!({ "path": "ensemble.foo", "minSafe": 0.0, "maxSafe": 1.0 })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_PATH");
}

#[tokio::test]
async fn amas_add_whitelist_bad_range_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // path 合法 → 越过 INVALID_PATH，落到 minSafe >= maxSafe → INVALID_RANGE
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/amas/advisor/whitelist",
        Some(serde_json::json!({ "path": "memoryModel.foo", "minSafe": 2.0, "maxSafe": 1.0 })),
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_RANGE");
}

#[tokio::test]
async fn amas_delete_whitelist_bad_path_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::DELETE,
        "/api/admin/amas/advisor/whitelist/ensemble.foo",
        None,
        &h(&token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_PATH");
}

#[tokio::test]
async fn amas_rollback_suggestion_unknown_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/amas/suggestions/999999/rollback",
        None,
        &h(&token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn amas_compare_missing_query_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    // CompareQuery 的 version_a / version_b 为必填 → 缺失触发 Query 反序列化 400
    let resp = request(&app.app, Method::GET, "/api/admin/amas/compare", None, &h(&token)).await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ── amas user-side（AuthUser）端点：边界 + 鉴权 ──

#[tokio::test]
async fn amas_visual_fatigue_score_out_of_range_400() {
    let app = spawn_test_server().await;
    let user_token = login_and_get_token(&app.app).await;
    for score in [-1.0, 101.0] {
        let resp = request(
            &app.app,
            Method::POST,
            "/api/amas/visual-fatigue",
            Some(serde_json::json!({ "score": score })),
            &h(&user_token),
        )
        .await;
        let (status, _, body) = response_json(resp).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "score={score}");
        assert_eq!(body["code"], "INVALID_SCORE", "score={score}");
    }
}

#[tokio::test]
async fn amas_visual_fatigue_valid_score_ok() {
    let app = spawn_test_server().await;
    let user_token = login_and_get_token(&app.app).await;
    let resp = request(
        &app.app,
        Method::POST,
        "/api/amas/visual-fatigue",
        Some(serde_json::json!({ "score": 42.0 })),
        &h(&user_token),
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn amas_user_endpoints_require_user_token() {
    let app = spawn_test_server().await;
    // 无 token 打 user-side AMAS 端点 → 401
    for (method, path, body) in [
        (Method::GET, "/api/amas/state", None),
        (Method::GET, "/api/amas/strategy", None),
        (Method::GET, "/api/amas/phase", None),
        (Method::GET, "/api/amas/intervention", None),
        (
            Method::POST,
            "/api/amas/process-event",
            Some(serde_json::json!({ "wordId": "w", "isCorrect": true, "responseTime": 100 })),
        ),
    ] {
        let resp = request(&app.app, method, path, body, &[]).await;
        let (status, _, _) = response_json(resp).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "path: {path}");
    }
}

#[tokio::test]
async fn amas_batch_too_large_400() {
    let app = spawn_test_server().await;
    let user_token = login_and_get_token(&app.app).await;
    // limits.max_batch_size 默认值；501 条超限触发 BATCH_TOO_LARGE
    let events: Vec<serde_json::Value> = (0..1001)
        .map(|i| {
            serde_json::json!({
                "wordId": format!("w-{i}"),
                "isCorrect": true,
                "responseTime": 100
            })
        })
        .collect();
    let resp = request(
        &app.app,
        Method::POST,
        "/api/amas/batch-process",
        Some(serde_json::json!({ "events": events })),
        &h(&user_token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "BATCH_TOO_LARGE");
}

#[tokio::test]
async fn amas_evaluate_mastery_missing_word_returns_new() {
    let app = spawn_test_server().await;
    let user_token = login_and_get_token(&app.app).await;
    // word_state 为 None → 兜底 NEW 分支
    let resp = request(
        &app.app,
        Method::GET,
        "/api/amas/mastery/evaluate?wordId=ghost-word",
        None,
        &h(&user_token),
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["state"], "NEW");
}
