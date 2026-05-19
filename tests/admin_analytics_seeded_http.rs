//! admin/analytics 路由补充 —— 在 store 中预置足量学习记录，触发：
//!   - calc_trend 中 yesterday > 0 的真实计算分支
//!   - record_types 的 daily / totals 聚合分支（learning/review/all 都有数据）
//!   - retention_curve 的 estimate_retention / nearest_bucket 真实分支
//!   - daily_active_users / daily_records 的填充分支（map 命中）
//!   - study_overview 的 summary + daily 非零分支

mod common;

use axum::http::{Method, StatusCode};
use chrono::{Duration, Utc};

use common::app::spawn_test_server;
use common::auth::{auth_header, setup_admin_and_get_token};
use common::http::{request, response_json};

use learning_backend::auth::hash_password;
use learning_backend::store::operations::records::{LearningRecord, RecordType};
use learning_backend::store::operations::users::User;
use learning_backend::store::operations::word_states::{WordLearningState, WordState};
use learning_backend::store::operations::words::Word;
use learning_backend::store::Store;

/// 在指定 date 上注入一组学习记录。
fn seed_records(
    store: &Store,
    user_id: &str,
    word_ids: &[String],
    correct_ratio: f64,
    record_type: RecordType,
    at: chrono::DateTime<Utc>,
) {
    for (i, wid) in word_ids.iter().enumerate() {
        let rec = LearningRecord {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            word_id: wid.clone(),
            is_correct: (i as f64) < (word_ids.len() as f64) * correct_ratio,
            response_time_ms: 1500,
            session_id: None,
            created_at: at,
            record_type,
            self_rating: None,
        };
        store.create_record(&rec).expect("create_record");
    }
}

fn seed_user(store: &Store, suffix: &str) -> User {
    let now = Utc::now();
    let user = User {
        id: format!("u-{suffix}"),
        email: format!("{suffix}@analytics.test"),
        username: format!("user-{suffix}"),
        password_hash: hash_password("Passw0rd!").expect("hash"),
        is_banned: false,
        created_at: now,
        updated_at: now,
        failed_login_count: 0,
        locked_until: None,
    };
    store.create_user(&user).expect("create user");
    user
}

fn seed_words(store: &Store, prefix: &str, count: usize) -> Vec<String> {
    (0..count)
        .map(|i| {
            let w = Word {
                id: format!("w-{prefix}-{i}"),
                text: format!("{prefix}-{i}"),
                meaning: format!("meaning {i}"),
                pronunciation: None,
                part_of_speech: None,
                difficulty: 0.5,
                examples: vec![],
                tags: vec![],
                embedding: None,
                created_at: Utc::now(),
            };
            store.upsert_word(&w).expect("upsert word");
            w.id
        })
        .collect()
}

#[tokio::test]
async fn it_admin_analytics_with_seeded_data_hits_real_branches() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    // seed user + words
    let user = seed_user(app.state.store(), "seed");
    let words = seed_words(app.state.store(), "ana", 30);

    // 昨天的 review 记录（有 correct/total > 0）
    let yesterday = Utc::now() - Duration::days(1);
    seed_records(
        app.state.store(),
        &user.id,
        &words[..10],
        0.6,
        RecordType::Review,
        yesterday,
    );
    // 今日的 learning 记录
    seed_records(
        app.state.store(),
        &user.id,
        &words[10..20],
        0.8,
        RecordType::Learning,
        Utc::now(),
    );
    // 今日的 all 记录
    seed_records(
        app.state.store(),
        &user.id,
        &words[20..30],
        0.5,
        RecordType::All,
        Utc::now(),
    );

    // engagement —— calc_trend 应该走 yesterday>0 真实计算分支
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/analytics/engagement",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert!(body["data"]["totalUsers"].as_u64().unwrap_or(0) >= 1);

    // learning metrics —— acc_today / acc_yesterday 走 records_today > 0 分支
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/analytics/learning",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["totalRecords"].as_u64().unwrap_or(0) >= 1);

    // record-types —— daily/totals 聚合 learning/review/all 三类
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/analytics/record-types?days=7",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let totals = body["data"]["totals"].as_array().expect("totals");
    assert!(totals.len() == 3);
    // 验证至少存在 learning/review/all 三种
    let types: Vec<&str> = totals
        .iter()
        .filter_map(|t| t["recordType"].as_str())
        .collect();
    assert!(types.contains(&"learning"));
    assert!(types.contains(&"review"));
    assert!(types.contains(&"all"));

    // daily-active-users —— map.get 命中分支
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/analytics/daily-active-users?days=7",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let entries = body["data"].as_array().expect("array");
    assert_eq!(entries.len(), 7);

    // daily-records —— fill_daily_records_entries map.get 命中
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/analytics/daily-records?days=7",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    let entries = body["data"].as_array().expect("array");
    assert_eq!(entries.len(), 7);

    // study-overview —— summary + daily 非零
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/analytics/study-overview?days=7",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["data"]["daily"].as_array().unwrap().len() == 7);

    // study-overview category=learning
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/analytics/study-overview?days=14&category=learning",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);

    // record-types days=30 极限值
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/analytics/record-types?days=30",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
}

