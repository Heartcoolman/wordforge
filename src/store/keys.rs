pub fn user_key(user_id: &str) -> String {
    user_id.to_string()
}

pub fn user_email_index_key(email: &str) -> String {
    format!("email:{}", email.to_lowercase())
}

pub fn session_key(token_hash: &str) -> String {
    token_hash.to_string()
}

pub fn session_user_index_key(user_id: &str, token_hash: &str) -> String {
    format!("user:{}:{}", user_id, token_hash)
}

pub fn word_key(word_id: &str) -> String {
    word_id.to_string()
}

pub fn record_key(user_id: &str, timestamp_ms: i64, record_id: &str) -> String {
    let ts = timestamp_ms.max(0) as u64;
    let reverse_ts = u64::MAX - ts;
    format!("{}:{:020}:{}", user_id, reverse_ts, record_id)
}

pub fn record_prefix(user_id: &str) -> String {
    format!("{}:", user_id)
}

pub fn learning_session_key(session_id: &str) -> String {
    session_id.to_string()
}

pub fn learning_session_user_index(user_id: &str, session_id: &str) -> String {
    format!("user:{}:{}", user_id, session_id)
}

pub fn engine_user_state_key(user_id: &str) -> String {
    user_id.to_string()
}

pub fn engine_algo_state_key(user_id: &str, algorithm_id: &str) -> String {
    format!("{}:{}", user_id, algorithm_id)
}

pub fn monitoring_event_key(timestamp_ms: i64, event_id: &str) -> String {
    let ts = timestamp_ms.max(0) as u64;
    let reverse_ts = u64::MAX - ts;
    format!("{:020}:{}", reverse_ts, event_id)
}

pub fn metrics_daily_key(date: &str, algorithm_id: &str) -> String {
    format!("{}:{}", date, algorithm_id)
}

pub fn password_reset_key(token_hash: &str) -> String {
    token_hash.to_string()
}

pub fn config_version_key(config_type: &str, version: u32) -> String {
    format!("{}:{:010}", config_type, version)
}

pub fn config_latest_key(config_type: &str) -> String {
    format!("{}:latest", config_type)
}

// Admin keys
pub fn admin_key(admin_id: &str) -> String {
    admin_id.to_string()
}

pub fn admin_email_index_key(email: &str) -> String {
    format!("email:{}", email.to_lowercase())
}

// Wordbook keys
pub fn wordbook_key(wordbook_id: &str) -> String {
    wordbook_id.to_string()
}

pub fn wordbook_words_key(wordbook_id: &str, word_id: &str) -> String {
    format!("{}:{}", wordbook_id, word_id)
}

pub fn wordbook_words_prefix(wordbook_id: &str) -> String {
    format!("{}:", wordbook_id)
}

// Study config keys
pub fn study_config_key(user_id: &str) -> String {
    user_id.to_string()
}

// Word learning state keys
pub fn word_learning_state_key(user_id: &str, word_id: &str) -> String {
    format!("{}:{}", user_id, word_id)
}

pub fn word_learning_state_prefix(user_id: &str) -> String {
    format!("{}:", user_id)
}

// User profile keys
pub fn user_profile_key(user_id: &str) -> String {
    user_id.to_string()
}

pub fn habit_profile_key(user_id: &str) -> String {
    user_id.to_string()
}

pub fn notification_key(user_id: &str, notification_id: &str) -> String {
    format!("{}:{}", user_id, notification_id)
}

pub fn notification_prefix(user_id: &str) -> String {
    format!("{}:", user_id)
}

pub fn badge_key(user_id: &str, badge_id: &str) -> String {
    format!("{}:{}", user_id, badge_id)
}

pub fn badge_prefix(user_id: &str) -> String {
    format!("{}:", user_id)
}

pub fn user_preferences_key(user_id: &str) -> String {
    user_id.to_string()
}

pub fn etymology_key(word_id: &str) -> String {
    word_id.to_string()
}

pub fn word_morpheme_key(word_id: &str) -> String {
    word_id.to_string()
}

pub fn confusion_pair_key(word_id_a: &str, word_id_b: &str) -> String {
    if word_id_a < word_id_b {
        format!("{}:{}", word_id_a, word_id_b)
    } else {
        format!("{}:{}", word_id_b, word_id_a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_key_orders_by_time_desc() {
        let k_new = record_key("u1", 2000, "r2");
        let k_old = record_key("u1", 1000, "r1");
        assert!(k_new < k_old);
    }

    #[test]
    fn email_index_is_normalized() {
        assert_eq!(user_email_index_key("A@Ex.com"), "email:a@ex.com");
    }
}
