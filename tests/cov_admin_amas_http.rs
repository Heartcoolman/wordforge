mod common;

use axum::http::{Method, StatusCode};

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token, setup_admin_and_get_token};
use common::http::{request, response_json};

use learning_backend::store::operations::amas_suggestions::{InsertSuggestion, SuggestionStatus};
use learning_backend::store::operations::amas_versions::ConfigVersionSource;

/// 取一份合法的 AMASConfig（从在线接口读，避免手写结构漂移），返回 (data 节点, 序列化字符串)
async fn fetch_config(app: &axum::Router, admin_token: &str) -> (serde_json::Value, String) {
    let resp = request(
        app,
        Method::GET,
        "/api/admin/amas/config",
        None,
        &[("authorization", auth_header(admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let data = body["data"].clone();
    let s = serde_json::to_string(&data).expect("serialize config");
    (data, s)
}

// ─────────── 用户态端点：visual-fatigue / retention-curve / evaluate ───────────

#[tokio::test]
async fn it_visual_fatigue_success_and_invalid_score() {
    let app = spawn_test_server().await;
    let user_token = login_and_get_token(&app.app).await;

    // 成功：分数在 [0,100]
    let ok_resp = request(
        &app.app,
        Method::POST,
        "/api/amas/visual-fatigue",
        Some(serde_json::json!({ "score": 42.0 })),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (ok_status, _, ok_body) = response_json(ok_resp).await;
    assert_eq!(ok_status, StatusCode::OK);
    assert!(ok_body["data"].is_object());

    // 边界：score 越界 → INVALID_SCORE 400
    let bad_resp = request(
        &app.app,
        Method::POST,
        "/api/amas/visual-fatigue",
        Some(serde_json::json!({ "score": 150.0 })),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (bad_status, _, bad_body) = response_json(bad_resp).await;
    assert_eq!(bad_status, StatusCode::BAD_REQUEST);
    assert_eq!(bad_body["code"], "INVALID_SCORE");

    // 边界：负分也越界
    let neg_resp = request(
        &app.app,
        Method::POST,
        "/api/amas/visual-fatigue",
        Some(serde_json::json!({ "score": -1.0 })),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (neg_status, _, _) = response_json(neg_resp).await;
    assert_eq!(neg_status, StatusCode::BAD_REQUEST);

    // 未授权
    let unauth = request(
        &app.app,
        Method::POST,
        "/api/amas/visual-fatigue",
        Some(serde_json::json!({ "score": 10.0 })),
        &[],
    )
    .await;
    let (unauth_status, _, _) = response_json(unauth).await;
    assert_eq!(unauth_status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn it_retention_curve_and_evaluate_mastery() {
    let app = spawn_test_server().await;
    let user_token = login_and_get_token(&app.app).await;

    // retention-curve 成功（无数据时也应 200，桶 retention 全 None）
    let rc = request(
        &app.app,
        Method::GET,
        "/api/amas/retention-curve",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (rc_status, _, rc_body) = response_json(rc).await;
    assert_eq!(rc_status, StatusCode::OK);
    assert!(rc_body["data"]["points"].is_array());
    // 6 个固定桶 1/2/4/7/15/30
    assert_eq!(rc_body["data"]["points"].as_array().unwrap().len(), 6);

    // evaluate-mastery：缺 word 数据返回 new 默认结构（wire 上 WordState 恒 lowercase）
    let em = request(
        &app.app,
        Method::GET,
        "/api/amas/mastery/evaluate?wordId=does-not-exist",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (em_status, _, em_body) = response_json(em).await;
    assert_eq!(em_status, StatusCode::OK);
    assert_eq!(em_body["data"]["state"], "new");
    assert_eq!(em_body["data"]["wordId"], "does-not-exist");

    // evaluate-mastery：缺 wordId 查询参数 → 400（axum Query 反序列化失败）
    let em_missing = request(
        &app.app,
        Method::GET,
        "/api/amas/mastery/evaluate",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (em_missing_status, _, _) = response_json(em_missing).await;
    assert_eq!(em_missing_status, StatusCode::BAD_REQUEST);

    // 未授权访问用户态端点
    let unauth = request(&app.app, Method::GET, "/api/amas/retention-curve", None, &[]).await;
    let (unauth_status, _, _) = response_json(unauth).await;
    assert_eq!(unauth_status, StatusCode::UNAUTHORIZED);
}

// ─────────── config schema / versions / restore ───────────

#[tokio::test]
async fn it_config_schema_and_versions_flow() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;
    let user_token = login_and_get_token(&app.app).await;

    // config/schema 成功
    let schema = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/config/schema",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (schema_status, _, schema_body) = response_json(schema).await;
    assert_eq!(schema_status, StatusCode::OK);
    assert!(schema_body["data"].is_object());

    // config/schema 未授权
    let schema_unauth = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/config/schema",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (schema_unauth_status, _, _) = response_json(schema_unauth).await;
    assert_eq!(schema_unauth_status, StatusCode::UNAUTHORIZED);

    // 通过 store 直接插一条合法版本快照
    let (_data, snapshot_str) = fetch_config(&app.app, &admin_token).await;
    let (_id, version_hash) = app
        .state
        .store()
        .insert_amas_config_version(
            &snapshot_str,
            "seed-admin",
            ConfigVersionSource::Manual,
            Some("seed version"),
            None,
        )
        .expect("insert version");

    // list versions 成功
    let list = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/config/versions?limit=10",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (list_status, _, list_body) = response_json(list).await;
    assert_eq!(list_status, StatusCode::OK);
    let arr = list_body["data"].as_array().expect("versions array");
    assert!(arr.iter().any(|v| v["versionHash"] == version_hash));

    // get version 成功
    let get_v = request(
        &app.app,
        Method::GET,
        &format!("/api/admin/amas/config/versions/{version_hash}"),
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (get_v_status, _, get_v_body) = response_json(get_v).await;
    assert_eq!(get_v_status, StatusCode::OK);
    assert_eq!(get_v_body["data"]["versionHash"], version_hash);

    // get version 404
    let get_404 = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/config/versions/deadbeefdeadbeef",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (get_404_status, _, _) = response_json(get_404).await;
    assert_eq!(get_404_status, StatusCode::NOT_FOUND);

    // restore 成功
    let restore = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/amas/config/versions/{version_hash}/restore"),
        Some(serde_json::json!({ "note": "回滚测试" })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (restore_status, _, restore_body) = response_json(restore).await;
    assert_eq!(restore_status, StatusCode::OK);
    assert_eq!(restore_body["data"]["updated"], true);

    // restore 不存在的 hash → 404
    let restore_404 = request(
        &app.app,
        Method::POST,
        "/api/admin/amas/config/versions/deadbeefdeadbeef/restore",
        Some(serde_json::json!({})),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (restore_404_status, _, _) = response_json(restore_404).await;
    assert_eq!(restore_404_status, StatusCode::NOT_FOUND);

    // versions 列表未授权
    let list_unauth = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/config/versions",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (list_unauth_status, _, _) = response_json(list_unauth).await;
    assert_eq!(list_unauth_status, StatusCode::UNAUTHORIZED);
}

// ─────────── 指标 / 异常 / 分布 / 对比 ───────────

#[tokio::test]
async fn it_metrics_timeseries_anomalies_distribution_compare() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;
    let user_token = login_and_get_token(&app.app).await;

    // metrics/timeseries（带 days，会被 clamp 到 1..90）
    let ts = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/metrics/timeseries?days=1000",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (ts_status, _, ts_body) = response_json(ts).await;
    assert_eq!(ts_status, StatusCode::OK);
    assert!(ts_body["data"].is_array());

    // metrics/timeseries 默认参数
    let ts_default = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/metrics/timeseries",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (ts_default_status, _, _) = response_json(ts_default).await;
    assert_eq!(ts_default_status, StatusCode::OK);

    // anomalies
    let an = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/anomalies?days=7",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (an_status, _, _) = response_json(an).await;
    assert_eq!(an_status, StatusCode::OK);

    // user-state/distribution（带 days + bins）
    let dist = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/user-state/distribution?days=3&bins=10",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (dist_status, _, _) = response_json(dist).await;
    assert_eq!(dist_status, StatusCode::OK);

    // compare：缺 versionA / versionB 必填查询参数 → 400
    let cmp_missing = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/compare",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (cmp_missing_status, _, _) = response_json(cmp_missing).await;
    assert_eq!(cmp_missing_status, StatusCode::BAD_REQUEST);

    // compare：带齐参数（即使 slice 为空也应 200）
    let cmp_ok = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/compare?versionA=h1&versionB=h2",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (cmp_ok_status, _, cmp_ok_body) = response_json(cmp_ok).await;
    assert_eq!(cmp_ok_status, StatusCode::OK);
    assert!(cmp_ok_body["data"].is_object());

    // 未授权访问指标端点
    let unauth = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/metrics/timeseries",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (unauth_status, _, _) = response_json(unauth).await;
    assert_eq!(unauth_status, StatusCode::UNAUTHORIZED);
}

// ─────────── suggestions / advisor 决策链 ───────────

#[tokio::test]
async fn it_suggestions_list_get_spend_and_explain() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;
    let user_token = login_and_get_token(&app.app).await;

    // 种入一条 pending 建议（合法白名单 patch）
    let id = app
        .state
        .store()
        .insert_amas_suggestion(&InsertSuggestion {
            based_on_version_hash: "abc123".into(),
            patch_json: r#"{"memoryModel.baseDesiredRetention":0.85}"#.into(),
            rationale: "测试建议".into(),
            evidence_json: "{}".into(),
            cost_usd: Some(0.02),
            tokens_input: Some(120),
            tokens_output: Some(60),
            confidence: Some(0.7),
            initial_status: SuggestionStatus::Pending,
            decided_by: None,
            decision_note: None,
            base_values_json: None,
        })
        .expect("insert suggestion");

    // list suggestions 全量
    let list = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/suggestions",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (list_status, _, list_body) = response_json(list).await;
    assert_eq!(list_status, StatusCode::OK);
    assert!(list_body["data"].as_array().map(|a| !a.is_empty()).unwrap_or(false));

    // list suggestions 带合法 status 过滤
    let list_pending = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/suggestions?status=pending&limit=5",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (list_pending_status, _, _) = response_json(list_pending).await;
    assert_eq!(list_pending_status, StatusCode::OK);

    // list suggestions 非法 status → BAD_STATUS 400
    let list_bad = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/suggestions?status=not-a-status",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (list_bad_status, _, list_bad_body) = response_json(list_bad).await;
    assert_eq!(list_bad_status, StatusCode::BAD_REQUEST);
    assert_eq!(list_bad_body["code"], "BAD_STATUS");

    // get suggestion 成功
    let get_s = request(
        &app.app,
        Method::GET,
        &format!("/api/admin/amas/suggestions/{id}"),
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (get_s_status, _, get_s_body) = response_json(get_s).await;
    assert_eq!(get_s_status, StatusCode::OK);
    assert_eq!(get_s_body["data"]["id"], id);

    // get suggestion 404
    let get_404 = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/suggestions/999999",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (get_404_status, _, _) = response_json(get_404).await;
    assert_eq!(get_404_status, StatusCode::NOT_FOUND);

    // spend 成功（聚合今日花费）
    let spend = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/suggestions/spend",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (spend_status, _, spend_body) = response_json(spend).await;
    assert_eq!(spend_status, StatusCode::OK);
    assert!(spend_body["data"]["dailyCapUsd"].is_number());
    assert!(spend_body["data"]["todayCostUsd"].as_f64().unwrap_or(-1.0) >= 0.0);

    // explain：LLM 未启用 → LLM_DISABLED 400
    let explain = request(
        &app.app,
        Method::POST,
        "/api/admin/amas/suggestions/explain",
        Some(serde_json::json!({
            "path": "memoryModel.baseDesiredRetention",
            "currentValue": 0.85
        })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (explain_status, _, explain_body) = response_json(explain).await;
    assert_eq!(explain_status, StatusCode::BAD_REQUEST);
    assert_eq!(explain_body["code"], "LLM_DISABLED");

    // suggestions 列表未授权
    let unauth = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/suggestions",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (unauth_status, _, _) = response_json(unauth).await;
    assert_eq!(unauth_status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn it_suggestion_approve_reject_branches() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let store = app.state.store();

    // 1) approve 不存在 → 404
    let approve_404 = request(
        &app.app,
        Method::POST,
        "/api/admin/amas/suggestions/888888/approve",
        Some(serde_json::json!({})),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (approve_404_status, _, _) = response_json(approve_404).await;
    assert_eq!(approve_404_status, StatusCode::NOT_FOUND);

    // 2) approve 非 pending（已 rejected）→ BAD_STATUS 400
    let rejected_id = store
        .insert_amas_suggestion(&InsertSuggestion {
            based_on_version_hash: "abc123".into(),
            patch_json: r#"{"memoryModel.baseDesiredRetention":0.85}"#.into(),
            rationale: "已拒绝".into(),
            evidence_json: "{}".into(),
            cost_usd: None,
            tokens_input: None,
            tokens_output: None,
            confidence: None,
            initial_status: SuggestionStatus::Rejected,
            decided_by: Some("seed".into()),
            decision_note: None,
            base_values_json: None,
        })
        .expect("insert rejected suggestion");
    let approve_bad_status_resp = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/amas/suggestions/{rejected_id}/approve"),
        Some(serde_json::json!({})),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (approve_bad_status, _, approve_bad_body) = response_json(approve_bad_status_resp).await;
    assert_eq!(approve_bad_status, StatusCode::BAD_REQUEST);
    assert_eq!(approve_bad_body["code"], "BAD_STATUS");

    // 3) approve patch 非法白名单路径 → PATCH_INVALID 400
    let bad_patch_id = store
        .insert_amas_suggestion(&InsertSuggestion {
            based_on_version_hash: "abc123".into(),
            patch_json: r#"{"ensemble.notWhitelisted":0.5}"#.into(),
            rationale: "非法 patch".into(),
            evidence_json: "{}".into(),
            cost_usd: None,
            tokens_input: None,
            tokens_output: None,
            confidence: None,
            initial_status: SuggestionStatus::Pending,
            decided_by: None,
            decision_note: None,
            base_values_json: None,
        })
        .expect("insert bad-patch suggestion");
    let approve_bad_patch = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/amas/suggestions/{bad_patch_id}/approve"),
        Some(serde_json::json!({})),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (approve_bad_patch_status, _, approve_bad_patch_body) =
        response_json(approve_bad_patch).await;
    assert_eq!(approve_bad_patch_status, StatusCode::BAD_REQUEST);
    assert_eq!(approve_bad_patch_body["code"], "PATCH_INVALID");

    // 4) approve 合法 pending（白名单 patch）→ 200 updated:true
    let good_id = store
        .insert_amas_suggestion(&InsertSuggestion {
            based_on_version_hash: "abc123".into(),
            patch_json: r#"{"memoryModel.baseDesiredRetention":0.85}"#.into(),
            rationale: "合法 patch".into(),
            evidence_json: "{}".into(),
            cost_usd: None,
            tokens_input: None,
            tokens_output: None,
            confidence: None,
            initial_status: SuggestionStatus::Pending,
            decided_by: None,
            decision_note: None,
            base_values_json: None,
        })
        .expect("insert good suggestion");
    let approve_ok = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/amas/suggestions/{good_id}/approve"),
        Some(serde_json::json!({ "note": "批准" })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (approve_ok_status, _, approve_ok_body) = response_json(approve_ok).await;
    assert_eq!(approve_ok_status, StatusCode::OK);
    assert_eq!(approve_ok_body["data"]["updated"], true);
    // 验证状态已落 approved
    let after = store.get_amas_suggestion(good_id).unwrap().unwrap();
    assert_eq!(after.status, SuggestionStatus::Approved);

    // 5) reject 成功
    let to_reject = store
        .insert_amas_suggestion(&InsertSuggestion {
            based_on_version_hash: "abc123".into(),
            patch_json: r#"{"memoryModel.baseDesiredRetention":0.85}"#.into(),
            rationale: "待拒绝".into(),
            evidence_json: "{}".into(),
            cost_usd: None,
            tokens_input: None,
            tokens_output: None,
            confidence: None,
            initial_status: SuggestionStatus::Pending,
            decided_by: None,
            decision_note: None,
            base_values_json: None,
        })
        .expect("insert to-reject suggestion");
    let reject = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/amas/suggestions/{to_reject}/reject"),
        Some(serde_json::json!({ "note": "不采纳" })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (reject_status, _, reject_body) = response_json(reject).await;
    assert_eq!(reject_status, StatusCode::OK);
    assert_eq!(reject_body["data"]["rejected"], true);
    let rejected_after = store.get_amas_suggestion(to_reject).unwrap().unwrap();
    assert_eq!(rejected_after.status, SuggestionStatus::Rejected);

    // 6) reject 不存在的 id → update_amas_suggestion_status 返回 StoreError::NotFound，
    //    经 From<StoreError> 映射为 500（仅 Validation 才转 400/404）
    let reject_missing = request(
        &app.app,
        Method::POST,
        "/api/admin/amas/suggestions/777777/reject",
        Some(serde_json::json!({})),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (reject_missing_status, _, _) = response_json(reject_missing).await;
    assert_eq!(reject_missing_status, StatusCode::INTERNAL_SERVER_ERROR);
}

// ─────────── process-event 幂等重放 + ELO 门控（校准事件零副作用） ───────────

/// 注册新用户并返回 (access_token, user_id)。login_and_get_token 不回传 id，
/// 本文件需要 user_id 直查 store 断言引擎状态。
async fn register_user_with_id(app: &axum::Router) -> (String, String) {
    let email = format!("user-{}@test.com", uuid::Uuid::new_v4());
    let username = format!("user-{}", uuid::Uuid::new_v4().simple());
    let resp = request(
        app,
        Method::POST,
        "/api/auth/register",
        Some(serde_json::json!({
            "email": email,
            "username": username,
            "password": "Passw0rd!",
        })),
        &[],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert!(status.is_success(), "register failed: {body}");
    (
        body["data"]["accessToken"].as_str().unwrap().to_string(),
        body["data"]["user"]["id"].as_str().unwrap().to_string(),
    )
}

/// 同 clientEventId 重发两次 /api/amas/process-event（验证 apply_elo 门控 + 幂等重放）：
/// 1) 第二次返回 duplicate_event 占位（重放态），且 sessionId 缺省对齐真实分支的
///    "{user_id}-session"（不返回空串）；
/// 2) mastery 状态不二次累加（两次调用后 mastery:{word} blob 相同）；
/// 3) 校准事件对 ELO 零副作用——apply_elo=false 门控下 user_elo/word_elo 全程不动。
///    若门控回退成早前的"有幂等键就写 ELO"，**第一次**调用就会把 games 推到 1，此断言立刻红。
#[tokio::test]
async fn it_process_event_idempotent_replay_and_zero_elo_side_effect() {
    let app = spawn_test_server().await;
    let (token, user_id) = register_user_with_id(&app.app).await;
    let hdr = [("authorization", auth_header(&token))];

    let word_id = "w-idem-http";
    let payload = serde_json::json!({
        "clientEventId": "evt-dup-1",
        "wordId": word_id,
        "isCorrect": true,
        "responseTime": 800,
    });

    // 首次：正常处理。
    let first = request(
        &app.app,
        Method::POST,
        "/api/amas/process-event",
        Some(payload.clone()),
        &hdr,
    )
    .await;
    let (s1, _, b1) = response_json(first).await;
    assert_eq!(s1, StatusCode::OK);
    assert_ne!(
        b1["data"]["explanation"]["primaryReason"], "duplicate_event",
        "首次处理不应命中重放态"
    );

    let store = app.state.store();
    let mastery_key = format!("mastery:{word_id}");
    let mastery_after_first = store.get_engine_algo_state(&user_id, &mastery_key).unwrap();
    assert!(mastery_after_first.is_some(), "首次处理应写入 mastery 状态");
    // 校准事件零 ELO 副作用：即便携带幂等键，全局 word_elo / user_elo 也不得被推进。
    assert_eq!(store.get_user_elo(&user_id).unwrap().games, 0);
    assert_eq!(store.get_word_elo(word_id).unwrap().games, 0);

    // 重发同 clientEventId：重放态占位，状态零变更。
    let second = request(
        &app.app,
        Method::POST,
        "/api/amas/process-event",
        Some(payload),
        &hdr,
    )
    .await;
    let (s2, _, b2) = response_json(second).await;
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(b2["data"]["explanation"]["primaryReason"], "duplicate_event");
    assert_eq!(
        b2["data"]["sessionId"],
        serde_json::json!(format!("{user_id}-session")),
        "重放态 sessionId 缺省必须对齐真实分支，不得为空串"
    );

    let mastery_after_second = store.get_engine_algo_state(&user_id, &mastery_key).unwrap();
    assert_eq!(
        mastery_after_first, mastery_after_second,
        "重放不得二次累加 mastery"
    );
    assert_eq!(store.get_user_elo(&user_id).unwrap().games, 0);
    assert_eq!(store.get_word_elo(word_id).unwrap().games, 0);
}

/// clientEventId 入参校验（对齐 records/single.rs 口径）：
/// 空白串 trim 后视为 None（旧非幂等路径，正常 200 且不落 processed_events）；
/// 超长（>128）→ 400 AMAS_INVALID_EVENT_ID。
#[tokio::test]
async fn it_process_event_client_event_id_normalization() {
    let app = spawn_test_server().await;
    let (token, user_id) = register_user_with_id(&app.app).await;
    let hdr = [("authorization", auth_header(&token))];

    // 空白串：视为未携带幂等键。
    let blank = request(
        &app.app,
        Method::POST,
        "/api/amas/process-event",
        Some(serde_json::json!({
            "clientEventId": "   ",
            "wordId": "w-blank-id",
            "isCorrect": true,
            "responseTime": 500,
        })),
        &hdr,
    )
    .await;
    let (blank_status, _, blank_body) = response_json(blank).await;
    assert_eq!(blank_status, StatusCode::OK, "空白幂等键应走旧非幂等路径: {blank_body}");
    // trim 后的空串不得作为幂等键落账（validate_id 也会拒绝空串，此处防御性确认无标记）。
    assert!(!app.state.store().is_event_processed(&user_id, "   ").unwrap_or(false));

    // 超长：400。
    let long_id = "x".repeat(129);
    let too_long = request(
        &app.app,
        Method::POST,
        "/api/amas/process-event",
        Some(serde_json::json!({
            "clientEventId": long_id,
            "wordId": "w-long-id",
            "isCorrect": true,
            "responseTime": 500,
        })),
        &hdr,
    )
    .await;
    let (long_status, _, long_body) = response_json(too_long).await;
    assert_eq!(long_status, StatusCode::BAD_REQUEST);
    assert_eq!(long_body["code"], "AMAS_INVALID_EVENT_ID");
}