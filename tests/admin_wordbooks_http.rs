//! admin 词库中心 HTTP 集成测试。覆盖 /api/admin/wordbooks 全部端点:
//! 列表+counts、stats、words(排序/搜索)、create/patch/add-word/remove-word/delete、
//! history 审计、export。鉴权走 admin token;统计依赖直接 seed 到 store 的跨用户数据。

mod common;

use axum::http::{Method, StatusCode};
use chrono::Utc;

use common::app::spawn_test_server;
use common::auth::{auth_header, setup_admin_and_get_token};
use common::http::{request, response_json};

use learning_backend::auth::hash_password;
use learning_backend::store::operations::records::{LearningRecord, RecordType};
use learning_backend::store::operations::users::User;
use learning_backend::store::operations::word_states::{WordLearningState, WordState};
use learning_backend::store::operations::wordbooks::{Wordbook, WordbookType};
use learning_backend::store::operations::words::Word;
use learning_backend::store::Store;

fn seed_user(store: &Store, id: &str, email: &str) {
    let now = Utc::now();
    store
        .create_user(&User {
            id: id.to_string(),
            email: email.to_string(),
            username: format!("user-{id}"),
            password_hash: hash_password("Passw0rd!").expect("hash"),
            is_banned: false,
            created_at: now,
            updated_at: now,
            failed_login_count: 0,
            locked_until: None,
            role: "user".to_string(),
            status: "active".to_string(),
            last_login_at: None,
            referrer_source: None,
        })
        .expect("create user");
}

fn seed_wordbook(store: &Store, id: &str, name: &str, t: WordbookType, user_id: Option<&str>) {
    store
        .upsert_wordbook(&Wordbook {
            id: id.to_string(),
            name: name.to_string(),
            description: String::new(),
            book_type: t,
            user_id: user_id.map(Into::into),
            word_count: 0,
            created_at: Utc::now(),
        })
        .expect("upsert wordbook");
}

fn seed_word(store: &Store, id: &str, text: &str, meaning: &str, pos: Option<&str>, diff: f64) {
    store
        .upsert_word(&Word {
            id: id.to_string(),
            text: text.to_string(),
            meaning: meaning.to_string(),
            pronunciation: None,
            part_of_speech: pos.map(Into::into),
            difficulty: diff,
            examples: vec!["示例".to_string()],
            tags: vec![],
            embedding: None,
            created_at: Utc::now(),
        })
        .expect("upsert word");
}

fn seed_record(store: &Store, user: &str, word: &str, correct: bool) {
    store
        .create_record(&LearningRecord {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user.to_string(),
            word_id: word.to_string(),
            is_correct: correct,
            response_time_ms: 1200,
            session_id: None,
            created_at: Utc::now(),
            record_type: RecordType::All,
            self_rating: None,
            question_mode: None,
        })
        .expect("create record");
}

fn seed_state(store: &Store, user: &str, word: &str, mastery: f64, attempts: u32) {
    store
        .set_word_learning_state(&WordLearningState {
            user_id: user.to_string(),
            word_id: word.to_string(),
            state: WordState::Learning,
            mastery_level: mastery,
            next_review_date: None,
            half_life: 24.0,
            correct_streak: 0,
            total_attempts: attempts,
            updated_at: Utc::now(),
        })
        .expect("set state");
}

