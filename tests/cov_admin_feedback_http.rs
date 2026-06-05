mod common;

use axum::http::{Method, StatusCode};
use chrono::Utc;

use learning_backend::store::operations::feedback::FeedbackItem;

use common::app::{spawn_test_server, TestApp};
use common::auth::{auth_header, setup_admin_and_get_token};
use common::fixtures::seed_user;
use common::http::{request, response_json};

/// 直接通过 store 预置一条反馈记录（路由层没有创建端点）。
fn seed_feedback(
    app: &TestApp,
    id: &str,
    user_id: &str,
    category: &str,
    status: &str,
) -> FeedbackItem {
    let item = FeedbackItem {
        id: id.to_string(),
        user_id: user_id.to_string(),
        category: Some(category.to_string()),
        body: format!("body-{id}"),
        route: Some("/home".to_string()),
        created_at: Utc::now(),
        priority: "normal".to_string(),
        status: status.to_string(),
        assignee_admin_id: None,
        resolved_at: None,
        resolution: None,
        device_profile: None,
        answer_snapshot: None,
        read_at: None,
        first_response_at: None,
        csat_score: None,
        csat_comment: None,
        dedup_count: 0,
        github_issue_url: None,
        user_name: None,
        platform: None,
        app_version: None,
    };
    app.state
        .store()
        .create_feedback(&item, &[])
        .expect("seed feedback");
    item
}

fn seed_owner(app: &TestApp, tag: &str) -> String {
    let user = seed_user(
        app.state.store(),
        &format!("{tag}-{}@test.com", uuid::Uuid::new_v4()),
        &format!("{tag}-{}", uuid::Uuid::new_v4().simple()),
        "Passw0rd!",
    );
    user.id
}

// ---------------------------------------------------------------------------
// GET /api/admin/feedback  (list_feedback)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_requires_admin_token() {
    let app = spawn_test_server().await;
    let resp = request(&app.app, Method::GET, "/api/admin/feedback", None, &[]).await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_empty_returns_paginated_envelope() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/feedback",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], true);
    assert_eq!(body["data"]["total"], 0);
    assert_eq!(body["data"]["page"], 1);
    // 默认 per_page = 20
    assert_eq!(body["data"]["perPage"], 20);
    assert!(body["data"]["data"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn list_populated_with_default_pagination() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let owner = seed_owner(&app, "lister");

    for i in 0..3 {
        seed_feedback(&app, &format!("fb-list-{i}"), &owner, "bug", "open");
    }

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/feedback",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["total"], 3);
    assert_eq!(body["data"]["data"].as_array().unwrap().len(), 3);
    assert_eq!(body["data"]["totalPages"], 1);
}

#[tokio::test]
async fn list_filters_by_status() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let owner = seed_owner(&app, "statusfilter");

    seed_feedback(&app, "fb-st-open", &owner, "bug", "open");
    seed_feedback(&app, "fb-st-resolved", &owner, "bug", "resolved");

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/feedback?status=open",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["data"][0]["id"], "fb-st-open");
}

#[tokio::test]
async fn list_filters_by_category() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let owner = seed_owner(&app, "catfilter");

    seed_feedback(&app, "fb-cat-bug", &owner, "bug", "open");
    seed_feedback(&app, "fb-cat-idea", &owner, "idea", "open");

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/feedback?category=idea",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["data"][0]["id"], "fb-cat-idea");
    assert_eq!(body["data"]["data"][0]["category"], "idea");
}

#[tokio::test]
async fn list_filters_by_category_and_status_combined() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let owner = seed_owner(&app, "combined");

    seed_feedback(&app, "fb-cb-1", &owner, "bug", "open");
    seed_feedback(&app, "fb-cb-2", &owner, "bug", "resolved");
    seed_feedback(&app, "fb-cb-3", &owner, "idea", "open");

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/feedback?category=bug&status=resolved",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["data"][0]["id"], "fb-cb-2");
}

#[tokio::test]
async fn list_per_page_clamped_to_max() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;

    // per_page=9999 应被 clamp 到 max_page_size (默认 100)
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/feedback?perPage=9999",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["perPage"], 100);
}