/// 校验 word-states 在 dist.tracked > 0 时返回正确字段。
#[tokio::test]
async fn it_admin_analytics_word_states_with_seeded_states() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let user = seed_user(app.state.store(), "ws");
    let words = seed_words(app.state.store(), "ws", 5);

    let states = [
        WordState::New,
        WordState::Learning,
        WordState::Reviewing,
        WordState::Mastered,
        WordState::Forgotten,
    ];
    for (i, wid) in words.iter().enumerate() {
        let wls = WordLearningState {
            user_id: user.id.clone(),
            word_id: wid.clone(),
            state: states[i].clone(),
            mastery_level: 0.4 + (i as f64) * 0.1,
            next_review_date: Some(Utc::now() + Duration::days(1)),
            half_life: 24.0,
            correct_streak: 2,
            total_attempts: 5,
            updated_at: Utc::now(),
        };
        app.state
            .store()
            .set_word_learning_state(&wls)
            .expect("set wls");
    }

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/analytics/word-states",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert!(body["data"]["states"].is_object());
    assert!(body["data"]["totals"].is_object());
    // 各状态都有 1 个
    let states_obj = &body["data"]["states"];
    let total = states_obj["newCount"].as_i64().unwrap_or(0)
        + states_obj["learning"].as_i64().unwrap_or(0)
        + states_obj["reviewing"].as_i64().unwrap_or(0)
        + states_obj["mastered"].as_i64().unwrap_or(0)
        + states_obj["forgotten"].as_i64().unwrap_or(0);
    assert_eq!(total, 5);
}

/// 校验 retention-curve 在有 word_learning_state 数据时进入 estimate_retention 分支。
#[tokio::test]
async fn it_admin_analytics_retention_curve_with_history() {
    let app = spawn_test_server().await;
    let admin_token = setup_admin_and_get_token(&app.app).await;

    let user = seed_user(app.state.store(), "ret");
    let words = seed_words(app.state.store(), "ret", 6);

    // 模拟分散在过去 1-30 天的学习状态，让 nearest_bucket 命中多种 bucket
    let days_back = [1, 2, 4, 7, 15, 30];
    for (i, wid) in words.iter().enumerate() {
        let first_learned = Utc::now() - Duration::days(days_back[i]);
        let wls = WordLearningState {
            user_id: user.id.clone(),
            word_id: wid.clone(),
            state: WordState::Reviewing,
            mastery_level: 0.7,
            next_review_date: Some(Utc::now() + Duration::days(1)),
            half_life: 48.0,
            correct_streak: 4,
            total_attempts: 5,
            updated_at: first_learned,
        };
        app.state
            .store()
            .set_word_learning_state(&wls)
            .expect("set wls");
        // 加几条 record，让 total_attempts > 0 在 admin 视角下也可见
        seed_records(
            app.state.store(),
            &user.id,
            &[wid.clone()],
            0.6,
            RecordType::Review,
            first_learned + Duration::days(1),
        );
    }

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/analytics/retention-curve",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    let points = body["data"]["points"].as_array().expect("points");
    assert_eq!(points.len(), 6); // 6 buckets

    // category=review 路径
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/analytics/retention-curve?category=review",
        None,
        &[("authorization", auth_header(&admin_token))],
    )
    .await;
    let (status, _, _) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
}