/// 列表 + counts + 排序/过滤;以及未鉴权拒绝。
#[tokio::test]
async fn it_list_wordbooks_with_counts_and_filters() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let store = app.state.store();

    seed_user(store, "u1", "owner@wb.test");
    seed_wordbook(store, "sys1", "GRE Core", WordbookType::System, None);
    seed_wordbook(store, "usr1", "My Words", WordbookType::User, Some("u1"));
    seed_word(store, "w1", "apple", "苹果", Some("noun"), 0.3);
    seed_word(store, "w2", "banana", "香蕉", Some("noun"), 0.7);
    store.add_word_to_wordbook("sys1", "w1").unwrap();
    store.add_word_to_wordbook("sys1", "w2").unwrap();
    store.add_word_to_wordbook("usr1", "w1").unwrap();
    seed_state(store, "ua", "w1", 0.5, 3);
    seed_state(store, "ub", "w1", 0.9, 2);

    // 未鉴权 -> 401
    let resp = request(&app.app, Method::GET, "/api/admin/wordbooks", None, &[]).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // 全量列表
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/wordbooks?type=all&sort=newest&page=1&perPage=20",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    let data = &body["data"];
    assert_eq!(data["total"].as_u64().unwrap(), 2);
    assert_eq!(data["page"].as_u64().unwrap(), 1);
    assert_eq!(data["perPage"].as_u64().unwrap(), 20);
    assert_eq!(data["counts"]["all"].as_u64().unwrap(), 2);
    assert_eq!(data["counts"]["system"].as_u64().unwrap(), 1);
    assert_eq!(data["counts"]["user"].as_u64().unwrap(), 1);
    assert_eq!(data["counts"]["totalEntries"].as_u64().unwrap(), 3);

    let items = data["items"].as_array().expect("items array");
    assert_eq!(items.len(), 2);
    // 字段契约
    let sys = items.iter().find(|i| i["id"] == "sys1").expect("sys1");
    assert_eq!(sys["type"], "system");
    assert_eq!(sys["wordCount"].as_u64().unwrap(), 2);
    assert_eq!(sys["activeUsers"].as_u64().unwrap(), 2);
    assert!(sys["tags"].is_array());
    assert!(sys["createdAt"].is_string());
    let usr = items.iter().find(|i| i["id"] == "usr1").expect("usr1");
    assert_eq!(usr["type"], "user");
    assert_eq!(usr["userId"], "u1");
    assert_eq!(usr["ownerEmail"], "owner@wb.test");

    // type 过滤
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/wordbooks?type=system",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (_, _, body) = response_json(resp).await;
    assert_eq!(body["data"]["total"].as_u64().unwrap(), 1);
    assert_eq!(body["data"]["items"][0]["id"], "sys1");

    // search(大小写不敏感)
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/wordbooks?search=gre",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (_, _, body) = response_json(resp).await;
    assert_eq!(body["data"]["total"].as_u64().unwrap(), 1);
    assert_eq!(body["data"]["items"][0]["id"], "sys1");
}

/// stats 卡:totalWords / activeUsers / avgMastery / weeklyAnswers。
#[tokio::test]
async fn it_wordbook_stats() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let store = app.state.store();

    seed_wordbook(store, "sys1", "B", WordbookType::System, None);
    seed_word(store, "w1", "a", "释义", None, 0.5);
    seed_word(store, "w2", "b", "释义", None, 0.5);
    store.add_word_to_wordbook("sys1", "w1").unwrap();
    store.add_word_to_wordbook("sys1", "w2").unwrap();
    seed_state(store, "u1", "w1", 0.4, 2);
    seed_state(store, "u1", "w2", 0.8, 2);
    seed_record(store, "u1", "w1", true);
    seed_record(store, "u1", "w2", false);

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/wordbooks/sys1/stats",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    let d = &body["data"];
    assert_eq!(d["wordbookId"], "sys1");
    assert_eq!(d["totalWords"].as_u64().unwrap(), 2);
    assert_eq!(d["activeUsers"].as_u64().unwrap(), 1);
    assert!((d["avgMastery"].as_f64().unwrap() - 0.6).abs() < 1e-6);
    assert_eq!(d["weeklyAnswers"].as_u64().unwrap(), 2);
}

