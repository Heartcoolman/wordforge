use std::collections::HashSet;
use std::time::{Duration as StdDuration, Instant};
use tempfile::tempdir;

use chrono::{DateTime, Duration, Utc};

use learning_backend::store::keys;
use learning_backend::store::operations::users::User;
use learning_backend::store::operations::word_states::{WordLearningState, WordState};
use learning_backend::store::Store;
use learning_backend::workers::delayed_reward;

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

fn sample_state(
    user_id: &str,
    word_id: &str,
    state: WordState,
    next_review_date: Option<DateTime<Utc>>,
) -> WordLearningState {
    WordLearningState {
        user_id: user_id.to_string(),
        word_id: word_id.to_string(),
        state,
        mastery_level: 0.5,
        next_review_date,
        half_life: 3.0,
        correct_streak: 2,
        total_attempts: 6,
        updated_at: Utc::now(),
    }
}

fn seed_perf_data(store: &Store) -> DateTime<Utc> {
    let now = Utc::now();
    const USER_COUNT: usize = 80;
    const WORDS_PER_USER: usize = 180;

    for user_index in 0..USER_COUNT {
        let user_id = format!("u{user_index}");
        let email = format!("u{user_index}@bench.local");
        store
            .create_user(&sample_user(&user_id, &email))
            .expect("create user");

        for word_index in 0..WORDS_PER_USER {
            let word_id = format!("w{user_index}_{word_index}");
            let (state, next_review_date) = match word_index % 6 {
                0 => (WordState::Learning, Some(now - Duration::hours(72))),
                1 => (WordState::Reviewing, Some(now - Duration::hours(12))),
                2 => (WordState::Learning, Some(now + Duration::hours(6))),
                3 => (WordState::Mastered, Some(now - Duration::hours(96))),
                4 => (WordState::Learning, None),
                _ => (WordState::Forgotten, Some(now - Duration::hours(120))),
            };

            store
                .set_word_learning_state(&sample_state(&user_id, &word_id, state, next_review_date))
                .expect("set word state");

            if word_index % 12 == 0 {
                let notification = serde_json::json!({
                    "id": format!("n{user_index}_{word_index}"),
                    "userId": user_id,
                    "type": "forgetting_alert",
                    "wordId": word_id,
                    "overdueHours": 72,
                    "createdAt": (now - Duration::hours(12)).to_rfc3339(),
                    "read": false,
                });
                let key = keys::notification_key(
                    &format!("u{user_index}"),
                    &format!("n{user_index}_{word_index}"),
                )
                .expect("notification key");
                store
                    .notifications
                    .insert(
                        key.as_bytes(),
                        serde_json::to_vec(&notification).expect("notification bytes"),
                    )
                    .expect("insert notification");
            }
        }
    }

    now
}

fn baseline_delayed_reward_count(store: &Store, now: DateTime<Utc>) -> u32 {
    let mut evaluated = 0u32;
    let user_ids = store.list_user_ids().expect("list users");

    for user_id in &user_ids {
        let prefix = keys::word_learning_state_prefix(user_id).expect("prefix");
        for item in store.word_learning_states.scan_prefix(prefix.as_bytes()) {
            let (_, value) = item.expect("scan wls");
            let state: WordLearningState = serde_json::from_slice(&value).expect("deserialize wls");
            if let Some(review_date) = state.next_review_date {
                if review_date <= now && state.state != WordState::Mastered {
                    evaluated += 1;
                }
            }
        }
    }

    evaluated
}

fn baseline_has_recent_alert(
    store: &Store,
    user_id: &str,
    word_id: &str,
    now: DateTime<Utc>,
    window: Duration,
) -> bool {
    let prefix = match keys::notification_prefix(user_id) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let cutoff = now - window;

    for item in store.notifications.scan_prefix(prefix.as_bytes()) {
        let (_, value) = match item {
            Ok(v) => v,
            Err(_) => continue,
        };

        let notif: serde_json::Value = match serde_json::from_slice(&value) {
            Ok(n) => n,
            Err(_) => continue,
        };

        let is_alert = notif.get("type").and_then(|t| t.as_str()) == Some("forgetting_alert");
        let same_word = notif.get("wordId").and_then(|w| w.as_str()) == Some(word_id);

        if is_alert && same_word {
            if let Some(created_str) = notif.get("createdAt").and_then(|c| c.as_str()) {
                if let Ok(created) = DateTime::parse_from_rfc3339(created_str) {
                    if created.with_timezone(&Utc) >= cutoff {
                        return true;
                    }
                }
            }
        }
    }

    false
}

fn baseline_forgetting_alert_candidates(store: &Store, now: DateTime<Utc>) -> u32 {
    let dedup_window = Duration::hours(48);
    let mut at_risk = 0u32;

    for item in store.word_learning_states.iter() {
        let (_, value) = match item {
            Ok(v) => v,
            Err(_) => continue,
        };

        let state: WordLearningState = match serde_json::from_slice(&value) {
            Ok(s) => s,
            Err(_) => continue,
        };

        if let Some(review_date) = state.next_review_date {
            let overdue_hours = (now - review_date).num_hours();
            if overdue_hours > 48 && state.state != WordState::Mastered {
                if baseline_has_recent_alert(
                    store,
                    &state.user_id,
                    &state.word_id,
                    now,
                    dedup_window,
                ) {
                    continue;
                }
                at_risk += 1;
            }
        }
    }

    at_risk
}

