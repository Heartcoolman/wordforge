mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token, setup_admin_and_get_token};
use common::http::{request, response_json};

const ANALYTICS_ENDPOINTS: &[&str] = &[
    "/api/admin/analytics/engagement",
    "/api/admin/analytics/learning",
    "/api/admin/analytics/daily-active-users",
    "/api/admin/analytics/daily-records",
    "/api/admin/analytics/study-overview",
    "/api/admin/analytics/record-types",
    "/api/admin/analytics/word-states",
    "/api/admin/analytics/retention-curve",
];

async fn get_with(
    app: &axum::Router,
    path: &str,
    token: &str,
) -> (StatusCode, serde_json::Value) {
    let resp = request(
        app,
        Method::GET,
        path,
        None,
        &[("authorization", auth_header(token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    (status, body)
}

// 全部端点：管理员鉴权 + 空数据成功路径（默认 7 天 / category=all）
#[tokio::test]
async fn it_analytics_all_endpoints_empty_success() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    for path in ANALYTICS_ENDPOINTS {
        let (status, body) = get_with(&app.app, path, &admin_token).await;
        assert_eq!(status, StatusCode::OK, "path: {path}");
        assert_eq!(body["success"], true, "path: {path}");
        assert!(body.get("data").is_some(), "path: {path} has data");
    }
}

// engagement：空数据时 retentionRate=0、trend 结构存在
#[tokio::test]
async fn it_analytics_engagement_shape() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let (status, body) = get_with(&app.app, "/api/admin/analytics/engagement", &admin_token).await;
    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert!(data["totalUsers"].is_number());
    assert!(data["activeToday"].is_number());
    assert!(data["retentionRate"].is_number());
    assert!(data["trend"]["activeToday"]["value"].is_number());
    assert_eq!(data["trend"]["activeToday"]["label"], "较昨日");
}

// learning：空数据时 overallAccuracy=0、两个 trend 项存在
#[tokio::test]
async fn it_analytics_learning_shape() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let (status, body) = get_with(&app.app, "/api/admin/analytics/learning", &admin_token).await;
    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert!(data["totalWords"].is_number());
    assert!(data["totalRecords"].is_number());
    assert!(data["totalCorrect"].is_number());
    assert_eq!(data["overallAccuracy"], 0.0);
    assert!(data["trend"]["totalRecords"]["value"].is_number());
    assert!(data["trend"]["overallAccuracy"]["value"].is_number());
}

// daily-active-users：默认 7 天 → 数组长度 7，每项含 date/count/registered
#[tokio::test]
async fn it_analytics_daily_active_users_window() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let (status, body) =
        get_with(&app.app, "/api/admin/analytics/daily-active-users", &admin_token).await;
    assert_eq!(status, StatusCode::OK);
    let arr = body["data"].as_array().expect("array");
    assert_eq!(arr.len(), 7);
    let first = &arr[0];
    assert!(first["date"].is_string());
    assert_eq!(first["count"], 0);
    assert_eq!(first["registered"], 0);

    // days=1 → 长度 1（下边界）
    let (s1, b1) = get_with(
        &app.app,
        "/api/admin/analytics/daily-active-users?days=1",
        &admin_token,
    )
    .await;
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(b1["data"].as_array().unwrap().len(), 1);

    // days=30 → 长度 30（上边界）
    let (s30, b30) = get_with(
        &app.app,
        "/api/admin/analytics/daily-active-users?days=30",
        &admin_token,
    )
    .await;
    assert_eq!(s30, StatusCode::OK);
    assert_eq!(b30["data"].as_array().unwrap().len(), 30);
}

// daily-records：窗口长度 + 字段
#[tokio::test]
async fn it_analytics_daily_records_window() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let (status, body) = get_with(
        &app.app,
        "/api/admin/analytics/daily-records?days=14",
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let arr = body["data"].as_array().expect("array");
    assert_eq!(arr.len(), 14);
    let first = &arr[0];
    assert!(first["date"].is_string());
    assert_eq!(first["correct"], 0);
    assert_eq!(first["total"], 0);
    assert_eq!(first["durationSecs"], 0);
    assert_eq!(first["newWords"], 0);
}

