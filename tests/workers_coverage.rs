use std::sync::Arc;
use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};
use tokio::sync::broadcast;

use learning_backend::amas::config::AMASConfig;
use learning_backend::amas::engine::AMASEngine;
use learning_backend::amas::memory::{evm, iad, mtp};
use learning_backend::amas::metrics::MetricsRegistry;
use learning_backend::amas::types::AlgorithmId;
use learning_backend::config::Config;
use learning_backend::store::keys;
use learning_backend::store::operations::records::LearningRecord;
use learning_backend::store::operations::sessions::Session;
use learning_backend::store::operations::users::User;
use learning_backend::store::operations::word_states::{WordLearningState, WordState};
use learning_backend::store::operations::words::Word;
use learning_backend::store::Store;
use learning_backend::workers;

fn setup_store(db_name: &str) -> (tempfile::TempDir, Arc<Store>) {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let db_path = temp_dir.path().join(db_name);
    let store = Arc::new(Store::open(db_path.to_str().expect("db path")).expect("open store"));
    (temp_dir, store)
}

fn sample_user(id: &str, email: &str) -> User {
    User {
        id: id.to_string(),
        email: email.to_string(),
        username: format!("user-{id}"),
        password_hash: "hash".to_string(),
        is_banned: false,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        failed_login_count: 0,
        locked_until: None,
    }
}

fn sample_word(
    id: &str,
    text: &str,
    difficulty: f64,
    embedding: Option<Vec<f64>>,
    tags: Vec<&str>,
) -> Word {
    Word {
        id: id.to_string(),
        text: text.to_string(),
        meaning: format!("meaning-{text}"),
        pronunciation: None,
        part_of_speech: None,
        difficulty,
        examples: vec![format!("example-{text}")],
        tags: tags.into_iter().map(|t| t.to_string()).collect(),
        embedding,
        created_at: Utc::now(),
    }
}

fn sample_record(
    id: &str,
    user_id: &str,
    word_id: &str,
    is_correct: bool,
    created_at: chrono::DateTime<Utc>,
) -> LearningRecord {
    LearningRecord {
        id: id.to_string(),
        user_id: user_id.to_string(),
        word_id: word_id.to_string(),
        is_correct,
        response_time_ms: 900,
        session_id: Some("session-1".to_string()),
        created_at,
    }
}

fn sample_word_state(
    user_id: &str,
    word_id: &str,
    state: WordState,
    next_review_date: Option<chrono::DateTime<Utc>>,
) -> WordLearningState {
    WordLearningState {
        user_id: user_id.to_string(),
        word_id: word_id.to_string(),
        state,
        mastery_level: 0.5,
        next_review_date,
        half_life: 2.0,
        correct_streak: 1,
        total_attempts: 3,
        updated_at: Utc::now(),
    }
}

fn sample_session(
    token_hash: &str,
    user_id: &str,
    expires_in_hours: i64,
    revoked: bool,
) -> Session {
    Session {
        token_hash: token_hash.to_string(),
        user_id: user_id.to_string(),
        token_type: "user".to_string(),
        created_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(expires_in_hours),
        revoked,
    }
}

#[tokio::test]
async fn it_worker_manager_registers_jobs_and_shutdowns() {
    let (_tmp, store) = setup_store("worker-manager.sled");
    let engine = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
    let (shutdown_tx, _) = broadcast::channel::<()>(8);

    let mut worker_cfg = Config::from_env().worker;
    worker_cfg.is_leader = true;
    worker_cfg.enable_monitoring = true;
    worker_cfg.enable_llm_advisor = true;

    let manager = workers::WorkerManager::new(
        store.clone(),
        engine.clone(),
        shutdown_tx.subscribe(),
        &worker_cfg,
    );

    let jobs = manager.planned_jobs();
    assert!(!jobs.is_empty());
    assert!(jobs
        .iter()
        .any(|j| j.name == workers::WorkerName::MetricsFlush && j.enabled));
    // MonitoringAggregate and LlmAdvisor are now stub-disabled by default
    assert!(jobs
        .iter()
        .any(|j| j.name == workers::WorkerName::MonitoringAggregate && !j.enabled));
    assert!(jobs
        .iter()
        .any(|j| j.name == workers::WorkerName::LlmAdvisor && !j.enabled));

    let mut worker_cfg_without_optional = worker_cfg.clone();
    worker_cfg_without_optional.enable_monitoring = false;
    worker_cfg_without_optional.enable_llm_advisor = false;

    let manager_without_optional = workers::WorkerManager::new(
        store.clone(),
        engine.clone(),
        shutdown_tx.subscribe(),
        &worker_cfg_without_optional,
    );
    let jobs_without_optional = manager_without_optional.planned_jobs();
    assert!(jobs_without_optional
        .iter()
        .any(|j| j.name == workers::WorkerName::MetricsFlush && !j.enabled));
    assert!(jobs_without_optional
        .iter()
        .any(|j| j.name == workers::WorkerName::MonitoringAggregate && !j.enabled));
    assert!(jobs_without_optional
        .iter()
        .any(|j| j.name == workers::WorkerName::LlmAdvisor && !j.enabled));

    let start_outcome = tokio::time::timeout(StdDuration::from_secs(2), manager.start()).await;
    assert!(
        start_outcome.is_err(),
        "leader worker manager start should wait for shutdown signal"
    );
}