fn parse_due_index_item_key(key: &[u8]) -> Option<(i64, String)> {
    let key_text = std::str::from_utf8(key).ok()?;
    let mut parts = key_text.splitn(3, ':');
    let _ = parts.next()?;
    let due_ts_part = parts.next()?;
    let word_id = parts.next()?.to_string();
    let due_ts = due_ts_part
        .parse::<u64>()
        .ok()
        .map(|value| value.min(i64::MAX as u64) as i64)?;
    Some((due_ts, word_id))
}

fn optimized_recent_alert_word_ids_in_window(
    store: &Store,
    user_id: &str,
    cutoff: DateTime<Utc>,
) -> HashSet<String> {
    let prefix = match keys::notification_prefix(user_id) {
        Ok(p) => p,
        Err(_) => return HashSet::new(),
    };

    let mut word_ids = HashSet::new();
    for item in store.notifications.scan_prefix(prefix.as_bytes()) {
        let (_, value) = match item {
            Ok(v) => v,
            Err(_) => continue,
        };

        let notif: serde_json::Value = match serde_json::from_slice(&value) {
            Ok(n) => n,
            Err(_) => continue,
        };

        if notif.get("type").and_then(|t| t.as_str()) != Some("forgetting_alert") {
            continue;
        }

        let Some(created_str) = notif.get("createdAt").and_then(|c| c.as_str()) else {
            continue;
        };
        let Ok(created) = DateTime::parse_from_rfc3339(created_str) else {
            continue;
        };

        if created.with_timezone(&Utc) < cutoff {
            continue;
        }

        if let Some(word_id) = notif.get("wordId").and_then(|w| w.as_str()) {
            word_ids.insert(word_id.to_string());
        }
    }

    word_ids
}

fn optimized_forgetting_alert_candidates(store: &Store, now: DateTime<Utc>) -> u32 {
    let dedup_window = Duration::hours(48);
    let cutoff = now - dedup_window;
    let cutoff_ms = cutoff.timestamp_millis().max(0);
    let mut at_risk = 0u32;

    let user_ids = match store.list_user_ids() {
        Ok(u) => u,
        Err(_) => return 0,
    };

    for user_id in &user_ids {
        let prefix = match keys::word_due_index_prefix(user_id) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let mut recent_alert_word_ids: Option<HashSet<String>> = None;

        for item in store.word_due_index.scan_prefix(prefix.as_bytes()) {
            let (key, _) = match item {
                Ok(v) => v,
                Err(_) => continue,
            };

            let Some((due_ts_ms, word_id)) = parse_due_index_item_key(&key) else {
                continue;
            };

            if due_ts_ms > cutoff_ms {
                break;
            }

            let state = match store.get_word_learning_state(user_id, &word_id) {
                Ok(Some(s)) => s,
                _ => continue,
            };

            let Some(review_date) = state.next_review_date else {
                continue;
            };

            let review_ts_ms = review_date.timestamp_millis().max(0);
            if review_ts_ms != due_ts_ms || state.state == WordState::Mastered {
                continue;
            }

            let recent_word_ids = recent_alert_word_ids.get_or_insert_with(|| {
                optimized_recent_alert_word_ids_in_window(store, user_id, cutoff)
            });

            if recent_word_ids.contains(word_id.as_str()) {
                continue;
            }

            recent_word_ids.insert(word_id);
            at_risk += 1;
        }
    }

    at_risk
}

fn bench_counter<F>(label: &str, repeat: usize, mut f: F) -> (u32, StdDuration)
where
    F: FnMut() -> u32,
{
    let mut total = StdDuration::ZERO;
    let mut count = 0u32;

    for _ in 0..repeat {
        let start = Instant::now();
        count = f();
        total += start.elapsed();
    }

    let avg = total / (repeat as u32);
    println!("{label}: avg={avg:?}, count={count}");
    (count, avg)
}

#[test]
#[ignore]
fn compare_workers_before_after_latency() {
    let dir = tempdir().expect("tempdir");
    let store = Store::open(dir.path().join("db").to_str().expect("db path")).expect("open");

    let now = seed_perf_data(&store);

    let _ = baseline_delayed_reward_count(&store, now);
    let _ = delayed_reward::count_overdue_words(&store, now.timestamp_millis().max(0));
    let _ = baseline_forgetting_alert_candidates(&store, now);
    let _ = optimized_forgetting_alert_candidates(&store, now);

    const REPEAT: usize = 8;

    let (delayed_before_count, delayed_before_avg) =
        bench_counter("delayed_reward_before", REPEAT, || {
            baseline_delayed_reward_count(&store, now)
        });
    let (delayed_after_count, delayed_after_avg) =
        bench_counter("delayed_reward_after", REPEAT, || {
            delayed_reward::count_overdue_words(&store, now.timestamp_millis().max(0))
        });

    assert_eq!(delayed_before_count, delayed_after_count);

    let delayed_speedup =
        delayed_before_avg.as_secs_f64() / delayed_after_avg.as_secs_f64().max(1e-9);

    let (forget_before_count, forget_before_avg) =
        bench_counter("forgetting_alert_before", REPEAT, || {
            baseline_forgetting_alert_candidates(&store, now)
        });
    let (forget_after_count, forget_after_avg) =
        bench_counter("forgetting_alert_after", REPEAT, || {
            optimized_forgetting_alert_candidates(&store, now)
        });

    assert_eq!(forget_before_count, forget_after_count);

    let forget_speedup = forget_before_avg.as_secs_f64() / forget_after_avg.as_secs_f64().max(1e-9);

    println!(
        "SPEEDUP delayed_reward={:.2}x, forgetting_alert={:.2}x",
        delayed_speedup, forget_speedup
    );
}
