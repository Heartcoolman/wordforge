use crate::store::Store;

/// 清理过期的密码重置 token
pub async fn run(store: &Store) {
    tracing::debug!("password_reset_cleanup: start");
    match cleanup_expired_tokens(store) {
        Ok(count) => {
            if count > 0 {
                tracing::info!(cleaned = count, "password_reset_cleanup: done");
            }
        }
        Err(e) => tracing::error!(error=%e, "password_reset_cleanup failed"),
    }
}

fn cleanup_expired_tokens(store: &Store) -> Result<u32, crate::store::StoreError> {
    let now = chrono::Utc::now();
    let mut expired_keys = Vec::new();

    for item in store.password_reset_tokens.iter() {
        let (k, v) = item.map_err(crate::store::StoreError::from)?;
        if let Ok(entry) = serde_json::from_slice::<PasswordResetEntry>(&v) {
            if entry.expires_at <= now {
                expired_keys.push(k);
            }
        }
    }

    let count = expired_keys.len() as u32;
    for key in expired_keys {
        let _ = store.password_reset_tokens.remove(key);
    }

    Ok(count)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PasswordResetEntry {
    #[allow(dead_code)]
    user_id: String,
    expires_at: chrono::DateTime<chrono::Utc>,
}
