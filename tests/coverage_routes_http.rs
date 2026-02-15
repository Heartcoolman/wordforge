mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::response::Response;
use axum::Router;
use chrono::{Duration, Utc};
use learning_backend::store::keys;
use tower::util::ServiceExt;

use common::app::spawn_test_server;
use common::auth::{auth_header, login_and_get_token, setup_admin_and_get_token};
use common::http::{request, response_json};

async fn request_raw(
    app: &Router,
    method: Method,
    path: &str,
    body: Vec<u8>,
    headers: &[(&str, String)],
) -> Response {
    let mut builder = Request::builder().method(method).uri(path);
    for (key, value) in headers {
        builder = builder.header(*key, value.as_str());
    }

    let req = builder.body(Body::from(body)).expect("raw request");
    app.clone().oneshot(req).await.expect("raw oneshot")
}

async fn create_word(app: &Router, token: &str, text: &str, meaning: &str) -> String {
    let response = request(
        app,
        Method::POST,
        "/api/words",
        Some(serde_json::json!({
            "text": text,
            "meaning": meaning,
            "difficulty": 0.45,
            "tags": ["coverage", "route"]
        })),
        &[("authorization", auth_header(token))],
    )
    .await;

    let (status, _, body) = response_json(response).await;
    assert!(status.is_success(), "create_word failed: {body}");
    body["data"]["id"].as_str().expect("word id").to_string()
}

async fn current_user_id(app: &Router, token: &str) -> String {
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
    body["data"]["id"].as_str().expect("user id").to_string()
}

