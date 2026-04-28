use std::error::Error;
use std::fmt;
use std::sync::Arc;

use tokio::sync::Semaphore;
use tokio::task::JoinError;

#[derive(Debug)]
pub struct BlockingTaskError {
    task_name: &'static str,
    source: JoinError,
}

impl BlockingTaskError {
    pub fn new(task_name: &'static str, source: JoinError) -> Self {
        Self { task_name, source }
    }
}

impl fmt::Display for BlockingTaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "blocking task `{}` failed: {}",
            self.task_name, self.source
        )
    }
}

impl Error for BlockingTaskError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

/// Global semaphore limiting concurrent blocking tasks to prevent thread pool exhaustion.
/// Aligned with SQLite pool size — when DB connections are the bottleneck,
/// queueing more blocking tasks only wastes threads and risks runtime stalls.
static BLOCKING_SEMAPHORE: std::sync::OnceLock<Arc<Semaphore>> = std::sync::OnceLock::new();

/// Initialize the global blocking semaphore with the given permits.
/// Should be called once at startup with a value ≥ sqlite_pool_size.
pub fn init_blocking_semaphore(permits: usize) {
    let _ = BLOCKING_SEMAPHORE.set(Arc::new(Semaphore::new(permits.max(1))));
}

fn blocking_semaphore() -> Arc<Semaphore> {
    BLOCKING_SEMAPHORE
        .get_or_init(|| {
            let permits = std::thread::available_parallelism()
                .map(|parallelism| parallelism.get().saturating_mul(2))
                .unwrap_or(8)
                .max(1);
            Arc::new(Semaphore::new(permits))
        })
        .clone()
}

/// Run a closure on the blocking thread pool with back-pressure.
/// Acquires a permit from the global semaphore *before* spawning,
/// so the number of concurrent blocking tasks never exceeds the permit count.
/// This prevents thread-pool exhaustion when SQLite connection contention causes
/// tasks to pile up inside `spawn_blocking`.
pub async fn run_blocking<T, F>(task_name: &'static str, f: F) -> Result<T, BlockingTaskError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let semaphore = blocking_semaphore();
    let permit = semaphore
        .acquire()
        .await
        .expect("blocking semaphore closed");
    let result = tokio::task::spawn_blocking(f).await;
    drop(permit);
    result.map_err(|source| BlockingTaskError::new(task_name, source))
}