/// words 列表:paginated 形状 + frequency/alpha 排序 + pos/search 过滤 + 字段契约。
#[tokio::test]
async fn it_list_words_sort_and_search() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let store = app.state.store();

    seed_wordbook(store, "sys1", "B", WordbookType::System, None);
    seed_word(store, "w1", "alpha", "第一", Some("noun"), 0.2);
    seed_word(store, "w2", "beta", "第二", Some("verb"), 0.9);
    store.add_word_to_wordbook("sys1", "w1").unwrap();
    store.add_word_to_wordbook("sys1", "w2").unwrap();
    // w2 出现 3 次(2对1错),w1 出现 1 次
    seed_record(store, "u1", "w2", true);
    seed_record(store, "u1", "w2", true);
    seed_record(store, "u1", "w2", false);
    seed_record(store, "u1", "w1", true);

    // frequency 排序(默认)
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/wordbooks/sys1/words?sort=frequency&page=1&perPage=10",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    // paginated 顶层形状
    assert_eq!(body["data"]["total"].as_u64().unwrap(), 2);
    assert_eq!(body["data"]["page"].as_u64().unwrap(), 1);
    assert_eq!(body["data"]["perPage"].as_u64().unwrap(), 10);
    assert_eq!(body["data"]["totalPages"].as_u64().unwrap(), 1);
    let rows = body["data"]["data"].as_array().expect("rows");
    assert_eq!(rows[0]["id"], "w2"); // 高频在前
    assert_eq!(rows[0]["appearCount"].as_u64().unwrap(), 3);
    assert!((rows[0]["accuracy"].as_f64().unwrap() - 2.0 / 3.0).abs() < 1e-6);
    // 字段契约
    assert_eq!(rows[0]["text"], "beta");
    assert_eq!(rows[0]["partOfSpeech"], "verb");
    assert!(rows[0]["examples"].is_array());

    // alpha 排序
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/wordbooks/sys1/words?sort=alpha",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (_, _, body) = response_json(resp).await;
    assert_eq!(body["data"]["data"][0]["id"], "w1");

    // pos 过滤
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/wordbooks/sys1/words?pos=verb",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (_, _, body) = response_json(resp).await;
    assert_eq!(body["data"]["total"].as_u64().unwrap(), 1);
    assert_eq!(body["data"]["data"][0]["id"], "w2");

    // search 命中 meaning
    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/wordbooks/sys1/words?search=%E7%AC%AC%E4%B8%80",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (_, _, body) = response_json(resp).await;
    assert_eq!(body["data"]["total"].as_u64().unwrap(), 1);
    assert_eq!(body["data"]["data"][0]["id"], "w1");
}

/// heatmap + user-distribution 形状。
#[tokio::test]
async fn it_heatmap_and_distribution() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;
    let store = app.state.store();

    seed_wordbook(store, "sys1", "B", WordbookType::System, None);
    seed_word(store, "w1", "a", "释义", None, 0.5);
    seed_word(store, "w2", "b", "释义", None, 0.5);
    store.add_word_to_wordbook("sys1", "w1").unwrap();
    store.add_word_to_wordbook("sys1", "w2").unwrap();
    seed_record(store, "u1", "w1", true);
    seed_record(store, "u1", "w1", true);
    seed_record(store, "u1", "w2", true);
    seed_state(store, "u1", "w1", 0.1, 2);
    seed_state(store, "u2", "w2", 0.85, 2);

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/wordbooks/sys1/heatmap?limit=600",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["data"]["wordbookId"], "sys1");
    assert_eq!(body["data"]["maxCount"].as_u64().unwrap(), 2);
    let cells = body["data"]["cells"].as_array().expect("cells");
    assert_eq!(cells[0]["wordId"], "w1");
    assert_eq!(cells[0]["count"].as_u64().unwrap(), 2);

    let resp = request(
        &app.app,
        Method::GET,
        "/api/admin/wordbooks/sys1/user-distribution",
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["totalUsers"].as_u64().unwrap(), 2);
    let buckets = body["data"]["buckets"].as_array().expect("buckets");
    assert_eq!(buckets.len(), 5);
    assert_eq!(buckets[0]["userCount"].as_u64().unwrap(), 1); // 0-20%
    assert_eq!(buckets[4]["userCount"].as_u64().unwrap(), 1); // 80-100%
    assert!(buckets[0]["label"].is_string());
}

