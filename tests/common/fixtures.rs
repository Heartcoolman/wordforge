use chrono::Utc;

use learning_backend::amas::types::UserState;
use learning_backend::auth::hash_password;
use learning_backend::store::operations::users::User;
use learning_backend::store::operations::words::Word;
use learning_backend::store::Store;

pub fn seed_user(store: &Store, email: &str, username: &str, password: &str) -> User {
    let now = Utc::now();
    let user = User {
        id: uuid::Uuid::new_v4().to_string(),
        email: email.to_string(),
        username: username.to_string(),
        password_hash: hash_password(password).expect("hash password"),
        is_banned: false,
        created_at: now,
        updated_at: now,
        failed_login_count: 0,
        locked_until: None,
    };
    store.create_user(&user).expect("create seed user");
    user
}

pub fn seed_words(store: &Store, count: usize) -> Vec<Word> {
    let mut out = Vec::new();
    for idx in 0..count {
        let word = Word {
            id: uuid::Uuid::new_v4().to_string(),
            text: format!("word-{idx}"),
            meaning: format!("meaning-{idx}"),
            pronunciation: None,
            part_of_speech: None,
            difficulty: 0.5,
            examples: vec![],
            tags: vec!["seed".to_string()],
            embedding: None,
            created_at: Utc::now(),
        };
        store.upsert_word(&word).expect("upsert seed word");
        out.push(word);
    }
    out
}

pub fn seed_engine_state(store: &Store, user_id: &str, fatigue: f64) -> UserState {
    let state = UserState {
        fatigue,
        ..UserState::default()
    };

    store
        .set_engine_user_state(
            user_id,
            &serde_json::to_value(&state).expect("state to value"),
        )
        .expect("persist engine state");

    state
}