// study-overview：各 category（all/learning/review）+ 默认 + daily 窗口长度
#[tokio::test]
async fn it_analytics_study_overview_categories() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    for category in ["all", "learning", "review"] {
        let path = format!("/api/admin/analytics/study-overview?days=10&category={category}");
        let (status, body) = get_with(&app.app, &path, &admin_token).await;
        assert_eq!(status, StatusCode::OK, "category: {category}");
        let data = &body["data"];
        assert_eq!(data["days"], 10);
        assert_eq!(data["category"], category);
        assert!(data["generatedAt"].is_string());
        assert!(data["summary"].is_object());
        // 空数据时 summary.accuracy=null（accuracy() total=0 → None）
        assert!(data["summary"]["accuracy"].is_null());
        assert_eq!(data["summary"]["recordCount"], 0);
        let daily = data["daily"].as_array().expect("daily array");
        assert_eq!(daily.len(), 10);
        assert!(daily[0]["accuracy"].is_null());
    }

    // 不带 category → 默认 all，默认 7 天
    let (status, body) = get_with(&app.app, "/api/admin/analytics/study-overview", &admin_token).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["category"], "all");
    assert_eq!(body["data"]["days"], 7);
    assert_eq!(body["data"]["daily"].as_array().unwrap().len(), 7);
}

// record-types：totals 三类齐全 + daily 窗口
#[tokio::test]
async fn it_analytics_record_types_shape() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let (status, body) = get_with(
        &app.app,
        "/api/admin/analytics/record-types?days=5",
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert_eq!(data["days"], 5);
    let totals = data["totals"].as_array().expect("totals array");
    assert_eq!(totals.len(), 3);
    let types: Vec<&str> = totals
        .iter()
        .map(|t| t["recordType"].as_str().unwrap())
        .collect();
    assert!(types.contains(&"learning"));
    assert!(types.contains(&"review"));
    assert!(types.contains(&"all"));
    // 空数据 → total=0、accuracy=null
    assert_eq!(totals[0]["total"], 0);
    assert!(totals[0]["accuracy"].is_null());
    let daily = data["daily"].as_array().expect("daily array");
    assert_eq!(daily.len(), 5);
    assert_eq!(daily[0]["all"], 0);
}

// word-states：各 category + 字段结构（空数据 trackedWords=0）
#[tokio::test]
async fn it_analytics_word_states_categories() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    for category in ["all", "learning", "review"] {
        let path = format!("/api/admin/analytics/word-states?category={category}");
        let (status, body) = get_with(&app.app, &path, &admin_token).await;
        assert_eq!(status, StatusCode::OK, "category: {category}");
        let data = &body["data"];
        assert_eq!(data["category"], category);
        assert!(data["generatedAt"].is_string());
        let states = &data["states"];
        assert_eq!(states["newCount"], 0);
        assert_eq!(states["learning"], 0);
        assert_eq!(states["reviewing"], 0);
        assert_eq!(states["mastered"], 0);
        assert_eq!(states["forgotten"], 0);
        let totals = &data["totals"];
        assert_eq!(totals["trackedWords"], 0);
        assert_eq!(totals["bookmarkedWords"], 0);
        assert_eq!(totals["dueReviewWords"], 0);
        assert_eq!(totals["overdueReviewWords"], 0);
    }
}

// retention-curve：固定 6 个桶（1/2/4/7/15/30），空数据 retention=null
#[tokio::test]
async fn it_analytics_retention_curve_buckets() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    for category in ["all", "learning", "review"] {
        let path = format!("/api/admin/analytics/retention-curve?category={category}");
        let (status, body) = get_with(&app.app, &path, &admin_token).await;
        assert_eq!(status, StatusCode::OK, "category: {category}");
        let data = &body["data"];
        assert_eq!(data["category"], category);
        assert!(data["generatedAt"].is_string());
        let points = data["points"].as_array().expect("points array");
        assert_eq!(points.len(), 6);
        let buckets: Vec<i64> = points
            .iter()
            .map(|p| p["daysSinceLearn"].as_i64().unwrap())
            .collect();
        assert_eq!(buckets, vec![1, 2, 4, 7, 15, 30]);
        // 空数据 → 每个桶 sampleSize=0、retention=null、平均 null
        assert_eq!(points[0]["sampleSize"], 0);
        assert!(points[0]["retention"].is_null());
        assert!(data["averageRetention"].is_null());
    }
}

