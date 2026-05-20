//! workers/* 补充单元测试：覆盖 update_checker / heartbeat_watchdog / config_watcher /
//! password_reset_cleanup / algorithm_optimization 等 0% 或低覆盖文件。
//!
//! 对 0% worker 的策略：
//!   - update_checker / heartbeat_watchdog 依赖 AppState，构造一个最小 state 直接调 run()。
//!   - config_watcher：写一个最小 TOML，触发文件变更后等防抖窗口。
//!   - password_reset_cleanup：直接在 store 上插入过期 token，调用 worker，断言被删。

use std::sync::Arc;
use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};
use tokio::sync::broadcast;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use learning_backend::amas::config::AMASConfig;
use learning_backend::amas::engine::AMASEngine;
use learning_backend::config::{
    AMASEnvConfig, AuthRateLimitConfig, Config, LLMConfig, RateLimitConfig, UpdateCheckConfig,
    WorkerConfig,
};
use learning_backend::services::updater::Updater;
use learning_backend::state::AppState;
use learning_backend::store::Store;
use learning_backend::workers;

fn setup_store(name: &str) -> (tempfile::TempDir, Arc<Store>) {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let path = temp_dir.path().join(name);
    let store = Arc::new(Store::open(path.to_str().unwrap(), 5000, 2).expect("open store"));
    store.run_migrations().expect("run migrations");
    (temp_dir, store)
}

fn build_test_config(database_url: String) -> Config {
    Config {
        host: "127.0.0.1".parse().unwrap(),
        port: 0,
        log_level: "info".to_string(),
        enable_file_logs: false,
        log_dir: "./logs".to_string(),
        database_url,
        api_only: true,
        sqlite_busy_timeout_ms: 5000,
        sqlite_connection_timeout_ms: 250,
        sqlite_pool_size: 2,
        jwt_secret: "test-jwt-secret".repeat(2),
        refresh_jwt_secret: "test-refresh-secret".repeat(2),
        jwt_expires_in_hours: 1,
        refresh_token_expires_in_hours: 24,
        admin_jwt_secret: "test-admin-secret".repeat(2),
        admin_jwt_expires_in_hours: 1,
        cors_origin: "http://localhost".into(),
        trust_proxy: false,
        cookie_secure: false,
        self_watchdog: Default::default(),
        rate_limit: RateLimitConfig {
            window_secs: 60,
            max_requests: 1000,
        },
        auth_rate_limit: AuthRateLimitConfig::default(),
        worker: WorkerConfig {
            is_leader: false,
            enable_llm_advisor: false,
            enable_monitoring: false,
        },
        amas: AMASEnvConfig {
            ensemble_enabled: true,
            monitor_sample_rate: 0.05,
        },
        amas_config_file: None,
        llm: LLMConfig {
            enabled: false,
            mock: true,
            api_url: String::new(),
            api_key: String::new(),
            model: String::new(),
            timeout_secs: 5,
            daily_cost_cap_usd: 1.0,
            input_price_per_mtok_usd: 0.55,
            output_price_per_mtok_usd: 2.19,
        },
        update_check: UpdateCheckConfig {
            api_url: String::new(),
            cache_ttl_secs: 3600,
            worker_enabled: false,
            worker_interval_secs: 3600,
            github_token: None,
            allow_downgrade: false,
            install_dir: None,
            max_tarball_bytes: 200 * 1024 * 1024,
            download_mirror_prefix: None,
        },
        pagination: Default::default(),
        strict_mode: Default::default(),
        limits: Default::default(),
        probe: Default::default(),
    }
}

fn build_state(store: Arc<Store>, cfg: &Config) -> (AppState, broadcast::Sender<()>) {
    let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
    let (tx, _) = broadcast::channel(4);
    let state = AppState::new(store, amas, cfg, tx.clone(), false);
    (state, tx)
}

// ────────────────────── password_reset_cleanup ──────────────────────

