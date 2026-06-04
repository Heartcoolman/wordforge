mod common;

use axum::http::{Method, StatusCode};
use axum::Router;
use chrono::{Duration, Utc};

use common::app::spawn_test_server;
use common::auth::auth_header;
use common::fixtures::seed_words;
use common::http::{request, response_json};
use learning_backend::store::operations::learning_sessions::{
    LearningSession, SessionStatus, SessionSummary,
};
use learning_backend::store::operations::records::{LearningRecord, RecordType};
use learning_backend::store::operations::wb_center::WordbookCenterImport;
use learning_backend::store::operations::word_states::{WordLearningState, WordState};
use learning_backend::store::operations::wordbooks::{Wordbook, WordbookType};
use learning_backend::store::Store;

async fn register_user(app: &Router) -> (String, String) {
    let email = format!("data-api-{}@test.com", uuid::Uuid::new_v4());
    let username = format!("data-api-{}", uuid::Uuid::new_v4().simple());
    let response = request(
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
    let (status, _, body) = response_json(response).await;
    assert_eq!(status, StatusCode::CREATED, "register failed: {body}");
    let token = body["data"]["accessToken"].as_str().unwrap().to_string();
    let user_id = body["data"]["user"]["id"].as_str().unwrap().to_string();
    (token, user_id)
}

fn create_completed_session(store: &Store, user_id: &str, mastered_word_id: &str) -> String {
    let now = Utc::now();
    let session_id = uuid::Uuid::new_v4().to_string();
    let session = LearningSession {
        id: session_id.clone(),
        user_id: user_id.to_string(),
        status: SessionStatus::Completed,
        target_mastery_count: 2,
        total_questions: 2,
        actual_mastery_count: 1,
        context_shifts: 0,
        // created_at 锚定到 now（与本测试的 record 数据同源），而非 now-30min：后者在服务端时区
        // （Asia/Shanghai）午夜后 0–30 分钟窗口会跨到“昨天”，被 range=day 聚合排除导致
        // studyDurationSecs/sessionCount/daily 偏 0（每日 UTC 16:00–16:30 必现的边界 flaky）。
        // 会话时长由 summary.duration_secs 提供，不依赖 created_at/updated_at，故对齐 now 不影响断言。
        created_at: now,
        updated_at: now,
        summary: Some(SessionSummary {
            accuracy: 0.5,
            avg_response_time_ms: 900,
            mastered_word_ids: vec![mastered_word_id.to_string()],
            error_prone_word_ids: vec![],
            duration_secs: 1800,
            hour_of_day: 10,
            final_difficulty: 0.5,
        }),
        correct_count: 1,
        total_count: 2,
    };
    store.create_learning_session(&session).unwrap();
    session_id
}

fn create_record(store: &Store, user_id: &str, word_id: &str, session_id: &str, is_correct: bool) {
    store
        .create_record(&LearningRecord {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            word_id: word_id.to_string(),
            is_correct,
            response_time_ms: 1000,
            session_id: Some(session_id.to_string()),
            created_at: Utc::now(),
            record_type: RecordType::All,
            self_rating: None,
            question_mode: None,
        })
        .unwrap();
}

#[tokio::test]
async fn dashboard_and_session_history_return_real_learning_data() {
    let app = spawn_test_server().await;
    let (token, user_id) = register_user(&app.app).await;
    let words = seed_words(app.state.store(), 2);
    let session_id = create_completed_session(app.state.store(), &user_id, &words[0].id);
    create_record(app.state.store(), &user_id, &words[0].id, &session_id, true);
    create_record(
        app.state.store(),
        &user_id,
        &words[1].id,
        &session_id,
        false,
    );

    let dashboard = request(
        &app.app,
        Method::GET,
        "/api/analytics/dashboard?range=day",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(dashboard).await;
    assert_eq!(status, StatusCode::OK, "dashboard body: {body}");
    assert_eq!(body["data"]["range"]["timezone"], "Asia/Shanghai");
    assert_eq!(body["data"]["summary"]["studyDurationSecs"], 1800);
    assert_eq!(body["data"]["summary"]["sessionCount"], 1);
    assert_eq!(body["data"]["summary"]["recordCount"], 2);
    assert_eq!(body["data"]["summary"]["correctCount"], 1);
    assert_eq!(body["data"]["summary"]["newWords"], 2);
    assert_eq!(body["data"]["summary"]["masteredWords"], 1);
    assert_eq!(body["data"]["daily"].as_array().unwrap().len(), 1);

    let sessions = request(
        &app.app,
        Method::GET,
        "/api/learning/sessions?page=1&perPage=10",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(sessions).await;
    assert_eq!(status, StatusCode::OK, "sessions body: {body}");
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["data"][0]["sessionId"], session_id);
    assert_eq!(body["data"]["data"][0]["durationSecs"], 1800);
    assert_eq!(body["data"]["data"][0]["recordCount"], 2);
    assert_eq!(body["data"]["data"][0]["correctCount"], 1);
}

#[tokio::test]
async fn wordbook_progress_favorites_and_notes_work_for_current_user() {
    let app = spawn_test_server().await;
    let (token, user_id) = register_user(&app.app).await;
    let words = seed_words(app.state.store(), 2);
    let wordbook = Wordbook {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Progress Book".to_string(),
        description: String::new(),
        book_type: WordbookType::System,
        user_id: None,
        word_count: 0,
        created_at: Utc::now(),
    };
    app.state.store().upsert_wordbook(&wordbook).unwrap();
    app.state
        .store()
        .add_word_to_wordbook(&wordbook.id, &words[0].id)
        .unwrap();
    app.state
        .store()
        .add_word_to_wordbook(&wordbook.id, &words[1].id)
        .unwrap();
    app.state
        .store()
        .set_word_learning_state(&WordLearningState {
            user_id: user_id.clone(),
            word_id: words[0].id.clone(),
            state: WordState::Mastered,
            mastery_level: 1.0,
            next_review_date: None,
            half_life: 24.0,
            correct_streak: 3,
            total_attempts: 3,
            updated_at: Utc::now(),
        })
        .unwrap();

    let progress = request(
        &app.app,
        Method::GET,
        &format!("/api/wordbooks/progress?wordbookIds={}", wordbook.id),
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(progress).await;
    assert_eq!(status, StatusCode::OK, "progress body: {body}");
    assert_eq!(body["data"][0]["wordCount"], 2);
    assert_eq!(body["data"][0]["studiedWords"], 1);
    assert_eq!(body["data"][0]["masteredWords"], 1);

    let favorite = request(
        &app.app,
        Method::POST,
        &format!("/api/word-favorites/{}", words[0].id),
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(favorite).await;
    assert_eq!(status, StatusCode::OK, "favorite body: {body}");
    assert_eq!(body["data"]["favorited"], true);

    let favorites = request(
        &app.app,
        Method::GET,
        "/api/word-favorites",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(favorites).await;
    assert_eq!(status, StatusCode::OK, "favorites body: {body}");
    // P3#5：list_favorites 改 paginated()，列表在 data.data 而非 data 直接为数组
    assert_eq!(body["data"]["data"][0]["word"]["id"], words[0].id);

    let create_note = request(
        &app.app,
        Method::POST,
        "/api/word-notes",
        Some(serde_json::json!({
            "wordId": words[0].id,
            "content": "memory hook",
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(create_note).await;
    assert_eq!(status, StatusCode::CREATED, "create note body: {body}");
    let note_id = body["data"]["id"].as_str().unwrap().to_string();

    let update_note = request(
        &app.app,
        Method::PUT,
        &format!("/api/word-notes/{note_id}"),
        Some(serde_json::json!({ "content": "updated hook" })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(update_note).await;
    assert_eq!(status, StatusCode::OK, "update note body: {body}");
    assert_eq!(body["data"]["content"], "updated hook");
}

#[tokio::test]
async fn import_history_includes_legacy_wordbook_center_records() {
    let app = spawn_test_server().await;
    let (token, user_id) = register_user(&app.app).await;
    let wordbook = Wordbook {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Legacy Import".to_string(),
        description: String::new(),
        book_type: WordbookType::User,
        user_id: Some(user_id.clone()),
        word_count: 12,
        created_at: Utc::now(),
    };
    app.state.store().upsert_wordbook(&wordbook).unwrap();
    app.state
        .store()
        .upsert_wb_center_import(&WordbookCenterImport {
            remote_id: "remote-book".to_string(),
            local_wordbook_id: wordbook.id.clone(),
            source_url: "https://example.com/catalog".to_string(),
            version: "1.0".to_string(),
            user_id: Some(user_id),
            imported_at: Utc::now(),
            updated_at: Utc::now(),
            word_count: 12,
        })
        .unwrap();

    let response = request(
        &app.app,
        Method::GET,
        "/api/wordbook-center/import-history?page=1&perPage=20",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(response).await;
    assert_eq!(status, StatusCode::OK, "history body: {body}");
    assert_eq!(body["data"]["total"], 1);
    assert_eq!(body["data"]["data"][0]["status"], "success");
    assert_eq!(body["data"]["data"][0]["wordbookName"], "Legacy Import");
    assert_eq!(
        body["data"]["data"][0]["wordsSkipped"],
        serde_json::Value::Null
    );
}

#[tokio::test]
async fn forgetting_risk_reports_due_and_high_risk_words() {
    let app = spawn_test_server().await;
    let (token, user_id) = register_user(&app.app).await;
    let words = seed_words(app.state.store(), 1);
    app.state
        .store()
        .set_word_learning_state(&WordLearningState {
            user_id,
            word_id: words[0].id.clone(),
            state: WordState::Reviewing,
            mastery_level: 0.5,
            next_review_date: Some(Utc::now() - Duration::hours(3)),
            half_life: 0.5,
            correct_streak: 1,
            total_attempts: 2,
            updated_at: Utc::now() - Duration::hours(12),
        })
        .unwrap();

    let response = request(
        &app.app,
        Method::GET,
        "/api/analytics/forgetting-risk?range=week",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(response).await;
    assert_eq!(status, StatusCode::OK, "forgetting body: {body}");
    assert_eq!(body["data"]["summary"]["highRiskWords"], 1);
    assert_eq!(body["data"]["summary"]["dueReviewWords"], 1);
    assert_eq!(body["data"]["riskWords"][0]["wordId"], words[0].id);
    assert_eq!(body["data"]["riskWords"][0]["riskLevel"], "high");
}