#[tokio::test]
async fn it_runs_worker_tasks_and_persists_side_effects() {
    let (_tmp, store) = setup_store("workers-side-effects.sled");
    let engine = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));

    workers::embedding_generation::run(store.as_ref()).await;

    let user_1 = sample_user("u1", "u1@test.com");
    let user_2 = sample_user("u2", "u2@test.com");
    store.create_user(&user_1).expect("create user_1");
    store.create_user(&user_2).expect("create user_2");

    let word_easy = sample_word("w1", "alpha", 0.2, None, vec!["basic", "seed"]);
    let word_mid = sample_word("w2", "beta", 0.5, None, vec!["basic"]);
    let word_hard = sample_word("w3", "gamma", 0.9, Some(vec![0.1, 0.2]), vec!["advanced"]);

    store.upsert_word(&word_easy).expect("upsert easy");
    store.upsert_word(&word_mid).expect("upsert mid");
    store.upsert_word(&word_hard).expect("upsert hard");

    let existing_etymology = serde_json::json!({
        "wordId": word_mid.id,
        "word": word_mid.text,
        "etymology": "pre-seeded",
        "generated": false
    });
    store
        .etymologies
        .insert(
            keys::etymology_key("w2").unwrap().as_bytes(),
            serde_json::to_vec(&existing_etymology).expect("etymology bytes"),
        )
        .expect("insert existing etymology");

    let now = Utc::now();
    store
        .create_record(&sample_record(
            "r1",
            &user_1.id,
            &word_easy.id,
            false,
            now - Duration::minutes(2),
        ))
        .expect("create r1");
    store
        .create_record(&sample_record(
            "r2",
            &user_1.id,
            &word_mid.id,
            false,
            now - Duration::minutes(1),
        ))
        .expect("create r2");
    store
        .create_record(&sample_record("r3", &user_1.id, &word_hard.id, true, now))
        .expect("create r3");
    store
        .create_record(&sample_record(
            "r4",
            &user_2.id,
            &word_easy.id,
            false,
            now - Duration::days(8),
        ))
        .expect("create r4");

    store
        .set_word_learning_state(&sample_word_state(
            &user_1.id,
            &word_easy.id,
            WordState::Learning,
            Some(now - Duration::hours(72)),
        ))
        .expect("set wls 1");
    store
        .set_word_learning_state(&sample_word_state(
            &user_1.id,
            &word_mid.id,
            WordState::Reviewing,
            Some(now - Duration::hours(1)),
        ))
        .expect("set wls 2");
    store
        .set_word_learning_state(&sample_word_state(
            &user_1.id,
            &word_hard.id,
            WordState::Mastered,
            Some(now - Duration::hours(100)),
        ))
        .expect("set wls 3");

    store
        .create_session(&sample_session("expired", &user_1.id, -1, false))
        .expect("create expired session");
    store
        .create_session(&sample_session("alive", &user_1.id, 3, false))
        .expect("create alive session");
    store
        .create_session(&sample_session("revoked", &user_1.id, 3, true))
        .expect("create revoked session");

    let old_ts = (now - Duration::days(8)).to_rfc3339();
    let new_ts = now.to_rfc3339();

    let old_event = serde_json::json!({
        "id": "old-event",
        "timestamp": old_ts,
        "kind": "old"
    });
    let new_event = serde_json::json!({
        "id": "new-event",
        "timestamp": new_ts,
        "kind": "new"
    });

    store
        .insert_monitoring_event(&old_event)
        .expect("insert old event");
    store
        .insert_monitoring_event(&new_event)
        .expect("insert new event");

    let old_key =
        keys::monitoring_event_key((now - Duration::days(8)).timestamp_millis(), "old-event")
            .unwrap();
    let new_key = keys::monitoring_event_key(now.timestamp_millis(), "new-event").unwrap();

    let registry = engine.metrics_registry().clone();
    registry.record_call(AlgorithmId::Heuristic, 120, false);
    registry.record_call(AlgorithmId::Heuristic, 240, true);

    workers::metrics_flush::run(&registry, store.as_ref()).await;
    registry.record_call(AlgorithmId::Heuristic, 80, false);
    workers::metrics_flush::run(&registry, store.as_ref()).await;

    workers::session_cleanup::run(store.as_ref()).await;
    workers::monitoring_aggregate::run(store.as_ref()).await;
    workers::llm_advisor::run(store.as_ref()).await;
    workers::delayed_reward::run(store.as_ref()).await;
    workers::forgetting_alert::run(store.as_ref()).await;
    workers::algorithm_optimization::run(store.as_ref(), &engine).await;
    workers::daily_aggregation::run(store.as_ref()).await;
    workers::health_analysis::run(store.as_ref()).await;
    workers::etymology_generation::run(store.as_ref()).await;
    workers::embedding_generation::run(store.as_ref()).await;
    workers::word_clustering::run(store.as_ref()).await;
    workers::confusion_pair_cache::run(store.as_ref()).await;
    workers::weekly_report::run(store.as_ref()).await;
    workers::log_export::run(store.as_ref()).await;
    workers::cache_cleanup::run(store.as_ref()).await;

    assert!(store.get_session("expired").expect("get expired").is_none());
    assert!(store.get_session("revoked").expect("get revoked").is_none());
    assert!(store.get_session("alive").expect("get alive").is_some());

    let notifications_prefix = keys::notification_prefix(&user_1.id).unwrap();
    let mut notification_count = 0usize;
    for item in store
        .notifications
        .scan_prefix(notifications_prefix.as_bytes())
    {
        item.expect("scan notification");
        notification_count += 1;
    }
    assert!(
        notification_count >= 1,
        "forgetting alert should create notifications"
    );

    assert!(store
        .etymologies
        .get(keys::etymology_key("w1").unwrap().as_bytes())
        .expect("get etymology w1")
        .is_some());

    let confusion_key = keys::confusion_pair_key(&word_easy.id, &word_mid.id).unwrap();
    assert!(store
        .confusion_pairs
        .get(confusion_key.as_bytes())
        .expect("get confusion pair")
        .is_some());

    assert!(store
        .engine_monitoring_events
        .get(old_key.as_bytes())
        .expect("get old monitoring key")
        .is_none());
    assert!(store
        .engine_monitoring_events
        .get(new_key.as_bytes())
        .expect("get new monitoring key")
        .is_some());

    let today = Utc::now().format("%Y-%m-%d").to_string();
    for metric_name in [
        "heuristic",
        "optimization",
        "daily_aggregation",
        "health_analysis",
        "weekly_report",
        "word_clustering",
    ] {
        assert!(
            store
                .get_metrics_daily(&today, metric_name)
                .expect("get metrics daily")
                .is_some(),
            "missing metrics for {metric_name}"
        );
    }

    let current_hour = Utc::now().format("%Y-%m-%d-%H").to_string();
    assert!(store
        .get_metrics_daily(&current_hour, "log_export")
        .expect("get log export")
        .is_some());
}

