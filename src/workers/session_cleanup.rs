use crate::store::Store;

pub async fn run(store: &Store) {
    tracing::debug!("session_cleanup: start");
    let store = store.clone();
    match crate::blocking::run_blocking("worker.session_cleanup", move || {
        store.cleanup_expired_sessions()
    })
    .await
    {
        Ok(Ok(count)) => tracing::info!(cleaned = count, "session_cleanup: done"),
        Ok(Err(e)) => tracing::error!(error=%e, "session_cleanup failed"),
        Err(e) => tracing::error!(error=%e, "session_cleanup task failed"),
    }
}