#[tokio::test]
async fn it_learning_wordbooks_word_states_and_study_config_flow() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let word_id_1 = create_word(&app.app, &admin_token, "alpha", "阿尔法").await;
    let word_id_2 = create_word(&app.app, &admin_token, "beta", "贝塔").await;
    let word_id_3 = create_word(&app.app, &admin_token, "gamma", "伽马").await;

    let create_wordbook = request(
        &app.app,
        Method::POST,
        "/api/wordbooks",
        Some(serde_json::json!({
            "name": "coverage-book",
            "description": "for integration coverage"
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (create_status, _, create_body) = response_json(create_wordbook).await;
    assert_eq!(create_status, StatusCode::CREATED);
    let wordbook_id = create_body["data"]["id"]
        .as_str()
        .expect("wordbook id")
        .to_string();

    let list_user_books = request(
        &app.app,
        Method::GET,
        "/api/wordbooks/user",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (list_user_status, _, list_user_body) = response_json(list_user_books).await;
    assert_eq!(list_user_status, StatusCode::OK);
    assert!(
        list_user_body["data"]
            .as_array()
            .unwrap_or(&Vec::new())
            .len()
            >= 1
    );

    let add_words = request(
        &app.app,
        Method::POST,
        &format!("/api/wordbooks/{wordbook_id}/words"),
        Some(serde_json::json!({
            "wordIds": [word_id_1.clone(), word_id_2.clone(), word_id_3.clone()]
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (add_status, _, add_body) = response_json(add_words).await;
    assert_eq!(add_status, StatusCode::OK);
    assert_eq!(add_body["data"]["added"], 3);

    let list_book_words = request(
        &app.app,
        Method::GET,
        &format!("/api/wordbooks/{wordbook_id}/words?page=1&per_page=50"),
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (list_words_status, _, list_words_body) = response_json(list_book_words).await;
    assert_eq!(list_words_status, StatusCode::OK);
    assert!(list_words_body["data"]["data"].is_array());
    assert!(list_words_body["data"]["total"].as_u64().unwrap_or(0) >= 3);

    let another_token = login_and_get_token(&app.app).await;
    let forbidden_list = request(
        &app.app,
        Method::GET,
        &format!("/api/wordbooks/{wordbook_id}/words"),
        None,
        &[("authorization", auth_header(&another_token))],
    )
    .await;
    let (forbidden_status, _, _) = response_json(forbidden_list).await;
    assert_eq!(forbidden_status, StatusCode::FORBIDDEN);

    let update_study_config = request(
        &app.app,
        Method::PUT,
        "/api/study-config",
        Some(serde_json::json!({
            "selectedWordbookIds": [wordbook_id],
            "dailyWordCount": 7,
            "dailyMasteryTarget": 4,
            "studyMode": "review"
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (config_status, _, config_body) = response_json(update_study_config).await;
    assert_eq!(config_status, StatusCode::OK);
    assert_eq!(config_body["data"]["dailyWordCount"], 7);

    let get_today_words = request(
        &app.app,
        Method::GET,
        "/api/study-config/today-words",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (today_status, _, today_body) = response_json(get_today_words).await;
    assert_eq!(today_status, StatusCode::OK);
    assert!(today_body["data"]["words"].is_array());

    let get_progress = request(
        &app.app,
        Method::GET,
        "/api/study-config/progress",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (progress_status, _, progress_body) = response_json(get_progress).await;
    assert_eq!(progress_status, StatusCode::OK);
    assert!(progress_body["data"]["target"].is_number());

    let create_session = request(
        &app.app,
        Method::POST,
        "/api/learning/session",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (session_status, _, session_body) = response_json(create_session).await;
    assert_eq!(session_status, StatusCode::OK);
    assert_eq!(session_body["data"]["resumed"], false);
    let session_id = session_body["data"]["sessionId"]
        .as_str()
        .expect("session id")
        .to_string();

    let resume_session = request(
        &app.app,
        Method::POST,
        "/api/learning/session",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (resume_status, _, resume_body) = response_json(resume_session).await;
    assert_eq!(resume_status, StatusCode::OK);
    assert_eq!(resume_body["data"]["resumed"], true);

    let study_words = request(
        &app.app,
        Method::GET,
        "/api/learning/study-words",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (study_status, _, study_body) = response_json(study_words).await;
    assert_eq!(study_status, StatusCode::OK);
    assert!(study_body["data"]["words"].is_array());
    assert!(study_body["data"]["strategy"].is_object());

    let next_words = request(
        &app.app,
        Method::POST,
        "/api/learning/next-words",
        Some(serde_json::json!({
            "excludeWordIds": [word_id_1.clone()],
            "masteredWordIds": [word_id_2.clone()]
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (next_status, _, next_body) = response_json(next_words).await;
    assert_eq!(next_status, StatusCode::OK);
    assert!(next_body["data"]["words"].is_array());

    let adjust_words = request(
        &app.app,
        Method::POST,
        "/api/learning/adjust-words",
        Some(serde_json::json!({
            "userState": "normal",
            "recentPerformance": 0.82
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (adjust_status, _, adjust_body) = response_json(adjust_words).await;
    assert_eq!(adjust_status, StatusCode::OK);
    assert!(adjust_body["data"]["adjustedStrategy"].is_object());

    let sync_progress = request(
        &app.app,
        Method::POST,
        "/api/learning/sync-progress",
        Some(serde_json::json!({
            "sessionId": session_id,
            "totalQuestions": 5,
            "contextShifts": 2
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (sync_status, _, sync_body) = response_json(sync_progress).await;
    assert_eq!(sync_status, StatusCode::OK);
    assert_eq!(sync_body["data"]["totalQuestions"], 5);

    let forbidden_sync = request(
        &app.app,
        Method::POST,
        "/api/learning/sync-progress",
        Some(serde_json::json!({
            "sessionId": sync_body["data"]["id"].as_str().unwrap_or(""),
            "totalQuestions": 9
        })),
        &[("authorization", auth_header(&another_token))],
    )
    .await;
    let (forbidden_sync_status, _, _) = response_json(forbidden_sync).await;
    assert_eq!(forbidden_sync_status, StatusCode::FORBIDDEN);

    let missing_sync = request(
        &app.app,
        Method::POST,
        "/api/learning/sync-progress",
        Some(serde_json::json!({
            "sessionId": "missing-session",
            "totalQuestions": 1
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (missing_sync_status, _, _) = response_json(missing_sync).await;
    assert_eq!(missing_sync_status, StatusCode::NOT_FOUND);

    let get_default_state = request(
        &app.app,
        Method::GET,
        &format!("/api/word-states/{word_id_1}"),
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (state_status, _, _state_body) = response_json(get_default_state).await;
    assert_eq!(state_status, StatusCode::NOT_FOUND);

    let mark_mastered = request(
        &app.app,
        Method::POST,
        &format!("/api/word-states/{word_id_1}/mark-mastered"),
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (mastered_status, _, mastered_body) = response_json(mark_mastered).await;
    assert_eq!(mastered_status, StatusCode::OK);
    assert_eq!(mastered_body["data"]["state"], "MASTERED");

    let reset_word = request(
        &app.app,
        Method::POST,
        &format!("/api/word-states/{word_id_1}/reset"),
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (reset_status, _, reset_body) = response_json(reset_word).await;
    assert_eq!(reset_status, StatusCode::OK);
    assert_eq!(reset_body["data"]["state"], "NEW");

    let too_large_query_ids: Vec<String> = (0..501).map(|idx| format!("w-{idx}")).collect();
    let too_large_batch_query = request(
        &app.app,
        Method::POST,
        "/api/word-states/batch",
        Some(serde_json::json!({ "wordIds": too_large_query_ids })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (too_large_query_status, _, _) = response_json(too_large_batch_query).await;
    assert_eq!(too_large_query_status, StatusCode::BAD_REQUEST);

    let valid_batch_query = request(
        &app.app,
        Method::POST,
        "/api/word-states/batch",
        Some(serde_json::json!({ "wordIds": [word_id_1.clone(), word_id_2.clone()] })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (batch_query_status, _, batch_query_body) = response_json(valid_batch_query).await;
    assert_eq!(batch_query_status, StatusCode::OK);
    assert!(batch_query_body["data"].is_array());

    let too_large_updates: Vec<serde_json::Value> = (0..501)
        .map(|idx| serde_json::json!({ "wordId": format!("word-{idx}") }))
        .collect();
    let too_large_batch_update = request(
        &app.app,
        Method::POST,
        "/api/word-states/batch-update",
        Some(serde_json::json!({ "updates": too_large_updates })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (too_large_update_status, _, _) = response_json(too_large_batch_update).await;
    assert_eq!(too_large_update_status, StatusCode::BAD_REQUEST);

    let valid_batch_update = request(
        &app.app,
        Method::POST,
        "/api/word-states/batch-update",
        Some(serde_json::json!({
            "updates": [
                { "wordId": word_id_1, "state": "LEARNING", "masteryLevel": 2.0 },
                { "wordId": word_id_2, "state": "REVIEWING", "masteryLevel": -1.0 }
            ]
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (update_status, _, update_body) = response_json(valid_batch_update).await;
    assert_eq!(update_status, StatusCode::OK);
    assert_eq!(update_body["data"]["updated"], 2);

    let due_list = request(
        &app.app,
        Method::GET,
        "/api/word-states/due/list?limit=10",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (due_status, _, due_body) = response_json(due_list).await;
    assert_eq!(due_status, StatusCode::OK);
    assert!(due_body["data"].is_array());

    let stats_overview = request(
        &app.app,
        Method::GET,
        "/api/word-states/stats/overview",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (stats_status, _, stats_body) = response_json(stats_overview).await;
    assert_eq!(stats_status, StatusCode::OK);
    assert!(stats_body["data"].is_object());

    let remove_word = request(
        &app.app,
        Method::DELETE,
        &format!(
            "/api/wordbooks/{}/words/{}",
            create_body["data"]["id"].as_str().unwrap_or_default(),
            word_id_3
        ),
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (remove_status, _, remove_body) = response_json(remove_word).await;
    assert_eq!(remove_status, StatusCode::OK);
    assert_eq!(remove_body["data"]["removed"], true);

    let system_books = request(
        &app.app,
        Method::GET,
        "/api/wordbooks/system",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (system_status, _, _) = response_json(system_books).await;
    assert_eq!(system_status, StatusCode::OK);
}

#[tokio::test]
async fn it_user_profile_notifications_content_and_v1_flow() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    let user_id = current_user_id(&app.app, &token).await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let word_id = create_word(&app.app, &admin_token, "coverage-word", "覆盖测试词").await;

    let reward_default = request(
        &app.app,
        Method::GET,
        "/api/user-profile/reward",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (reward_default_status, _, _) = response_json(reward_default).await;
    assert_eq!(reward_default_status, StatusCode::OK);

    let reward_invalid = request(
        &app.app,
        Method::PUT,
        "/api/user-profile/reward",
        Some(serde_json::json!({ "rewardType": "nope" })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (reward_invalid_status, _, _) = response_json(reward_invalid).await;
    assert_eq!(reward_invalid_status, StatusCode::BAD_REQUEST);

    let reward_valid = request(
        &app.app,
        Method::PUT,
        "/api/user-profile/reward",
        Some(serde_json::json!({ "rewardType": "explorer" })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (reward_valid_status, _, reward_valid_body) = response_json(reward_valid).await;
    assert_eq!(reward_valid_status, StatusCode::OK);
    assert_eq!(reward_valid_body["data"]["rewardType"], "explorer");

    for path in [
        "/api/user-profile/cognitive",
        "/api/user-profile/learning-style",
        "/api/user-profile/chronotype",
        "/api/user-profile/habit",
    ] {
        let response = request(
            &app.app,
            Method::GET,
            path,
            None,
            &[("authorization", auth_header(&token))],
        )
        .await;
        let (status, _, _) = response_json(response).await;
        assert_eq!(status, StatusCode::OK, "path: {path}");
    }

    let invalid_hours = request(
        &app.app,
        Method::POST,
        "/api/user-profile/habit",
        Some(serde_json::json!({ "preferredHours": [6, 25] })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (invalid_hours_status, _, _) = response_json(invalid_hours).await;
    assert_eq!(invalid_hours_status, StatusCode::BAD_REQUEST);

    let invalid_sessions_per_day = request(
        &app.app,
        Method::POST,
        "/api/user-profile/habit",
        Some(serde_json::json!({ "sessionsPerDay": 0 })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (invalid_spd_status, _, _) = response_json(invalid_sessions_per_day).await;
    assert_eq!(invalid_spd_status, StatusCode::BAD_REQUEST);

    let invalid_session_length = request(
        &app.app,
        Method::POST,
        "/api/user-profile/habit",
        Some(serde_json::json!({ "medianSessionLengthMins": 999 })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (invalid_length_status, _, _) = response_json(invalid_session_length).await;
    assert_eq!(invalid_length_status, StatusCode::BAD_REQUEST);

    let habit_valid = request(
        &app.app,
        Method::POST,
        "/api/user-profile/habit",
        Some(serde_json::json!({
            "preferredHours": [8, 13, 19],
            "sessionsPerDay": 3,
            "medianSessionLengthMins": 22
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (habit_valid_status, _, habit_valid_body) = response_json(habit_valid).await;
    assert_eq!(habit_valid_status, StatusCode::OK);
    assert_eq!(habit_valid_body["data"]["sessionsPerDay"], 3.0);

    let avatar_empty = request_raw(
        &app.app,
        Method::POST,
        "/api/user-profile/avatar",
        Vec::new(),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (avatar_empty_status, _, _) = response_json(avatar_empty).await;
    assert_eq!(avatar_empty_status, StatusCode::BAD_REQUEST);

    // 最小的有效PNG文件（1x1像素，透明）
    let minimal_png = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG signature
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
        0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
        0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41,
        0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
        0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
        0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
        0x42, 0x60, 0x82
    ];
    let avatar_ok = request_raw(
        &app.app,
        Method::POST,
        "/api/user-profile/avatar",
        minimal_png,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (avatar_ok_status, _, avatar_ok_body) = response_json(avatar_ok).await;
    assert_eq!(avatar_ok_status, StatusCode::OK);
    assert!(avatar_ok_body["data"]["avatarUrl"].is_string());

    let notification_1 = serde_json::json!({
        "id": "n-1",
        "userId": user_id,
        "type": "broadcast",
        "title": "hello",
        "message": "world",
        "read": false,
        "createdAt": (Utc::now() - Duration::minutes(5)).to_rfc3339(),
    });
    let notification_2 = serde_json::json!({
        "id": "n-2",
        "userId": current_user_id(&app.app, &token).await,
        "type": "reward",
        "title": "badge",
        "message": "earned",
        "read": true,
        "createdAt": Utc::now().to_rfc3339(),
    });
    app.state
        .store()
        .notifications
        .insert(
            keys::notification_key(&current_user_id(&app.app, &token).await, "n-1")
                .unwrap()
                .as_bytes(),
            serde_json::to_vec(&notification_1).expect("notification 1 bytes"),
        )
        .expect("insert notification 1");
    app.state
        .store()
        .notifications
        .insert(
            keys::notification_key(&current_user_id(&app.app, &token).await, "n-2")
                .unwrap()
                .as_bytes(),
            serde_json::to_vec(&notification_2).expect("notification 2 bytes"),
        )
        .expect("insert notification 2");

    let badge = serde_json::json!({
        "id": "first_word",
        "name": "First Word",
        "description": "Learn first word",
        "unlocked": true,
        "progress": 1.0,
        "unlockedAt": Utc::now().to_rfc3339(),
    });
    app.state
        .store()
        .badges
        .insert(
            keys::badge_key(&current_user_id(&app.app, &token).await, "first_word")
                .unwrap()
                .as_bytes(),
            serde_json::to_vec(&badge).expect("badge bytes"),
        )
        .expect("insert badge");

    let list_notifications = request(
        &app.app,
        Method::GET,
        "/api/notifications?limit=20&unreadOnly=true",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (list_notifications_status, _, list_notifications_body) =
        response_json(list_notifications).await;
    assert_eq!(list_notifications_status, StatusCode::OK);
    assert!(list_notifications_body["data"].is_array());

    let mark_read = request(
        &app.app,
        Method::PUT,
        "/api/notifications/n-1/read",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (mark_read_status, _, _) = response_json(mark_read).await;
    assert_eq!(mark_read_status, StatusCode::OK);

    let mark_all_read = request(
        &app.app,
        Method::POST,
        "/api/notifications/read-all",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (mark_all_status, _, mark_all_body) = response_json(mark_all_read).await;
    assert_eq!(mark_all_status, StatusCode::OK);
    assert!(mark_all_body["data"]["markedRead"].is_number());

    let list_badges = request(
        &app.app,
        Method::GET,
        "/api/notifications/badges",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (badges_status, _, badges_body) = response_json(list_badges).await;
    assert_eq!(badges_status, StatusCode::OK);
    assert!(badges_body["data"].is_array());

    let prefs_default = request(
        &app.app,
        Method::GET,
        "/api/notifications/preferences",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (prefs_default_status, _, _) = response_json(prefs_default).await;
    assert_eq!(prefs_default_status, StatusCode::OK);

    let prefs_set = request(
        &app.app,
        Method::PUT,
        "/api/notifications/preferences",
        Some(serde_json::json!({
            "theme": "dark",
            "language": "zh",
            "notificationEnabled": true,
            "soundEnabled": false
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (prefs_set_status, _, prefs_set_body) = response_json(prefs_set).await;
    assert_eq!(prefs_set_status, StatusCode::OK);
    assert_eq!(prefs_set_body["data"]["theme"], "dark");

    let etymology_first = request(
        &app.app,
        Method::GET,
        &format!("/api/content/etymology/{word_id}"),
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (ety_first_status, _, ety_first_body) = response_json(etymology_first).await;
    assert_eq!(ety_first_status, StatusCode::OK);
    assert!(ety_first_body["data"]["word"].is_string());

    let etymology_second = request(
        &app.app,
        Method::GET,
        &format!("/api/content/etymology/{word_id}"),
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (ety_second_status, _, _) = response_json(etymology_second).await;
    assert_eq!(ety_second_status, StatusCode::OK);

    let missing_etymology = request(
        &app.app,
        Method::GET,
        "/api/content/etymology/not-exists",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (missing_ety_status, _, _) = response_json(missing_etymology).await;
    assert_eq!(missing_ety_status, StatusCode::NOT_FOUND);

    let semantic_search = request(
        &app.app,
        Method::GET,
        "/api/content/semantic/search?query=coverage&limit=5",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (semantic_status, _, semantic_body) = response_json(semantic_search).await;
    assert_eq!(semantic_status, StatusCode::OK);
    // method can be "text_search" or "keyword_fallback" depending on implementation
    assert!(semantic_body["data"]["method"].is_string());

    let word_contexts = request(
        &app.app,
        Method::GET,
        &format!("/api/content/word-contexts/{word_id}"),
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (contexts_status, _, contexts_body) = response_json(word_contexts).await;
    assert_eq!(contexts_status, StatusCode::OK);
    assert!(contexts_body["data"]["contexts"].is_array());

    let get_morphemes = request(
        &app.app,
        Method::GET,
        &format!("/api/content/morphemes/{word_id}"),
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (morphemes_get_status, _, morphemes_get_body) = response_json(get_morphemes).await;
    assert_eq!(morphemes_get_status, StatusCode::OK);
    assert!(morphemes_get_body["data"]["morphemes"].is_array());

    let set_morphemes = request(
        &app.app,
        Method::POST,
        &format!("/api/content/morphemes/{word_id}"),
        Some(serde_json::json!({
            "wordId": word_id,
            "morphemes": [
                { "text": "cover", "type": "root", "meaning": "to hide" },
                { "text": "age", "type": "suffix", "meaning": "result" }
            ]
        })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (morphemes_set_status, _, morphemes_set_body) = response_json(set_morphemes).await;
    assert_eq!(morphemes_set_status, StatusCode::OK);
    assert_eq!(
        morphemes_set_body["data"]["morphemes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let current_user = current_user_id(&app.app, &token).await;
    app.state
        .store()
        .confusion_pairs
        .insert(
            keys::confusion_pair_key("aaa-word", &word_id)
                .unwrap()
                .as_bytes(),
            serde_json::to_vec(
                &serde_json::json!({ "wordA": "aaa-word", "wordB": word_id, "score": 0.9 }),
            )
            .expect("confusion pair 1 bytes"),
        )
        .expect("insert confusion pair 1");
    app.state
        .store()
        .confusion_pairs
        .insert(
            keys::confusion_pair_key(&word_id, "zzzz")
                .unwrap()
                .as_bytes(),
            serde_json::to_vec(
                &serde_json::json!({ "wordA": word_id, "wordB": "zzzz", "score": 0.4 }),
            )
            .expect("confusion pair 2 bytes"),
        )
        .expect("insert confusion pair 2");

    let confusion_pairs = request(
        &app.app,
        Method::GET,
        &format!("/api/content/confusion-pairs/{current_user}"),
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (confusion_status, _, confusion_body) = response_json(confusion_pairs).await;
    assert_eq!(confusion_status, StatusCode::OK);
    assert!(confusion_body["data"]["confusionPairs"].is_array());

    let v1_words = request(
        &app.app,
        Method::GET,
        "/api/v1/words?page=1&per_page=10",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (v1_words_status, _, _) = response_json(v1_words).await;
    assert_eq!(v1_words_status, StatusCode::OK);

    let v1_word = request(
        &app.app,
        Method::GET,
        &format!("/api/v1/words/{word_id}"),
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (v1_word_status, _, _) = response_json(v1_word).await;
    assert_eq!(v1_word_status, StatusCode::OK);

    let v1_create_record = request(
        &app.app,
        Method::POST,
        "/api/v1/records",
        Some(serde_json::json!({
            "wordId": word_id,
            "isCorrect": true,
            "responseTimeMs": 800
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (v1_record_status, _, _) = response_json(v1_create_record).await;
    assert_eq!(v1_record_status, StatusCode::OK);

    let v1_records = request(
        &app.app,
        Method::GET,
        "/api/v1/records?page=1&per_page=20",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (v1_records_status, _, v1_records_body) = response_json(v1_records).await;
    assert_eq!(v1_records_status, StatusCode::OK);
    assert!(v1_records_body["data"]["data"].is_array());

    let v1_config = request(
        &app.app,
        Method::GET,
        "/api/v1/study-config",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (v1_config_status, _, _) = response_json(v1_config).await;
    assert_eq!(v1_config_status, StatusCode::OK);

    let v1_create_session = request(
        &app.app,
        Method::POST,
        "/api/v1/learning/session",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (v1_session_status, _, v1_session_body) = response_json(v1_create_session).await;
    assert_eq!(v1_session_status, StatusCode::OK);
    assert!(v1_session_body["data"]["sessionId"].is_string());

    let v1_resume_session = request(
        &app.app,
        Method::POST,
        "/api/v1/learning/session",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (v1_resume_status, _, v1_resume_body) = response_json(v1_resume_session).await;
    assert_eq!(v1_resume_status, StatusCode::OK);
    assert_eq!(v1_resume_body["data"]["resumed"], true);
}

#[tokio::test]
async fn it_words_users_records_auth_and_extractor_edges() {
    let app = spawn_test_server().await;
    let token = login_and_get_token(&app.app).await;
    let user_id = current_user_id(&app.app, &token).await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let bad_username = request(
        &app.app,
        Method::PUT,
        "/api/users/me",
        Some(serde_json::json!({ "username": "   " })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (bad_username_status, _, _) = response_json(bad_username).await;
    assert_eq!(bad_username_status, StatusCode::BAD_REQUEST);

    let update_profile = request(
        &app.app,
        Method::PUT,
        "/api/users/me",
        Some(serde_json::json!({ "username": "route-cover-user" })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (update_profile_status, _, update_profile_body) = response_json(update_profile).await;
    assert_eq!(update_profile_status, StatusCode::OK);
    assert_eq!(update_profile_body["data"]["username"], "route-cover-user");

    let weak_password = request(
        &app.app,
        Method::PUT,
        "/api/users/me/password",
        Some(serde_json::json!({
            "currentPassword": "Passw0rd!",
            "newPassword": "123"
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (weak_password_status, _, _) = response_json(weak_password).await;
    assert_eq!(weak_password_status, StatusCode::BAD_REQUEST);

    let wrong_password = request(
        &app.app,
        Method::PUT,
        "/api/users/me/password",
        Some(serde_json::json!({
            "currentPassword": "WrongPass!",
            "newPassword": "NewPassw0rd!"
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (wrong_password_status, _, _) = response_json(wrong_password).await;
    assert_eq!(wrong_password_status, StatusCode::UNAUTHORIZED);

    let change_password = request(
        &app.app,
        Method::PUT,
        "/api/users/me/password",
        Some(serde_json::json!({
            "currentPassword": "Passw0rd!",
            "newPassword": "NewPassw0rd!"
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (change_password_status, _, _) = response_json(change_password).await;
    assert_eq!(change_password_status, StatusCode::OK);

    let me_with_old_token = request(
        &app.app,
        Method::GET,
        "/api/users/me",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (me_old_status, _, _) = response_json(me_with_old_token).await;
    assert_eq!(me_old_status, StatusCode::UNAUTHORIZED);

    let user = app
        .state
        .store()
        .get_user_by_id(&user_id)
        .expect("get user by id")
        .expect("user exists");

    let login_new_password = request(
        &app.app,
        Method::POST,
        "/api/auth/login",
        Some(serde_json::json!({
            "email": user.email,
            "password": "NewPassw0rd!"
        })),
        &[],
    )
    .await;
    let (login_new_status, _, login_new_body) = response_json(login_new_password).await;
    assert_eq!(login_new_status, StatusCode::OK);
    let new_token = login_new_body["data"]["accessToken"]
        .as_str()
        .expect("new access token")
        .to_string();

    let word_1 = create_word(&app.app, &admin_token, "delta", "德尔塔").await;
    let word_2 = create_word(&app.app, &admin_token, "epsilon", "艾普西隆").await;

    let list_search = request(
        &app.app,
        Method::GET,
        "/api/words?page=1&per_page=20&search=del",
        None,
        &[("authorization", auth_header(&new_token))],
    )
    .await;
    let (list_search_status, _, list_search_body) = response_json(list_search).await;
    assert_eq!(list_search_status, StatusCode::OK);
    assert!(list_search_body["data"]["data"].is_array());

    let count_words = request(
        &app.app,
        Method::GET,
        "/api/words/count",
        None,
        &[("authorization", auth_header(&new_token))],
    )
    .await;
    let (count_words_status, _, count_words_body) = response_json(count_words).await;
    assert_eq!(count_words_status, StatusCode::OK);
    assert!(count_words_body["data"]["total"].as_u64().unwrap_or(0) >= 2);

    let update_word = request(
        &app.app,
        Method::PUT,
        &format!("/api/words/{word_1}"),
        Some(serde_json::json!({
            "text": "delta-updated",
            "meaning": "更新含义",
            "difficulty": 0.9,
            "examples": ["example-1"],
            "tags": ["updated"]
        })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (update_word_status, _, update_word_body) = response_json(update_word).await;
    assert_eq!(update_word_status, StatusCode::OK);
    assert_eq!(update_word_body["data"]["text"], "delta-updated");

    let batch_create = request(
        &app.app,
        Method::POST,
        "/api/words/batch",
        Some(serde_json::json!({
            "words": [
                { "text": "zeta", "meaning": "z" },
                { "text": "", "meaning": "skip-me" }
            ]
        })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (batch_status, _, batch_body) = response_json(batch_create).await;
    assert_eq!(batch_status, StatusCode::CREATED);
    assert_eq!(batch_body["data"]["count"], 1);

    let too_many_words: Vec<serde_json::Value> = (0..501)
        .map(|idx| serde_json::json!({ "text": format!("word-{idx}"), "meaning": "m" }))
        .collect();
    let batch_too_large = request(
        &app.app,
        Method::POST,
        "/api/words/batch",
        Some(serde_json::json!({ "words": too_many_words })),
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (batch_too_large_status, _, _) = response_json(batch_too_large).await;
    assert_eq!(batch_too_large_status, StatusCode::BAD_REQUEST);

    let create_learning_session = request(
        &app.app,
        Method::POST,
        "/api/learning/session",
        None,
        &[("authorization", auth_header(&new_token))],
    )
    .await;
    let (session_status, _, session_body) = response_json(create_learning_session).await;
    assert_eq!(session_status, StatusCode::OK);
    let session_id = session_body["data"]["sessionId"]
        .as_str()
        .unwrap_or_default();

    let create_record = request(
        &app.app,
        Method::POST,
        "/api/records",
        Some(serde_json::json!({
            "wordId": word_1,
            "isCorrect": true,
            "responseTimeMs": 650,
            "sessionId": session_id,
            "hintUsed": true
        })),
        &[("authorization", auth_header(&new_token))],
    )
    .await;
    let (create_record_status, _, create_record_body) = response_json(create_record).await;
    assert_eq!(create_record_status, StatusCode::CREATED);
    assert!(create_record_body["data"]["amasResult"].is_object());

    let list_records = request(
        &app.app,
        Method::GET,
        "/api/records?page=1&per_page=20",
        None,
        &[("authorization", auth_header(&new_token))],
    )
    .await;
    let (list_records_status, _, list_records_body) = response_json(list_records).await;
    assert_eq!(list_records_status, StatusCode::OK);
    assert!(list_records_body["data"]["data"].is_array());

    for stats_path in [
        "/api/records/statistics",
        "/api/records/statistics/enhanced",
    ] {
        let response = request(
            &app.app,
            Method::GET,
            stats_path,
            None,
            &[("authorization", auth_header(&new_token))],
        )
        .await;
        let (status, _, _) = response_json(response).await;
        assert_eq!(status, StatusCode::OK, "path: {stats_path}");
    }

    let batch_records = request(
        &app.app,
        Method::POST,
        "/api/records/batch",
        Some(serde_json::json!({
            "records": [
                { "wordId": word_2, "isCorrect": false, "responseTimeMs": 1200 },
                { "wordId": "missing-word", "isCorrect": true, "responseTimeMs": 900 }
            ]
        })),
        &[("authorization", auth_header(&new_token))],
    )
    .await;
    let (batch_records_status, _, batch_records_body) = response_json(batch_records).await;
    assert_eq!(batch_records_status, StatusCode::CREATED);
    assert_eq!(batch_records_body["data"]["count"], 2);

    let too_many_records: Vec<serde_json::Value> = (0..501)
        .map(|idx| {
            serde_json::json!({
                "wordId": format!("w-{idx}"),
                "isCorrect": true,
                "responseTimeMs": 500
            })
        })
        .collect();
    let batch_records_too_large = request(
        &app.app,
        Method::POST,
        "/api/records/batch",
        Some(serde_json::json!({ "records": too_many_records })),
        &[("authorization", auth_header(&new_token))],
    )
    .await;
    let (batch_records_too_large_status, _, _) = response_json(batch_records_too_large).await;
    assert_eq!(batch_records_too_large_status, StatusCode::BAD_REQUEST);

    let delete_word = request(
        &app.app,
        Method::DELETE,
        &format!("/api/words/{word_2}"),
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (delete_status, _, delete_body) = response_json(delete_word).await;
    assert_eq!(delete_status, StatusCode::OK);
    assert_eq!(delete_body["data"]["deleted"], true);

    let get_deleted_word = request(
        &app.app,
        Method::GET,
        &format!("/api/words/{word_2}"),
        None,
        &[("authorization", auth_header(&new_token))],
    )
    .await;
    let (deleted_word_status, _, _) = response_json(get_deleted_word).await;
    assert_eq!(deleted_word_status, StatusCode::NOT_FOUND);

    let register_invalid_email = request(
        &app.app,
        Method::POST,
        "/api/auth/register",
        Some(serde_json::json!({
            "email": "invalid-email",
            "username": "x",
            "password": "Passw0rd!"
        })),
        &[],
    )
    .await;
    let (register_invalid_status, _, _) = response_json(register_invalid_email).await;
    assert_eq!(register_invalid_status, StatusCode::BAD_REQUEST);

    let register_weak_password = request(
        &app.app,
        Method::POST,
        "/api/auth/register",
        Some(serde_json::json!({
            "email": "weak@example.com",
            "username": "weak",
            "password": "123"
        })),
        &[],
    )
    .await;
    let (register_weak_status, _, _) = response_json(register_weak_password).await;
    assert_eq!(register_weak_status, StatusCode::BAD_REQUEST);

    let forgot_password = request(
        &app.app,
        Method::POST,
        "/api/auth/forgot-password",
        Some(serde_json::json!({ "email": app.state.store().get_user_by_id(&user_id).unwrap().unwrap().email })),
        &[],
    )
    .await;
    let (forgot_status, _, forgot_body) = response_json(forgot_password).await;
    assert_eq!(forgot_status, StatusCode::OK);
    assert_eq!(forgot_body["data"]["emailSent"], true);

    let reset_weak = request(
        &app.app,
        Method::POST,
        "/api/auth/reset-password",
        Some(serde_json::json!({
            "token": "any-token",
            "newPassword": "123"
        })),
        &[],
    )
    .await;
    let (reset_weak_status, _, _) = response_json(reset_weak).await;
    assert_eq!(reset_weak_status, StatusCode::BAD_REQUEST);

    let reset_invalid_token = request(
        &app.app,
        Method::POST,
        "/api/auth/reset-password",
        Some(serde_json::json!({
            "token": "missing-token",
            "newPassword": "NewResetPass1!"
        })),
        &[],
    )
    .await;
    let (reset_invalid_status, _, _) = response_json(reset_invalid_token).await;
    assert_eq!(reset_invalid_status, StatusCode::BAD_REQUEST);

    app.state.store().ban_user(&user_id).expect("ban user");
    let banned_login = request(
        &app.app,
        Method::POST,
        "/api/auth/login",
        Some(serde_json::json!({
            "email": app.state.store().get_user_by_id(&user_id).unwrap().unwrap().email,
            "password": "NewPassw0rd!"
        })),
        &[],
    )
    .await;
    let (banned_login_status, _, _) = response_json(banned_login).await;
    assert_eq!(banned_login_status, StatusCode::FORBIDDEN);

    let token_for_json_tests = login_and_get_token(&app.app).await;

    let missing_content_type = request_raw(
        &app.app,
        Method::PUT,
        "/api/study-config",
        br#"{"dailyWordCount": 9}"#.to_vec(),
        &[("authorization", auth_header(&token_for_json_tests))],
    )
    .await;
    let (missing_ct_status, _, missing_ct_body) = response_json(missing_content_type).await;
    assert_eq!(missing_ct_status, StatusCode::BAD_REQUEST);
    // Error code can be MISSING_CONTENT_TYPE or INVALID_REQUEST_BODY depending on middleware
    assert!(missing_ct_body["code"].is_string());

    let invalid_json = request_raw(
        &app.app,
        Method::PUT,
        "/api/study-config",
        br#"{"dailyWordCount": 9"#.to_vec(),
        &[
            ("authorization", auth_header(&token_for_json_tests)),
            ("content-type", "application/json".to_string()),
        ],
    )
    .await;
    let (invalid_json_status, _, invalid_json_body) = response_json(invalid_json).await;
    assert_eq!(invalid_json_status, StatusCode::BAD_REQUEST);
    // Error code can be INVALID_JSON_SYNTAX or INVALID_REQUEST_BODY depending on middleware
    assert!(invalid_json_body["code"].is_string());
}