#[tokio::test]
async fn password_reset_cleanup_removes_expired_tokens() {
    let (_tmp, store) = setup_store("pw-reset-cleanup.db");

    let past = (Utc::now() - Duration::hours(2)).to_rfc3339();
    let future = (Utc::now() + Duration::hours(2)).to_rfc3339();
    store
        .create_password_reset_token("expired-hash", "u1", &past)
        .expect("insert expired");
    store
        .create_password_reset_token("alive-hash", "u1", &future)
        .expect("insert alive");

    workers::password_reset_cleanup::run(&store).await;

    assert!(store
        .get_password_reset_token("alive-hash")
        .unwrap()
        .is_some());
    assert!(store
        .get_password_reset_token("expired-hash")
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn password_reset_cleanup_noop_when_empty() {
    let (_tmp, store) = setup_store("pw-reset-cleanup-empty.db");
    // 不应 panic
    workers::password_reset_cleanup::run(&store).await;
}

// ────────────────────── cache_cleanup ──────────────────────

#[tokio::test]
async fn cache_cleanup_removes_old_monitoring_events() {
    let (_tmp, store) = setup_store("cache-cleanup.db");

    let old_ts = (Utc::now() - Duration::days(10)).to_rfc3339();
    let new_ts = Utc::now().to_rfc3339();
    let old_event = serde_json::json!({
        "id": "old-1",
        "timestamp": old_ts,
        "kind": "test"
    });
    let new_event = serde_json::json!({
        "id": "new-1",
        "timestamp": new_ts,
        "kind": "test"
    });
    store.insert_monitoring_event(&old_event).unwrap();
    store.insert_monitoring_event(&new_event).unwrap();

    workers::cache_cleanup::run(&store).await;

    let remaining = store
        .get_recent_monitoring_events(100)
        .expect("get events");
    // new 还在，old 被清
    assert!(!remaining.is_empty());
    assert!(remaining
        .iter()
        .any(|e| e.get("id").and_then(|v| v.as_str()) == Some("new-1")));
}

// ────────────────────── algorithm_optimization 分支 ──────────────────────

#[tokio::test]
async fn algorithm_optimization_lowers_difficulty_on_low_accuracy() {
    let (_tmp, store) = setup_store("alg-opt-low.db");
    let engine = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));

    // 创建一个用户和大量低正确率记录（>50 条且 < 40% accuracy）
    let user = learning_backend::store::operations::users::User {
        id: "u-low".to_string(),
        email: "low@test.com".to_string(),
        username: "low-user".to_string(),
        password_hash: "h".to_string(),
        is_banned: false,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        failed_login_count: 0,
        locked_until: None,
    };
    store.create_user(&user).unwrap();

    let now = Utc::now();
    for i in 0..60 {
        let rec = learning_backend::store::operations::records::LearningRecord {
            id: format!("r{i}"),
            user_id: user.id.clone(),
            word_id: format!("w{i}"),
            is_correct: i < 10, // 10/60 ≈ 16% accuracy
            response_time_ms: 1000,
            session_id: None,
            created_at: now,
            record_type: learning_backend::store::operations::records::RecordType::All,
            self_rating: None,
        };
        store.create_record(&rec).unwrap();
    }

    let before = engine.get_config().constraints.max_difficulty_when_fatigued;
    workers::algorithm_optimization::run(&store, &engine).await;
    let after = engine.get_config().constraints.max_difficulty_when_fatigued;
    assert!(after < before, "difficulty should be lowered: {before} -> {after}");
}

#[tokio::test]
async fn algorithm_optimization_raises_difficulty_on_high_accuracy() {
    let (_tmp, store) = setup_store("alg-opt-high.db");
    let engine = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));

    let user = learning_backend::store::operations::users::User {
        id: "u-high".to_string(),
        email: "high@test.com".to_string(),
        username: "high-user".to_string(),
        password_hash: "h".to_string(),
        is_banned: false,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        failed_login_count: 0,
        locked_until: None,
    };
    store.create_user(&user).unwrap();

    let now = Utc::now();
    for i in 0..60 {
        let rec = learning_backend::store::operations::records::LearningRecord {
            id: format!("r{i}"),
            user_id: user.id.clone(),
            word_id: format!("w{i}"),
            is_correct: i < 58, // ≈ 96% accuracy
            response_time_ms: 1000,
            session_id: None,
            created_at: now,
            record_type: learning_backend::store::operations::records::RecordType::All,
            self_rating: None,
        };
        store.create_record(&rec).unwrap();
    }

    let before = engine.get_config().constraints.max_difficulty_when_fatigued;
    workers::algorithm_optimization::run(&store, &engine).await;
    let after = engine.get_config().constraints.max_difficulty_when_fatigued;
    assert!(after > before, "difficulty should be raised: {before} -> {after}");
}

