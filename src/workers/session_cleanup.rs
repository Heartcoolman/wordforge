use crate::store::Store;

pub async fn run(store: &Store) {
    tracing::debug!("session_cleanup: start");
    match store.cleanup_expired_sessions() {
        Ok(count) => tracing::info!(cleaned = count, "session_cleanup: done"),
        Err(e) => tracing::error!(error=%e, "session_cleanup failed"),
    }
}
