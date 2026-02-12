use crate::store::StoreError;

/// 验证 ID 不包含冒号分隔符且非空，防止键注入
pub fn validate_id(id: &str) -> Result<&str, StoreError> {
    if id.is_empty() {
        return Err(StoreError::Validation("ID 不能为空".to_string()));
    }
    if id.contains(':') {
        return Err(StoreError::Validation("ID 不能包含冒号分隔符".to_string()));
    }
    Ok(id)
}

pub fn user_key(user_id: &str) -> Result<String, StoreError> {
    Ok(validate_id(user_id)?.to_string())
}

pub fn user_email_index_key(email: &str) -> Result<String, StoreError> {
    if email.is_empty() {
        return Err(StoreError::Validation("邮箱不能为空".to_string()));
    }
    Ok(format!("email:{}", email.to_lowercase()))
}

pub fn session_key(token_hash: &str) -> Result<String, StoreError> {
    Ok(validate_id(token_hash)?.to_string())
}

pub fn session_user_index_key(user_id: &str, token_hash: &str) -> Result<String, StoreError> {
    Ok(format!(
        "user:{}:{}",
        validate_id(user_id)?,
        validate_id(token_hash)?
    ))
}

/// 用于前缀扫描的会话用户索引前缀（不验证后缀部分）
pub fn session_user_index_prefix(user_id: &str) -> Result<String, StoreError> {
    Ok(format!("user:{}:", validate_id(user_id)?))
}

pub fn word_key(word_id: &str) -> Result<String, StoreError> {
    Ok(validate_id(word_id)?.to_string())
}

pub fn record_key(user_id: &str, timestamp_ms: i64, record_id: &str) -> Result<String, StoreError> {
    let ts = timestamp_ms.max(0) as u64;
    let reverse_ts = u64::MAX - ts;
    Ok(format!(
        "{}:{:020}:{}",
        validate_id(user_id)?,
        reverse_ts,
        validate_id(record_id)?
    ))
}

pub fn record_prefix(user_id: &str) -> Result<String, StoreError> {
    Ok(format!("{}:", validate_id(user_id)?))
}

pub fn learning_session_key(session_id: &str) -> Result<String, StoreError> {
    Ok(validate_id(session_id)?.to_string())
}

pub fn learning_session_user_index(user_id: &str, session_id: &str) -> Result<String, StoreError> {
    Ok(format!(
        "user:{}:{}",
        validate_id(user_id)?,
        validate_id(session_id)?
    ))
}

pub fn learning_session_user_index_prefix(user_id: &str) -> Result<String, StoreError> {
    Ok(format!("user:{}:", validate_id(user_id)?))
}

pub fn engine_user_state_key(user_id: &str) -> Result<String, StoreError> {
    Ok(validate_id(user_id)?.to_string())
}

/// 构建引擎算法状态的存储键。
/// 设计决策：algorithm_id 允许包含冒号（如 "mastery:word_id"），
/// 因为算法状态需要按层级关系组织（算法类型:目标实体），
/// 而 user_id 作为前缀已足够区分不同用户的数据。
/// 前缀扫描 `{user_id}:` 可获取该用户的所有算法状态。
pub fn engine_algo_state_key(user_id: &str, algorithm_id: &str) -> Result<String, StoreError> {
    // algorithm_id 可能包含冒号（如 "mastery:word_id"），所以只验证 user_id
    if algorithm_id.is_empty() {
        return Err(StoreError::Validation("算法 ID 不能为空".to_string()));
    }
    Ok(format!("{}:{}", validate_id(user_id)?, algorithm_id))
}

pub fn monitoring_event_key(timestamp_ms: i64, event_id: &str) -> Result<String, StoreError> {
    let ts = timestamp_ms.max(0) as u64;
    let reverse_ts = u64::MAX - ts;
    Ok(format!("{:020}:{}", reverse_ts, validate_id(event_id)?))
}

pub fn metrics_daily_key(date: &str, algorithm_id: &str) -> Result<String, StoreError> {
    Ok(format!(
        "{}:{}",
        validate_id(date)?,
        validate_id(algorithm_id)?
    ))
}

pub fn password_reset_key(token_hash: &str) -> Result<String, StoreError> {
    Ok(validate_id(token_hash)?.to_string())
}

pub fn config_version_key(config_type: &str, version: u32) -> Result<String, StoreError> {
    Ok(format!("{}:{:010}", validate_id(config_type)?, version))
}

pub fn config_latest_key(config_type: &str) -> Result<String, StoreError> {
    Ok(format!("{}:latest", validate_id(config_type)?))
}

// Admin keys
pub fn admin_key(admin_id: &str) -> Result<String, StoreError> {
    Ok(validate_id(admin_id)?.to_string())
}

pub fn admin_email_index_key(email: &str) -> Result<String, StoreError> {
    if email.is_empty() {
        return Err(StoreError::Validation("邮箱不能为空".to_string()));
    }
    Ok(format!("email:{}", email.to_lowercase()))
}

// Wordbook keys
pub fn wordbook_key(wordbook_id: &str) -> Result<String, StoreError> {
    Ok(validate_id(wordbook_id)?.to_string())
}

pub fn wordbook_words_key(wordbook_id: &str, word_id: &str) -> Result<String, StoreError> {
    Ok(format!(
        "{}:{}",
        validate_id(wordbook_id)?,
        validate_id(word_id)?
    ))
}