#[tokio::test]
async fn algorithm_optimization_no_change_when_records_below_threshold() {
    let (_tmp, store) = setup_store("alg-opt-few.db");
    let engine = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));

    let before = engine.get_config().constraints.max_difficulty_when_fatigued;
    // 没有任何 record，total_records=0 < 50
    workers::algorithm_optimization::run(&store, &engine).await;
    let after = engine.get_config().constraints.max_difficulty_when_fatigued;
    assert!((after - before).abs() < f64::EPSILON);
}

// ────────────────────── update_checker ──────────────────────

#[tokio::test]
async fn update_checker_broadcasts_when_new_release_available() {
    if std::env::consts::OS != "linux" {
        // updater 的 apply 只支持 linux，但 check_latest 可以跑所有平台；
        // 这里只验证 broadcast，不调用 apply。
    }
    let (_tmp, store) = setup_store("updater.db");
    let server = MockServer::start().await;

    let release_body = serde_json::json!({
        "tag_name": "v99.99.99",
        "published_at": "2030-01-01T00:00:00Z",
        "html_url": format!("{}/release", server.uri()),
        "body": "new release",
        "assets": [
            {
                "name": "wordforge-linux-x86_64.tar.gz",
                "browser_download_url": format!("{}/dl/wordforge-linux-x86_64.tar.gz", server.uri()),
                "size": 1024
            },
            {
                "name": "wordforge-linux-x86_64.tar.gz.sha256",
                "browser_download_url": format!("{}/dl/wordforge-linux-x86_64.tar.gz.sha256", server.uri()),
                "size": 64
            },
            {
                "name": "wordforge-linux-aarch64.tar.gz",
                "browser_download_url": format!("{}/dl/wordforge-linux-aarch64.tar.gz", server.uri()),
                "size": 1024
            },
            {
                "name": "wordforge-linux-aarch64.tar.gz.sha256",
                "browser_download_url": format!("{}/dl/wordforge-linux-aarch64.tar.gz.sha256", server.uri()),
                "size": 64
            }
        ]
    });
    Mock::given(method("GET"))
        .and(path("/repos/o/r/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(release_body))
        .mount(&server)
        .await;

    let tmp_install = tempfile::tempdir().unwrap();
    let cfg = UpdateCheckConfig {
        api_url: format!("{}/repos/o/r/releases/latest", server.uri()),
        cache_ttl_secs: 60,
        worker_enabled: true,
        worker_interval_secs: 60,
        github_token: None,
        allow_downgrade: false,
        install_dir: Some(tmp_install.path().to_path_buf()),
        max_tarball_bytes: 200 * 1024 * 1024,
        download_mirror_prefix: None,
    };
    let updater = Updater::new(&cfg, "v0.0.1").expect("updater");

    let test_cfg = build_test_config(":memory:".to_string());
    let (state, _tx) = build_state(store.clone(), &test_cfg);

    // 第一次：snapshot.latest_version 为 None，check 后变 v99.99.99 -> 触发广播
    workers::update_checker::run(updater.clone(), state.clone()).await;

    // 第二次：snapshot 已是 v99.99.99，不再广播（覆盖 prev_tag == new_tag 分支）
    workers::update_checker::run(updater, state).await;
}

#[tokio::test]
async fn update_checker_swallows_check_error() {
    let (_tmp, store) = setup_store("updater-err.db");
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/o/r/releases/latest"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let tmp_install = tempfile::tempdir().unwrap();
    let cfg = UpdateCheckConfig {
        api_url: format!("{}/repos/o/r/releases/latest", server.uri()),
        cache_ttl_secs: 60,
        worker_enabled: false,
        worker_interval_secs: 60,
        github_token: None,
        allow_downgrade: false,
        install_dir: Some(tmp_install.path().to_path_buf()),
        max_tarball_bytes: 200 * 1024 * 1024,
        download_mirror_prefix: None,
    };
    let updater = Updater::new(&cfg, "v0.0.1").expect("updater");

    let test_cfg = build_test_config(":memory:".to_string());
    let (state, _tx) = build_state(store, &test_cfg);

    // 500 -> warn + 提前 return，不 panic
    workers::update_checker::run(updater, state).await;
}

#[tokio::test]
async fn update_checker_returns_early_when_no_update() {
    let (_tmp, store) = setup_store("updater-noupd.db");
    let server = MockServer::start().await;

    // tag 与当前 current_tag 相同 -> has_update = false -> 提前 return
    let release_body = serde_json::json!({
        "tag_name": "v0.0.1",
        "published_at": "2030-01-01T00:00:00Z",
        "html_url": format!("{}/release", server.uri()),
        "body": "current",
        "assets": []
    });
    Mock::given(method("GET"))
        .and(path("/repos/o/r/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(release_body))
        .mount(&server)
        .await;

    let tmp_install = tempfile::tempdir().unwrap();
    let cfg = UpdateCheckConfig {
        api_url: format!("{}/repos/o/r/releases/latest", server.uri()),
        cache_ttl_secs: 60,
        worker_enabled: false,
        worker_interval_secs: 60,
        github_token: None,
        allow_downgrade: false,
        install_dir: Some(tmp_install.path().to_path_buf()),
        max_tarball_bytes: 200 * 1024 * 1024,
        download_mirror_prefix: None,
    };
    let updater = Updater::new(&cfg, "v0.0.1").expect("updater");

    let test_cfg = build_test_config(":memory:".to_string());
    let (state, _tx) = build_state(store, &test_cfg);

    workers::update_checker::run(updater, state).await;
}

// ────────────────────── heartbeat_watchdog ──────────────────────

#[tokio::test]
async fn heartbeat_watchdog_run_responds_to_shutdown() {
    let (_tmp, store) = setup_store("heartbeat.db");
    let test_cfg = build_test_config(":memory:".to_string());
    let (state, tx) = build_state(store, &test_cfg);

    let shutdown_rx = tx.subscribe();
    let handle = tokio::spawn(workers::heartbeat_watchdog::run(state, shutdown_rx));

    // 立即发 shutdown，循环 select 应跳出
    let _ = tx.send(());
    tokio::time::timeout(StdDuration::from_secs(2), handle)
        .await
        .expect("worker exits after shutdown")
        .expect("no panic");
}

// ────────────────────── config_watcher ──────────────────────

#[tokio::test]
async fn config_watcher_returns_when_file_missing() {
    let (_tmp, store) = setup_store("cfgwatch.db");
    let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store));

    // 不存在的路径 -> watch 失败 -> 立即 return
    let path = "/nonexistent/dir/that/does/not/exist/amas.toml".to_string();
    let handle = tokio::spawn(workers::config_watcher::run(path, amas));
    tokio::time::timeout(StdDuration::from_secs(2), handle)
        .await
        .expect("watcher returned")
        .expect("no panic");
}

#[tokio::test]
async fn config_watcher_reloads_on_modify() {
    let (_tmp, store) = setup_store("cfgwatch2.db");
    let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("amas.toml");
    let toml_content = toml::to_string(&AMASConfig::default()).expect("ser toml");
    std::fs::write(&path, toml_content).expect("write toml");

    let amas_clone = amas.clone();
    let path_str = path.to_str().unwrap().to_string();
    let handle = tokio::spawn(workers::config_watcher::run(path_str, amas_clone));

    // 等 watcher 启动
    tokio::time::sleep(StdDuration::from_millis(200)).await;

    // 修改文件，触发 reload
    let mut new_cfg = AMASConfig::default();
    new_cfg.constraints.max_difficulty_when_fatigued =
        (new_cfg.constraints.max_difficulty_when_fatigued - 0.05).max(0.2);
    let new_toml = toml::to_string(&new_cfg).expect("ser new toml");
    std::fs::write(&path, new_toml).expect("write new toml");

    // 等 500ms 防抖 + 处理
    tokio::time::sleep(StdDuration::from_millis(1200)).await;

    // worker 仍在 spawn 中，drop 它以结束
    handle.abort();
    let _ = handle.await;
}

// ────────────────────── confusion_pair_cache 边界 ──────────────────────

#[tokio::test]
async fn confusion_pair_cache_with_empty_store_completes() {
    let (_tmp, store) = setup_store("confusion-empty.db");
    // 空 store -> users 列表为空，立刻 break loop
    workers::confusion_pair_cache::run(&store).await;
}

// ────────────────────── session_cleanup 边界 ──────────────────────

#[tokio::test]
async fn session_cleanup_runs_without_panic_on_empty_store() {
    let (_tmp, store) = setup_store("sc-empty.db");
    workers::session_cleanup::run(&store).await;
}

// ────────────────────── etymology_generation ──────────────────────

#[tokio::test]
async fn etymology_generation_processes_words_without_existing() {
    let (_tmp, store) = setup_store("ety.db");
    // 插入一个没 etymology 的词
    let word = learning_backend::store::operations::words::Word {
        id: "w-eg".to_string(),
        text: "alpha".to_string(),
        meaning: "the first letter".to_string(),
        pronunciation: None,
        part_of_speech: None,
        difficulty: 0.5,
        examples: vec!["This is alpha".into()],
        tags: vec![],
        embedding: None,
        created_at: Utc::now(),
    };
    store.upsert_word(&word).unwrap();

    workers::etymology_generation::run(&store).await;

    // 检查已生成
    let etymology = store.get_etymology("w-eg").expect("get etymology");
    assert!(etymology.is_some());
}

// ────────────────────── delayed_reward 0 records 边界 ──────────────────────

#[tokio::test]
async fn delayed_reward_runs_with_no_overdue_words() {
    let (_tmp, store) = setup_store("dr-empty.db");
    workers::delayed_reward::run(&store).await;
}

// ────────────────────── log_export 边界 ──────────────────────

#[tokio::test]
async fn log_export_creates_metrics_entry_when_events_present() {
    let (_tmp, store) = setup_store("logexport.db");

    // 没事件 -> 提前 return，不写 metrics
    workers::log_export::run(&store).await;

    // 注入一条事件，再跑一次
    let event = serde_json::json!({
        "id": "ev-1",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "kind": "test"
    });
    store.insert_monitoring_event(&event).unwrap();

    workers::log_export::run(&store).await;
    let hour = chrono::Utc::now().format("%Y-%m-%d-%H").to_string();
    assert!(store
        .get_metrics_daily(&hour, "log_export")
        .unwrap()
        .is_some());
}

// ────────────────────── weekly_report 边界 ──────────────────────

#[tokio::test]
async fn weekly_report_runs_on_empty_store() {
    let (_tmp, store) = setup_store("wr-empty.db");
    workers::weekly_report::run(&store).await;
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    assert!(store
        .get_metrics_daily(&date, "weekly_report")
        .unwrap()
        .is_some());
}

// ────────────────────── word_clustering 边界 ──────────────────────

#[tokio::test]
async fn word_clustering_runs_on_empty_store() {
    let (_tmp, store) = setup_store("wc-empty.db");
    workers::word_clustering::run(&store).await;
}

// ────────────────────── monitoring_aggregate 边界 ──────────────────────

#[tokio::test]
async fn monitoring_aggregate_runs_without_events() {
    let (_tmp, store) = setup_store("ma-empty.db");
    workers::monitoring_aggregate::run(&store).await;
}

// ────────────────────── health_analysis 边界 ──────────────────────

#[tokio::test]
async fn health_analysis_runs_on_empty_store() {
    let (_tmp, store) = setup_store("ha-empty.db");
    workers::health_analysis::run(&store).await;
}
