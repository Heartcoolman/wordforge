mod common;

use axum::http::{Method, StatusCode};
use chrono::Utc;

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token};
use common::http::{request, response_json};

async fn create_word(app: &axum::Router, token: &str, text: &str) -> String {
    let response = request(
        app,
        Method::POST,
        "/api/words",
        Some(serde_json::json!({
            "text": text,
            "meaning": format!("meaning-{text}"),
            "difficulty": 0.4,
        })),
        &[("authorization", auth_header(token))],
    )
    .await;

    let (status, _, body) = response_json(response).await;
    assert_eq!(status, StatusCode::CREATED);
    body["data"]["id"].as_str().expect("word id").to_string()
}

async fn current_user_info(app: &axum::Router, token: &str) -> (String, String) {
    let response = request(
        app,
        Method::GET,
        "/api/users/me",
        None,
        &[("authorization", auth_header(token))],
    )
    .await;

    let (status, _, body) = response_json(response).await;
    assert_eq!(status, StatusCode::OK);

    (
        body["data"]["id"].as_str().expect("user id").to_string(),
        body["data"]["email"]
            .as_str()
            .expect("user email")
            .to_string(),
    )
}

#[tokio::test]
async fn it_admin_auth_and_management_routes() {
    let app = spawn_test_server().await;

    let status_before = request(&app.app, Method::GET, "/api/admin/auth/status", None, &[]).await;
    let (status_before_code, _, status_before_body) = response_json(status_before).await;
    assert_eq!(status_before_code, StatusCode::OK);
    assert_eq!(status_before_body["data"]["initialized"], false);

    let invalid_email_setup = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/setup",
        Some(serde_json::json!({
            "email": "not-an-email",
            "password": "AdminPassw0rd!"
        })),
        &[],
    )
    .await;
    let (invalid_email_status, _, _) = response_json(invalid_email_setup).await;
    assert_eq!(invalid_email_status, StatusCode::BAD_REQUEST);

    let weak_password_setup = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/setup",
        Some(serde_json::json!({
            "email": "admin@test.com",
            "password": "123"
        })),
        &[],
    )
    .await;
    let (weak_password_status, _, _) = response_json(weak_password_setup).await;
    assert_eq!(weak_password_status, StatusCode::BAD_REQUEST);

    let admin_email = format!("admin-{}@test.com", uuid::Uuid::new_v4());
    let admin_password = "AdminPassw0rd!";

    let setup_ok = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/setup",
        Some(serde_json::json!({
            "email": admin_email,
            "password": admin_password
        })),
        &[],
    )
    .await;
    let (setup_status, _, setup_body) = response_json(setup_ok).await;
    assert_eq!(setup_status, StatusCode::CREATED);
    let admin_token = setup_body["data"]["token"]
        .as_str()
        .expect("admin token")
        .to_string();

    let status_after = request(&app.app, Method::GET, "/api/admin/auth/status", None, &[]).await;
    let (status_after_code, _, status_after_body) = response_json(status_after).await;
    assert_eq!(status_after_code, StatusCode::OK);
    assert_eq!(status_after_body["data"]["initialized"], true);

    let setup_again = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/setup",
        Some(serde_json::json!({
            "email": "another-admin@test.com",
            "password": admin_password
        })),
        &[],
    )
    .await;
    let (setup_again_status, _, _) = response_json(setup_again).await;
    assert_eq!(setup_again_status, StatusCode::CONFLICT);

    let login_wrong = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/login",
        Some(serde_json::json!({
            "email": setup_body["data"]["admin"]["email"],
            "password": "WrongPass!"
        })),
        &[],
    )
    .await;
    let (login_wrong_status, _, _) = response_json(login_wrong).await;
    assert_eq!(login_wrong_status, StatusCode::UNAUTHORIZED);

    let login_ok = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/login",
        Some(serde_json::json!({
            "email": setup_body["data"]["admin"]["email"],
            "password": admin_password
        })),
        &[],
    )
    .await;
    let (login_ok_status, _, login_ok_body) = response_json(login_ok).await;
    assert_eq!(login_ok_status, StatusCode::OK);
    let admin_login_token = login_ok_body["data"]["token"]
        .as_str()
        .expect("admin login token")
        .to_string();

    let user_token = login_and_get_token(&app.app).await;
    let (user_id, user_email) = current_user_info(&app.app, &user_token).await;
    let _word_id = create_word(&app.app, &admin_login_token, "admin-route-word").await;

    let unauthorized_admin = request(
        &app.app,
        Method::GET,
        "/api/admin/users",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (unauthorized_status, _, _) = response_json(unauthorized_admin).await;
    assert_eq!(unauthorized_status, StatusCode::UNAUTHORIZED);

    let list_users = request(
        &app.app,
        Method::GET,
        "/api/admin/users",
        None,
        &[("authorization", auth_header(&admin_login_token))],
    )
    .await;
    let (list_users_status, _, list_users_body) = response_json(list_users).await;
    assert_eq!(list_users_status, StatusCode::OK);
    assert!(list_users_body["data"]["data"].is_array());

    let ban_user = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/users/{user_id}/ban"),
        None,
        &[("authorization", auth_header(&admin_login_token))],
    )
    .await;
    let (ban_status, _, ban_body) = response_json(ban_user).await;
    assert_eq!(ban_status, StatusCode::OK);
    assert_eq!(ban_body["data"]["banned"], true);

    let banned_login = request(
        &app.app,
        Method::POST,
        "/api/auth/login",
        Some(serde_json::json!({
            "email": user_email,
            "password": "Passw0rd!"
        })),
        &[],
    )
    .await;
    let (banned_login_status, _, _) = response_json(banned_login).await;
    assert_eq!(banned_login_status, StatusCode::FORBIDDEN);

    let unban_user = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/users/{user_id}/unban"),
        None,
        &[("authorization", auth_header(&admin_login_token))],
    )
    .await;
    let (unban_status, _, unban_body) = response_json(unban_user).await;
    assert_eq!(unban_status, StatusCode::OK);
    assert_eq!(unban_body["data"]["banned"], false);

    // ban 时用户会话被撤销，unban 后需要重新登录获取新 token
    let relogin = request(
        &app.app,
        Method::POST,
        "/api/auth/login",
        Some(serde_json::json!({
            "email": user_email,
            "password": "Passw0rd!"
        })),
        &[],
    )
    .await;
    let (relogin_status, _, relogin_body) = response_json(relogin).await;
    assert_eq!(relogin_status, StatusCode::OK);
    let user_token = relogin_body["data"]["accessToken"]
        .as_str()
        .expect("re-login access token")
        .to_string();

    for path in [
        "/api/admin/stats",
        "/api/admin/analytics/engagement",
        "/api/admin/analytics/learning",
        "/api/admin/monitoring/health",
        "/api/admin/monitoring/database",
    ] {
        let response = request(
            &app.app,
            Method::GET,
            path,
            None,
            &[("authorization", auth_header(&admin_login_token))],
        )
        .await;
        let (status, _, _) = response_json(response).await;
        assert_eq!(status, StatusCode::OK, "path: {path}");
    }

    let settings_get = request(
        &app.app,
        Method::GET,
        "/api/admin/settings",
        None,
        &[("authorization", auth_header(&admin_login_token))],
    )
    .await;
    let (settings_get_status, _, settings_get_body) = response_json(settings_get).await;
    assert_eq!(settings_get_status, StatusCode::OK);
    assert!(settings_get_body["data"]["maxUsers"].is_number());

    let settings_update = request(
        &app.app,
        Method::PUT,
        "/api/admin/settings",
        Some(serde_json::json!({
            "maxUsers": 54321,
            "registrationEnabled": true,
            "maintenanceMode": false,
            "defaultDailyWords": 33
        })),
        &[("authorization", auth_header(&admin_login_token))],
    )
    .await;
    let (settings_update_status, _, settings_update_body) = response_json(settings_update).await;
    assert_eq!(settings_update_status, StatusCode::OK);
    assert_eq!(settings_update_body["data"]["maxUsers"], 54321);

    let broadcast = request(
        &app.app,
        Method::POST,
        "/api/admin/broadcast",
        Some(serde_json::json!({
            "title": "System Notice",
            "message": "Maintenance tonight"
        })),
        &[("authorization", auth_header(&admin_login_token))],
    )
    .await;
    let (broadcast_status, _, broadcast_body) = response_json(broadcast).await;
    assert_eq!(broadcast_status, StatusCode::OK);
    assert!(broadcast_body["data"]["sent"].as_u64().unwrap_or(0) >= 1);

    let notifications = request(
        &app.app,
        Method::GET,
        "/api/notifications?limit=50",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (notifications_status, _, notifications_body) = response_json(notifications).await;
    assert_eq!(notifications_status, StatusCode::OK);
    assert!(
        !notifications_body["data"]
            .as_array()
            .unwrap_or(&Vec::new())
            .is_empty()
    );

    let logout = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/logout",
        None,
        &[("authorization", auth_header(&admin_login_token))],
    )
    .await;
    let (logout_status, _, logout_body) = response_json(logout).await;
    assert_eq!(logout_status, StatusCode::OK);
    assert_eq!(logout_body["data"]["loggedOut"], true);

    let after_logout = request(
        &app.app,
        Method::GET,
        "/api/admin/stats",
        None,
        &[("authorization", auth_header(&admin_login_token))],
    )
    .await;
    let (after_logout_status, _, _) = response_json(after_logout).await;
    assert_eq!(after_logout_status, StatusCode::UNAUTHORIZED);

    let setup_token_still_works = request(
        &app.app,
        Method::GET,
        "/api/admin/stats",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (setup_token_status, _, _) = response_json(setup_token_still_works).await;
    assert_eq!(setup_token_status, StatusCode::OK);
}

