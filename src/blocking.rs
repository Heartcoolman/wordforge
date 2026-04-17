use std::error::Error;
use std::fmt;

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

pub async fn run_blocking<T, F>(task_name: &'static str, f: F) -> Result<T, BlockingTaskError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|source| BlockingTaskError::new(task_name, source))
}
