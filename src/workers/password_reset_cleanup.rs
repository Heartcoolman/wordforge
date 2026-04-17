use crate::store::Store;

pub async fn run(store: &Store) {
    tracing::debug!("password_reset_cleanup: start");
    let store = store.clone();
    match crate::blocking::run_blocking("worker.password_reset_cleanup", move || {
        store.cleanup_expired_reset_tokens()
    })
    .await
    {
        Ok(Ok(count)) if count > 0 => {
            tracing::info!(cleaned = count, "password_reset_cleanup: done");
        }
        Ok(Err(e)) => tracing::error!(error=%e, "password_reset_cleanup failed"),
        Err(e) => tracing::error!(error=%e, "password_reset_cleanup task failed"),
        _ => {}
    }
}