#[tokio::test]
async fn forgetting_alert_is_deduplicated_across_consecutive_runs() {
    let (_tmp, store) = setup_store("workers-forgetting-alert-dedup.sled");

    let user = sample_user("u-forget", "forget@test.com");
    store.create_user(&user).expect("create user");

    let due_time = Utc::now() - Duration::hours(72);
    store
        .set_word_learning_state(&sample_word_state(
            &user.id,
            "word-overdue",
            WordState::Learning,
            Some(due_time),
        ))
        .expect("set overdue state");

    workers::forgetting_alert::run(store.as_ref()).await;
    workers::forgetting_alert::run(store.as_ref()).await;

    let prefix = keys::notification_prefix(&user.id).expect("notification prefix");
    let mut forgetting_alert_count = 0usize;

    for item in store.notifications.scan_prefix(prefix.as_bytes()) {
        let (_, value) = item.expect("scan notification");
        let notif: serde_json::Value = serde_json::from_slice(&value).expect("parse notification");
        let is_forgetting_alert =
            notif.get("type").and_then(|t| t.as_str()) == Some("forgetting_alert");
        let same_word = notif.get("wordId").and_then(|w| w.as_str()) == Some("word-overdue");
        if is_forgetting_alert && same_word {
            forgetting_alert_count += 1;
        }
    }

    assert_eq!(forgetting_alert_count, 1);
}