/// 完整生命周期:create -> patch -> add-word -> remove-word -> history 审计 -> export -> delete。
#[tokio::test]
async fn it_wordbook_lifecycle_and_history_and_export() {
    let app = spawn_test_server().await;
    let token = setup_admin_and_get_token(&app.app).await;

    // create
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/wordbooks",
        Some(serde_json::json!({ "name": "测试词库", "description": "desc", "type": "system" })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CREATED, "body={body}");
    assert_eq!(body["data"]["name"], "测试词库");
    assert_eq!(body["data"]["type"], "system");
    assert_eq!(body["data"]["wordCount"].as_u64().unwrap(), 0);
    let wb_id = body["data"]["id"].as_str().expect("id").to_string();

    // create 空名 -> 400
    let resp = request(
        &app.app,
        Method::POST,
        "/api/admin/wordbooks",
        Some(serde_json::json!({ "name": "   " })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // patch name + description
    let resp = request(
        &app.app,
        Method::PATCH,
        &format!("/api/admin/wordbooks/{wb_id}"),
        Some(serde_json::json!({ "name": "改名后", "description": "新描述" })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["data"]["name"], "改名后");
    assert_eq!(body["data"]["description"], "新描述");

    // patch 不存在的 -> 404
    let resp = request(
        &app.app,
        Method::PATCH,
        "/api/admin/wordbooks/nonexistent-id",
        Some(serde_json::json!({ "name": "x" })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // add-word
    let resp = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/wordbooks/{wb_id}/words"),
        Some(serde_json::json!({
            "text": "serendipity",
            "pronunciation": "/ˌserənˈdipədē/",
            "partOfSpeech": "noun",
            "meaning": "意外的好运",
            "examples": ["A fortunate serendipity."]
        })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::CREATED, "body={body}");
    assert_eq!(body["data"]["text"], "serendipity");
    assert_eq!(body["data"]["partOfSpeech"], "noun");
    assert_eq!(body["data"]["appearCount"].as_u64().unwrap(), 0);
    let word_id = body["data"]["id"].as_str().expect("word id").to_string();

    // add-word 缺 meaning -> 400
    let resp = request(
        &app.app,
        Method::POST,
        &format!("/api/admin/wordbooks/{wb_id}/words"),
        Some(serde_json::json!({ "text": "x", "meaning": "" })),
        &[("authorization", auth_header(&token))],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // 词加进去后,列表 wordCount 应为 1
    let resp = request(
        &app.app,
        Method::GET,
        &format!("/api/admin/wordbooks/{wb_id}/words"),
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (_, _, body) = response_json(resp).await;
    assert_eq!(body["data"]["total"].as_u64().unwrap(), 1);

    // export(在 remove 之前,词条存在)
    let resp = request(
        &app.app,
        Method::GET,
        &format!("/api/admin/wordbooks/{wb_id}/export"),
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    let d = &body["data"];
    assert_eq!(d["id"], wb_id);
    assert_eq!(d["name"], "改名后");
    assert_eq!(d["type"], "system");
    assert_eq!(d["version"], "");
    let words = d["words"].as_array().expect("words");
    assert_eq!(words.len(), 1);
    assert_eq!(words[0]["spelling"], "serendipity");
    assert_eq!(words[0]["phonetic"], "/ˌserənˈdipədē/");
    assert_eq!(words[0]["partOfSpeech"], "noun");
    assert_eq!(words[0]["meaning"], "意外的好运");
    assert!(words[0]["examples"].is_array());

    // remove-word
    let resp = request(
        &app.app,
        Method::DELETE,
        &format!("/api/admin/wordbooks/{wb_id}/words/{word_id}"),
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["data"]["removed"], true);

    // history:应记录 create / update / add_word / remove_word 审计
    let resp = request(
        &app.app,
        Method::GET,
        &format!("/api/admin/wordbooks/{wb_id}/history?page=1&perPage=50"),
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    let entries = body["data"]["data"].as_array().expect("history rows");
    assert!(body["data"]["total"].as_u64().unwrap() >= 4);
    let actions: Vec<&str> = entries
        .iter()
        .filter_map(|e| e["action"].as_str())
        .collect();
    assert!(actions.contains(&"create"));
    assert!(actions.contains(&"update"));
    assert!(actions.contains(&"add_word"));
    assert!(actions.contains(&"remove_word"));
    // 审计字段契约
    let first = &entries[0];
    assert_eq!(first["wordbookId"], wb_id);
    assert!(first["id"].is_string());
    assert!(first["adminId"].is_string());
    assert!(first["createdAt"].is_string());

    // delete
    let resp = request(
        &app.app,
        Method::DELETE,
        &format!("/api/admin/wordbooks/{wb_id}"),
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    let (status, _, body) = response_json(resp).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["data"]["deleted"], true);

    // delete 再删 -> 404
    let resp = request(
        &app.app,
        Method::DELETE,
        &format!("/api/admin/wordbooks/{wb_id}"),
        None,
        &[("authorization", auth_header(&token))],
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
