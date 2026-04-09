use std::collections::HashSet;
use std::time::{Duration as StdDuration, Instant};
use tempfile::tempdir;

use chrono::{DateTime, Duration, Utc};

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
                store
                    .create_notification(&notification)
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
        let states = store.list_user_word_states(user_id, 100_000, 0).expect("list wls");
        for state in &states {
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
    let cutoff = now - window;

    let notifications = match store.list_notifications(user_id, 100_000, false) {
        Ok(n) => n,
        Err(_) => return false,
    };

    for notif in &notifications {
        let same_word = notif.word_id.as_deref() == Some(word_id);
        if same_word && notif.created_at >= cutoff {
            return true;
        }
    }

    false
}

fn baseline_forgetting_alert_candidates(store: &Store, now: DateTime<Utc>) -> u32 {
    let dedup_window = Duration::hours(48);
    let mut at_risk = 0u32;

    let user_ids = store.list_user_ids().unwrap_or_default();
    for user_id in &user_ids {
        let states = match store.list_user_word_states(user_id, 100_000, 0) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for state in &states {
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
    }

    at_risk
}

fn optimized_recent_alert_word_ids_in_window(
    store: &Store,
    user_id: &str,
    cutoff: DateTime<Utc>,
) -> HashSet<String> {
    let notifications = match store.list_notifications(user_id, 100_000, false) {
        Ok(n) => n,
        Err(_) => return HashSet::new(),
    };

    let mut word_ids = HashSet::new();
    for notif in &notifications {
        if notif.word_id.is_none() {
            continue;
        }

        if notif.created_at < cutoff {
            continue;
        }

        if let Some(word_id) = &notif.word_id {
            word_ids.insert(word_id.clone());
        }
    }

    word_ids
}

fn optimized_forgetting_alert_candidates(store: &Store, now: DateTime<Utc>) -> u32 {
    let dedup_window = Duration::hours(48);
    let cutoff = now - dedup_window;
    let mut at_risk = 0u32;

    let user_ids = match store.list_user_ids() {
        Ok(u) => u,
        Err(_) => return 0,
    };

    for user_id in &user_ids {
        let due_words = match store.get_due_words(user_id, 100_000) {
            Ok(w) => w,
            Err(_) => continue,
        };

        let mut recent_alert_word_ids: Option<HashSet<String>> = None;

        for state in &due_words {
            let Some(review_date) = state.next_review_date else {
                continue;
            };

            let overdue_hours = (now - review_date).num_hours();
            if overdue_hours <= 48 || state.state == WordState::Mastered {
                continue;
            }

            let recent_word_ids = recent_alert_word_ids.get_or_insert_with(|| {
                optimized_recent_alert_word_ids_in_window(store, user_id, cutoff)
            });

            if recent_word_ids.contains(state.word_id.as_str()) {
                continue;
            }

            recent_word_ids.insert(state.word_id.clone());
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
    let store = Store::open(dir.path().join("db").to_str().expect("db path"), 5000, 1).expect("open");

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
