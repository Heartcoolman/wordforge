use crate::store::Store;

pub async fn run(store: &Store) {
    tracing::debug!("password_reset_cleanup: start");
    match store.cleanup_expired_reset_tokens() {
        Ok(count) if count > 0 => {
            tracing::info!(cleaned = count, "password_reset_cleanup: done");
        }
        Err(e) => tracing::error!(error=%e, "password_reset_cleanup failed"),
        _ => {}
    }
}