pub fn wordbook_words_prefix(wordbook_id: &str) -> Result<String, StoreError> {
    Ok(format!("{}:", validate_id(wordbook_id)?))
}

// Study config keys
pub fn study_config_key(user_id: &str) -> Result<String, StoreError> {
    Ok(validate_id(user_id)?.to_string())
}

// Word learning state keys
pub fn word_learning_state_key(user_id: &str, word_id: &str) -> Result<String, StoreError> {
    Ok(format!(
        "{}:{}",
        validate_id(user_id)?,
        validate_id(word_id)?
    ))
}

pub fn word_learning_state_prefix(user_id: &str) -> Result<String, StoreError> {
    Ok(format!("{}:", validate_id(user_id)?))
}

pub fn word_due_index_key(
    user_id: &str,
    due_ts_ms: i64,
    word_id: &str,
) -> Result<String, StoreError> {
    let ts = due_ts_ms.max(0) as u64;
    Ok(format!(
        "{}:{:020}:{}",
        validate_id(user_id)?,
        ts,
        validate_id(word_id)?
    ))
}

pub fn word_due_index_prefix(user_id: &str) -> Result<String, StoreError> {
    Ok(format!("{}:", validate_id(user_id)?))
}

// User profile keys
pub fn user_profile_key(user_id: &str) -> Result<String, StoreError> {
    Ok(validate_id(user_id)?.to_string())
}

pub fn user_avatar_key(user_id: &str) -> Result<String, StoreError> {
    Ok(format!("avatar:{}", validate_id(user_id)?))
}

pub fn habit_profile_key(user_id: &str) -> Result<String, StoreError> {
    Ok(validate_id(user_id)?.to_string())
}

pub fn notification_key(user_id: &str, notification_id: &str) -> Result<String, StoreError> {
    Ok(format!(
        "{}:{}",
        validate_id(user_id)?,
        validate_id(notification_id)?
    ))
}

pub fn notification_prefix(user_id: &str) -> Result<String, StoreError> {
    Ok(format!("{}:", validate_id(user_id)?))
}

pub fn badge_key(user_id: &str, badge_id: &str) -> Result<String, StoreError> {
    Ok(format!(
        "{}:{}",
        validate_id(user_id)?,
        validate_id(badge_id)?
    ))
}

pub fn badge_prefix(user_id: &str) -> Result<String, StoreError> {
    Ok(format!("{}:", validate_id(user_id)?))
}

pub fn user_preferences_key(user_id: &str) -> Result<String, StoreError> {
    Ok(validate_id(user_id)?.to_string())
}

pub fn etymology_key(word_id: &str) -> Result<String, StoreError> {
    Ok(validate_id(word_id)?.to_string())
}

pub fn word_morpheme_key(word_id: &str) -> Result<String, StoreError> {
    Ok(validate_id(word_id)?.to_string())
}

// ELO rating keys
pub fn user_elo_key(user_id: &str) -> Result<String, StoreError> {
    Ok(format!("user_elo:{}", validate_id(user_id)?))
}

pub fn word_elo_key(word_id: &str) -> Result<String, StoreError> {
    Ok(format!("word_elo:{}", validate_id(word_id)?))
}

pub fn confusion_pair_key(word_id_a: &str, word_id_b: &str) -> Result<String, StoreError> {
    let a = validate_id(word_id_a)?;
    let b = validate_id(word_id_b)?;
    if a < b {
        Ok(format!("{}:{}", a, b))
    } else {
        Ok(format!("{}:{}", b, a))
    }
}

/// 解析 word_due_index 中条目的键，提取 (due_ts_ms, word_id)。
/// 键格式: "{user_id}:{due_ts_ms:020}:{word_id}"
/// 第一段（user_id）已被 scan_prefix 跳过，此处从第二段开始解析。
pub fn parse_due_index_item_key(key: &[u8]) -> Option<(i64, String)> {
    let key_text = std::str::from_utf8(key).ok()?;
    let mut parts = key_text.splitn(3, ':');
    let _ = parts.next()?; // user_id
    let due_ts_part = parts.next()?;
    let word_id = parts.next()?.to_string();
    let due_ts = due_ts_part
        .parse::<u64>()
        .ok()
        .map(|value| value.min(i64::MAX as u64) as i64)?;
    Some((due_ts, word_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_key_orders_by_time_desc() {
        let k_new = record_key("u1", 2000, "r2").unwrap();
        let k_old = record_key("u1", 1000, "r1").unwrap();
        assert!(k_new < k_old);
    }

    #[test]
    fn email_index_is_normalized() {
        assert_eq!(user_email_index_key("A@Ex.com").unwrap(), "email:a@ex.com");
    }

    #[test]
    fn validate_id_rejects_empty() {
        assert!(validate_id("").is_err());
    }

    #[test]
    fn validate_id_rejects_colon() {
        assert!(validate_id("a:b").is_err());
    }

    #[test]
    fn validate_id_accepts_valid() {
        assert!(validate_id("abc-123").is_ok());
    }

    #[test]
    fn parse_due_index_item_key_works() {
        let key = b"user1:00000000001000000:word42";
        let result = parse_due_index_item_key(key);
        assert!(result.is_some());
        let (ts, word_id) = result.unwrap();
        assert_eq!(ts, 1000000);
        assert_eq!(word_id, "word42");
    }

    #[test]
    fn parse_due_index_item_key_invalid_format() {
        let key = b"only_one_part";
        assert!(parse_due_index_item_key(key).is_none());
    }
}