#[tokio::test]
async fn list_explicit_pagination_second_page() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let owner = seed_owner(&app, "pager");

    for i in 0..3 {
        seed_feedback(&app, &format!("fb-pg-{i}"), &owner, "bug", "open");
    }

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/feedback?page=2&perPage=2",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["total"], 3);
    assert_eq!(body["data"]["page"], 2);
    assert_eq!(body["data"]["perPage"], 2);
    assert_eq!(body["data"]["totalPages"], 2);
    // 第二页应只剩 1 条
    assert_eq!(body["data"]["data"].as_array().unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// PATCH /api/admin/feedback/:id  (update_feedback)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_requires_admin_token() {
    let app = spawn_test_server().await;
    let resp = request(
        &app.app,
        Method::PATCH,
        "/api/admin/feedback/whatever",
        Some(serde_json::json!({ "status": "open" })),
        &[],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn update_nonexistent_returns_404() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;

    let resp = request(
        &app.app,
        Method::PATCH,
        "/api/admin/feedback/does-not-exist",
        Some(serde_json::json!({ "status": "in_progress" })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "NOT_FOUND");
}

#[tokio::test]
async fn update_status_resolved_sets_resolved_at() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let owner = seed_owner(&app, "resolver");
    seed_feedback(&app, "fb-up-resolved", &owner, "bug", "open");

    let resp = request(
        &app.app,
        Method::PATCH,
        "/api/admin/feedback/fb-up-resolved",
        Some(serde_json::json!({
            "status": "resolved",
            "resolution": "已在 v1.2 修复"
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "resolved");
    assert_eq!(body["resolution"], "已在 v1.2 修复");
    assert!(!body["resolvedAt"].is_null());
}

#[tokio::test]
async fn update_status_non_resolved_clears_resolved_at() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let owner = seed_owner(&app, "reopen");
    seed_feedback(&app, "fb-up-reopen", &owner, "bug", "open");

    // 先标 resolved 写入 resolved_at
    let r1 = request(
        &app.app,
        Method::PATCH,
        "/api/admin/feedback/fb-up-reopen",
        Some(serde_json::json!({ "status": "resolved" })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (s1, _, b1) = response_json(r1).await;
    assert_eq!(s1, StatusCode::OK);
    assert!(!b1["resolvedAt"].is_null());

    // 再切回 in_progress，resolved_at 应被清空
    let r2 = request(
        &app.app,
        Method::PATCH,
        "/api/admin/feedback/fb-up-reopen",
        Some(serde_json::json!({ "status": "in_progress" })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (s2, _, b2) = response_json(r2).await;
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(b2["status"], "in_progress");
    assert!(b2["resolvedAt"].is_null());
}

#[tokio::test]
async fn update_priority_only() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let owner = seed_owner(&app, "prio");
    seed_feedback(&app, "fb-up-prio", &owner, "bug", "open");

    let resp = request(
        &app.app,
        Method::PATCH,
        "/api/admin/feedback/fb-up-prio",
        Some(serde_json::json!({ "priority": "urgent" })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["priority"], "urgent");
    // status 未改动应保持 open
    assert_eq!(body["status"], "open");
}

#[tokio::test]
async fn update_assignee_set() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let owner = seed_owner(&app, "assign");
    seed_feedback(&app, "fb-up-assign", &owner, "bug", "open");

    // 指派
    let r1 = request(
        &app.app,
        Method::PATCH,
        "/api/admin/feedback/fb-up-assign",
        Some(serde_json::json!({ "assigneeAdminId": "admin-42" })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (s1, _, b1) = response_json(r1).await;
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(b1["assigneeAdminId"], "admin-42");
}

#[tokio::test]
async fn update_assignee_null_is_treated_as_absent() {
    // 说明：UpdateFeedbackRequest.assignee_admin_id 为 `Option<Option<String>>`，
    // 但未配置 double-option 反序列化器；标准 serde 行为下 JSON 显式 null 映射到
    // 外层 None（视为字段缺省），因此 SET 子句被跳过，已有指派不会被清空。
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let owner = seed_owner(&app, "assignnull");
    seed_feedback(&app, "fb-up-assignnull", &owner, "bug", "open");

    // 先指派
    let r1 = request(
        &app.app,
        Method::PATCH,
        "/api/admin/feedback/fb-up-assignnull",
        Some(serde_json::json!({ "assigneeAdminId": "admin-9" })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (s1, _, b1) = response_json(r1).await;
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(b1["assigneeAdminId"], "admin-9");

    // 发送显式 null：字段被当作缺省，指派保持不变
    let r2 = request(
        &app.app,
        Method::PATCH,
        "/api/admin/feedback/fb-up-assignnull",
        Some(serde_json::json!({ "assigneeAdminId": null })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (s2, _, b2) = response_json(r2).await;
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(b2["assigneeAdminId"], "admin-9");
}

#[tokio::test]
async fn update_all_fields_at_once() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let owner = seed_owner(&app, "allfields");
    seed_feedback(&app, "fb-up-all", &owner, "bug", "open");

    let resp = request(
        &app.app,
        Method::PATCH,
        "/api/admin/feedback/fb-up-all",
        Some(serde_json::json!({
            "priority": "high",
            "status": "resolved",
            "assigneeAdminId": "admin-7",
            "resolution": "全字段更新"
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["priority"], "high");
    assert_eq!(body["status"], "resolved");
    assert_eq!(body["assigneeAdminId"], "admin-7");
    assert_eq!(body["resolution"], "全字段更新");
    assert!(!body["resolvedAt"].is_null());
}

#[tokio::test]
async fn update_empty_body_returns_unchanged_record() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let owner = seed_owner(&app, "noop");
    seed_feedback(&app, "fb-up-noop", &owner, "bug", "open");

    // 空补丁：没有任何 SET 字段，应原样回读
    let resp = request(
        &app.app,
        Method::PATCH,
        "/api/admin/feedback/fb-up-noop",
        Some(serde_json::json!({})),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], "fb-up-noop");
    assert_eq!(body["status"], "open");
    assert_eq!(body["priority"], "normal");
}

#[tokio::test]
async fn update_invalid_priority_returns_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let owner = seed_owner(&app, "badprio");
    seed_feedback(&app, "fb-up-badprio", &owner, "bug", "open");

    let resp = request(
        &app.app,
        Method::PATCH,
        "/api/admin/feedback/fb-up-badprio",
        Some(serde_json::json!({ "priority": "critical" })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn update_invalid_status_returns_400() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let owner = seed_owner(&app, "badstatus");
    seed_feedback(&app, "fb-up-badstatus", &owner, "bug", "open");

    let resp = request(
        &app.app,
        Method::PATCH,
        "/api/admin/feedback/fb-up-badstatus",
        Some(serde_json::json!({ "status": "archived" })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn update_id_with_colon_rejected_by_validate_id() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;

    // 路径含冒号 → validate_id 返回 Validation → 400 VALIDATION_ERROR
    let resp = request(
        &app.app,
        Method::PATCH,
        "/api/admin/feedback/a:b",
        Some(serde_json::json!({ "status": "open" })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "VALIDATION_ERROR");
}