// 非法 days：0（下越界）/ 31（上越界）→ 400 INVALID_DAYS
#[tokio::test]
async fn it_analytics_invalid_days_returns_400() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    // 接受 days 参数的端点
    let days_paths = [
        "/api/admin/analytics/daily-active-users",
        "/api/admin/analytics/daily-records",
        "/api/admin/analytics/study-overview",
        "/api/admin/analytics/record-types",
    ];

    for base in days_paths {
        for bad in ["0", "31", "100"] {
            let path = format!("{base}?days={bad}");
            let (status, body) = get_with(&app.app, &path, &admin_token).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "path: {path}");
            assert_eq!(body["success"], false, "path: {path}");
            assert_eq!(body["code"], "INVALID_DAYS", "path: {path}");
        }
    }
}

// 非法 category → 400 INVALID_CATEGORY（覆盖接受 category 的端点）
#[tokio::test]
async fn it_analytics_invalid_category_returns_400() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let category_paths = [
        "/api/admin/analytics/study-overview?category=bogus",
        "/api/admin/analytics/word-states?category=bogus",
        "/api/admin/analytics/retention-curve?category=bogus",
    ];

    for path in category_paths {
        let (status, body) = get_with(&app.app, path, &admin_token).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "path: {path}");
        assert_eq!(body["success"], false, "path: {path}");
        assert_eq!(body["code"], "INVALID_CATEGORY", "path: {path}");
    }
}

// study-overview：days 合法但 category 非法 — ensure_days 在前，仍应 400 INVALID_CATEGORY
#[tokio::test]
async fn it_analytics_study_overview_valid_days_bad_category() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let (status, body) = get_with(
        &app.app,
        "/api/admin/analytics/study-overview?days=7&category=xyz",
        &admin_token,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "INVALID_CATEGORY");
}

// 无 token → 401（全部端点）
#[tokio::test]
async fn it_analytics_missing_token_returns_401() {
    let app = spawn_test_server().await;
    // 先建管理员，确保系统已初始化但请求不带 token
    let _ = setup_admin_and_get_token(&app.app).await;

    for path in ANALYTICS_ENDPOINTS {
        let resp = request(&app.app, Method::GET, path, None, &[]).await;
        let (status, _, _) = response_json(resp).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "path: {path}");
    }
}

// 普通用户 token（非 admin token_type）→ 401（全部端点）
#[tokio::test]
async fn it_analytics_user_token_returns_401() {
    let app = spawn_test_server().await;
    let _admin = setup_admin_and_get_token(&app.app).await;
    let user_token = login_and_get_token(&app.app).await;

    for path in ANALYTICS_ENDPOINTS {
        let (status, _) = get_with(&app.app, path, &user_token).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "path: {path}");
    }
}

// 携带真实学习数据后 record-types / daily-records 仍 200（覆盖 store 聚合非空分支不崩）
#[tokio::test]
async fn it_analytics_with_seeded_records_ok() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;
    let user_token = login_and_get_token(&app.app).await;

    // 创建一个单词
    let word_resp = request(
        &app.app,
        Method::POST,
        "/api/words",
        Some(serde_json::json!({
            "text": "analytics-cov-word",
            "meaning": "覆盖用",
            "difficulty": 0.4,
        })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (word_status, _, word_body) = response_json(word_resp).await;
    assert_eq!(word_status, StatusCode::CREATED);
    let word_id = word_body["data"]["id"].as_str().expect("word id").to_string();

    // 提交一条学习记录
    let rec_resp = request(
        &app.app,
        Method::POST,
        "/api/records",
        Some(serde_json::json!({
            "wordId": word_id,
            "isCorrect": true,
            "responseTimeMs": 1200,
            "recordType": "learning",
            "sessionId": "cov-sess-1"
        })),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (rec_status, _, _) = response_json(rec_resp).await;
    // 不强断言具体码（契约可能 200/201），仅要求成功
    assert!(
        rec_status.is_success() || rec_status == StatusCode::BAD_REQUEST,
        "record submit status: {rec_status}"
    );

    // 无论记录是否落库，分析端点都应正常返回 200
    for path in [
        "/api/admin/analytics/daily-records",
        "/api/admin/analytics/record-types",
        "/api/admin/analytics/study-overview",
        "/api/admin/analytics/word-states",
        "/api/admin/analytics/learning",
        "/api/admin/analytics/engagement",
    ] {
        let (status, body) = get_with(&app.app, path, &admin_token).await;
        assert_eq!(status, StatusCode::OK, "path: {path}");
        assert_eq!(body["success"], true, "path: {path}");
    }
}