#[tokio::test]
async fn it_amas_user_and_admin_endpoints() {
    let app = spawn_test_server().await;
    let user_token = login_and_get_token(&app.app).await;

    let admin_email = format!("amas-admin-{}@test.com", uuid::Uuid::new_v4());
    let admin_password = "AdminPassw0rd!";
    let setup_admin = request(
        &app.app,
        Method::POST,
        "/api/admin/auth/setup",
        Some(serde_json::json!({
            "email": admin_email,
            "password": admin_password
        })),
        &[],
    )
    .await;
    let (setup_admin_status, _, setup_admin_body) = response_json(setup_admin).await;
    assert_eq!(setup_admin_status, StatusCode::CREATED);
    let admin_token = setup_admin_body["data"]["token"]
        .as_str()
        .expect("admin token")
        .to_string();

    let word_id = create_word(&app.app, &admin_token, "amas-coverage-word").await;

    let process_event = request(
        &app.app,
        Method::POST,
        "/api/amas/process-event",
        Some(serde_json::json!({
            "wordId": word_id,
            "isCorrect": true,
            "responseTime": 900,
            "sessionId": "amas-s-1"
        })),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (process_status, _, process_body) = response_json(process_event).await;
    assert_eq!(process_status, StatusCode::OK);
    assert!(process_body["data"]["strategy"].is_object());

    let too_large_events: Vec<serde_json::Value> = (0..501)
        .map(|idx| {
            serde_json::json!({
                "wordId": format!("w-{idx}"),
                "isCorrect": true,
                "responseTime": 500,
                "sessionId": "too-large"
            })
        })
        .collect();
    let batch_too_large = request(
        &app.app,
        Method::POST,
        "/api/amas/batch-process",
        Some(serde_json::json!({ "events": too_large_events })),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (batch_too_large_status, _, _) = response_json(batch_too_large).await;
    assert_eq!(batch_too_large_status, StatusCode::BAD_REQUEST);

    let batch_ok = request(
        &app.app,
        Method::POST,
        "/api/amas/batch-process",
        Some(serde_json::json!({
            "events": [
                {
                    "wordId": "amas-b-1",
                    "isCorrect": false,
                    "responseTime": 1400,
                    "sessionId": "batch-1"
                },
                {
                    "wordId": "amas-b-2",
                    "isCorrect": true,
                    "responseTime": 800,
                    "sessionId": "batch-1",
                    "hintUsed": true
                }
            ]
        })),
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (batch_ok_status, _, batch_ok_body) = response_json(batch_ok).await;
    assert_eq!(batch_ok_status, StatusCode::OK);
    assert_eq!(batch_ok_body["data"]["count"], 2);

    for path in [
        "/api/amas/state",
        "/api/amas/strategy",
        "/api/amas/phase",
        "/api/amas/learning-curve",
        "/api/amas/intervention",
        "/api/amas/mastery/evaluate?wordId=missing-word",
    ] {
        let response = request(
            &app.app,
            Method::GET,
            path,
            None,
            &[("authorization", auth_header(&user_token))],
        )
        .await;
        let (status, _, _) = response_json(response).await;
        assert_eq!(status, StatusCode::OK, "path: {path}");
    }

    let reset = request(
        &app.app,
        Method::POST,
        "/api/amas/reset",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (reset_status, _, reset_body) = response_json(reset).await;
    assert_eq!(reset_status, StatusCode::OK);
    assert_eq!(reset_body["data"]["reset"], true);

    let config_get = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/config",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (config_get_status, _, config_get_body) = response_json(config_get).await;
    assert_eq!(config_get_status, StatusCode::OK);

    let mut invalid_config = config_get_body["data"].clone();
    invalid_config["monitoring"]["sampleRate"] = serde_json::json!(2.0);
    let config_put_invalid = request(
        &app.app,
        Method::PUT,
        "/api/admin/amas/config",
        Some(invalid_config),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (config_put_invalid_status, _, _) = response_json(config_put_invalid).await;
    assert_eq!(config_put_invalid_status, StatusCode::BAD_REQUEST);

    let mut valid_config = config_get_body["data"].clone();
    valid_config["monitoring"]["sample_rate"] = serde_json::json!(0.2);
    let config_put_valid = request(
        &app.app,
        Method::PUT,
        "/api/admin/amas/config",
        Some(valid_config),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (config_put_valid_status, _, config_put_valid_body) = response_json(config_put_valid).await;
    assert_eq!(config_put_valid_status, StatusCode::OK);
    assert_eq!(config_put_valid_body["data"]["updated"], true);

    let admin_metrics = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/metrics",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (metrics_status, _, metrics_body) = response_json(admin_metrics).await;
    assert_eq!(metrics_status, StatusCode::OK);
    assert!(metrics_body["data"].is_object());

    app.state
        .store()
        .insert_monitoring_event(&serde_json::json!({
            "id": "admin-amas-event-1",
            "timestamp": Utc::now().to_rfc3339(),
            "kind": "test"
        }))
        .expect("insert monitoring event");

    let admin_monitoring = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/monitoring?limit=1",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (monitoring_status, _, monitoring_body) = response_json(admin_monitoring).await;
    assert_eq!(monitoring_status, StatusCode::OK);
    assert!(monitoring_body["data"].is_array());

    let config_as_user = request(
        &app.app,
        Method::GET,
        "/api/admin/amas/config",
        None,
        &[("authorization", auth_header(&user_token))],
    )
    .await;
    let (config_as_user_status, _, _) = response_json(config_as_user).await;
    assert_eq!(config_as_user_status, StatusCode::UNAUTHORIZED);
}