#[test]
fn delayed_reward_counts_only_overdue_non_mastered_words() {
    let (_tmp, store) = setup_store("workers-delayed-reward-count.sled");

    let user = sample_user("u-delay", "delay@test.com");
    store.create_user(&user).expect("create user");

    let now = Utc::now();

    store
        .set_word_learning_state(&sample_word_state(
            &user.id,
            "w-overdue-learning",
            WordState::Learning,
            Some(now - Duration::hours(3)),
        ))
        .expect("set overdue learning");
    store
        .set_word_learning_state(&sample_word_state(
            &user.id,
            "w-overdue-reviewing",
            WordState::Reviewing,
            Some(now - Duration::hours(5)),
        ))
        .expect("set overdue reviewing");
    store
        .set_word_learning_state(&sample_word_state(
            &user.id,
            "w-future",
            WordState::Learning,
            Some(now + Duration::hours(2)),
        ))
        .expect("set future");
    store
        .set_word_learning_state(&sample_word_state(
            &user.id,
            "w-overdue-mastered",
            WordState::Mastered,
            Some(now - Duration::hours(9)),
        ))
        .expect("set overdue mastered");

    let count =
        workers::delayed_reward::count_overdue_words(store.as_ref(), now.timestamp_millis());
    assert_eq!(count, 2);
}

#[test]
fn it_exercises_memory_models_and_metrics_registry() {
    let mut evm_state = evm::EvmState::default();
    assert_eq!(evm::context_diversity_bonus(&evm_state), 0.0);
    evm::record_context(&mut evm_state, true);
    evm::record_context(&mut evm_state, false);
    let evm_bonus = evm::context_diversity_bonus(&evm_state);
    assert!(evm_bonus > 0.0);
    assert!(evm_bonus <= 0.3);
    assert!(evm::interval_modifier(&evm_state) >= 1.0);

    let mut iad_state = iad::IadState::default();
    let iad_config = learning_backend::amas::config::IadConfig::default();
    iad::record_confusion(&mut iad_state, "word-a", "word-b", 0.1, &iad_config);
    iad::record_confusion(&mut iad_state, "word-a", "word-b", 0.0, &iad_config);
    let penalty = iad::interference_penalty("word-b", &iad_state, &iad_config);
    assert!(penalty > 0.0);
    assert_eq!(
        iad::interference_penalty("word-x", &iad_state, &iad_config),
        0.0
    );
    assert!(iad::interval_extension_factor(penalty, &iad_config) < 1.0);

    let mut mtp_state = mtp::MtpState::default();
    let mtp_config = learning_backend::amas::config::MtpConfig::default();
    let morphemes = vec!["pre".to_string(), "dict".to_string()];
    mtp::update_known_morphemes(&mut mtp_state, &morphemes, 0.8, &mtp_config);
    let mtp_bonus =
        mtp::morpheme_transfer_bonus(&morphemes, &mtp_state.known_morphemes, &mtp_config);
    assert!(mtp_bonus > 0.0);
    assert_eq!(
        mtp::morpheme_transfer_bonus(&[], &mtp_state.known_morphemes, &mtp_config),
        0.0
    );

    let mut large_mtp_state = mtp::MtpState {
        known_morphemes: (0..501)
            .map(|idx| (format!("m-{idx}"), idx as f64 / 500.0))
            .collect(),
    };
    mtp::update_known_morphemes(
        &mut large_mtp_state,
        &["core".to_string()],
        0.9,
        &mtp_config,
    );
    assert!(large_mtp_state.known_morphemes.len() <= 500);

    let registry = MetricsRegistry::new();
    registry.record_call(AlgorithmId::Swd, 100, false);
    registry.record_call(AlgorithmId::Swd, 220, true);

    let snapshot = registry.snapshot();
    let swd = snapshot.get("swd").expect("swd metrics");
    assert_eq!(swd.call_count, 2);
    assert_eq!(swd.total_latency_us, 320);
    assert_eq!(swd.error_count, 1);

    let reset_snapshot = registry.snapshot_and_reset();
    let swd_before_reset = reset_snapshot.get("swd").expect("swd metrics before reset");
    assert_eq!(swd_before_reset.call_count, 2);

    let after_reset = registry.snapshot();
    let swd_after_reset = after_reset.get("swd").expect("swd metrics after reset");
    assert_eq!(swd_after_reset.call_count, 0);
    assert_eq!(swd_after_reset.total_latency_us, 0);
    assert_eq!(swd_after_reset.error_count, 0);

    registry.reset();
    let after_manual_reset = registry.snapshot();
    let swd_after_manual_reset = after_manual_reset
        .get("swd")
        .expect("swd metrics after manual reset");
    assert_eq!(swd_after_manual_reset.call_count, 0);
}
