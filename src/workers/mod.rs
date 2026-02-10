pub mod algorithm_optimization;
pub mod cache_cleanup;
pub mod confusion_pair_cache;
pub mod daily_aggregation;
pub mod delayed_reward;
pub mod embedding_generation;
pub mod etymology_generation;
pub mod forgetting_alert;
pub mod health_analysis;
pub mod llm_advisor;
pub mod log_export;
pub mod metrics_flush;
pub mod monitoring_aggregate;
pub mod session_cleanup;
pub mod weekly_report;
pub mod word_clustering;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use tokio_cron_scheduler::{Job, JobScheduler};

use crate::amas::engine::AMASEngine;
use crate::config::WorkerConfig;
use crate::store::Store;

/// Timeout for individual worker invocations (5 minutes).
const WORKER_TIMEOUT: Duration = Duration::from_secs(300);

/// Drain period before scheduler shutdown to let in-flight tasks complete.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobSpec {
    pub name: &'static str,
    pub cron: &'static str,
}

pub struct WorkerManager {
    store: Arc<Store>,
    amas_engine: Arc<AMASEngine>,
    shutdown_rx: broadcast::Receiver<()>,
    config: WorkerConfig,
}

impl WorkerManager {
    pub fn new(
        store: Arc<Store>,
        amas_engine: Arc<AMASEngine>,
        shutdown_rx: broadcast::Receiver<()>,
        config: &WorkerConfig,
    ) -> Self {
        Self {
            store,
            amas_engine,
            shutdown_rx,
            config: config.clone(),
        }
    }

    /// Single source of truth for all planned jobs and their cron schedules.
    pub fn planned_jobs(&self) -> Vec<JobSpec> {
        if !self.config.is_leader {
            return Vec::new();
        }

        let mut jobs = Vec::new();

        if self.config.enable_monitoring {
            jobs.push(JobSpec {
                name: "metrics_flush",
                cron: "0 */5 * * * *",
            });
        }

        jobs.push(JobSpec {
            name: "session_cleanup",
            cron: "0 0 * * * *",
        });

        if self.config.enable_monitoring {
            jobs.push(JobSpec {
                name: "monitoring_aggregate",
                cron: "0 */15 * * * *",
            });
        }

        if self.config.enable_llm_advisor {
            jobs.push(JobSpec {
                name: "llm_advisor",
                cron: "0 */20 * * * *",
            });
        }

        // B43: Delayed reward (every minute)
        jobs.push(JobSpec {
            name: "delayed_reward",
            cron: "0 * * * * *",
        });

        // B44: Forgetting alert (daily at 06:30, staggered)
        jobs.push(JobSpec {
            name: "forgetting_alert",
            cron: "0 30 6 * * *",
        });

        // B45: Algorithm optimization (daily at 0:00)
        jobs.push(JobSpec {
            name: "algorithm_optimization",
            cron: "0 0 0 * * *",
        });

        // B68: Cache cleanup (every 10 min)
        jobs.push(JobSpec {
            name: "cache_cleanup",
            cron: "0 */10 * * * *",
        });

        // B69: Daily aggregation (1:00)
        jobs.push(JobSpec {
            name: "daily_aggregation",
            cron: "0 0 1 * * *",
        });

        // B70: Health analysis (weekly Mon 5:00)
        jobs.push(JobSpec {
            name: "health_analysis",
            cron: "0 0 5 * * 1",
        });

        // B71: Etymology generation (daily 3:30)
        jobs.push(JobSpec {
            name: "etymology_generation",
            cron: "0 30 3 * * *",
        });

        // B72: Embedding generation (5min)
        jobs.push(JobSpec {
            name: "embedding_generation",
            cron: "0 */5 * * * *",
        });

        // B73: Word clustering (weekly Sun 4:00)
        jobs.push(JobSpec {
            name: "word_clustering",
            cron: "0 0 4 * * 0",
        });

        // B74: Confusion pair cache (weekly Sun 5:00)
        jobs.push(JobSpec {
            name: "confusion_pair_cache",
            cron: "0 0 5 * * 0",
        });

        // B75: Weekly report (Mon 06:30, staggered from health_analysis)
        jobs.push(JobSpec {
            name: "weekly_report",
            cron: "0 30 6 * * 1",
        });

        // B76: Log export (hourly)
        jobs.push(JobSpec {
            name: "log_export",
            cron: "0 0 * * * *",
        });

        jobs
    }

    /// Start the worker scheduler. Returns an error if the scheduler cannot be created or started.
    pub async fn start(
        mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.config.is_leader {
            tracing::info!("Worker leader disabled; skipping worker startup");
            return Ok(());
        }

        let mut scheduler = JobScheduler::new().await?;

        self.register_jobs(&scheduler).await;

        scheduler.start().await?;

        tracing::info!("Worker manager started");
        let _ = self.shutdown_rx.recv().await;

        tracing::info!("Worker manager shutting down, draining for {}s", DRAIN_TIMEOUT.as_secs());
        tokio::time::sleep(DRAIN_TIMEOUT).await;
        let _ = scheduler.shutdown().await;
        Ok(())
    }

    /// Register all jobs with the scheduler, using `planned_jobs()` as the single source of truth.
    async fn register_jobs(&self, scheduler: &JobScheduler) {
        let specs = self.planned_jobs();

        for spec in &specs {
            let store = self.store.clone();
            let engine = self.amas_engine.clone();
            let name = spec.name;

            match name {
                "metrics_flush" => {
                    let registry = engine.metrics_registry().clone();
                    add_job(scheduler, spec.cron, name, move || {
                        let store = store.clone();
                        let registry = registry.clone();
                        async move { metrics_flush::run(&registry, &store).await; }
                    }).await;
                }
                "session_cleanup" => {
                    add_job(scheduler, spec.cron, name, move || {
                        let store = store.clone();
                        async move { session_cleanup::run(&store).await; }
                    }).await;
                }
                "monitoring_aggregate" => {
                    add_job(scheduler, spec.cron, name, move || {
                        let store = store.clone();
                        async move { monitoring_aggregate::run(&store).await; }
                    }).await;
                }
                "llm_advisor" => {
                    add_job(scheduler, spec.cron, name, move || {
                        let store = store.clone();
                        async move { llm_advisor::run(&store).await; }
                    }).await;
                }
                "delayed_reward" => {
                    add_job(scheduler, spec.cron, name, move || {
                        let store = store.clone();
                        async move { delayed_reward::run(&store).await; }
                    }).await;
                }
                "forgetting_alert" => {
                    add_job(scheduler, spec.cron, name, move || {
                        let store = store.clone();
                        async move { forgetting_alert::run(&store).await; }
                    }).await;
                }
                "algorithm_optimization" => {
                    add_job(scheduler, spec.cron, name, move || {
                        let store = store.clone();
                        let engine = engine.clone();
                        async move { algorithm_optimization::run(&store, &engine).await; }
                    }).await;
                }
                "cache_cleanup" => {
                    add_job(scheduler, spec.cron, name, move || {
                        let store = store.clone();
                        async move { cache_cleanup::run(&store).await; }
                    }).await;
                }
                "daily_aggregation" => {
                    add_job(scheduler, spec.cron, name, move || {
                        let store = store.clone();
                        async move { daily_aggregation::run(&store).await; }
                    }).await;
                }
                "health_analysis" => {
                    add_job(scheduler, spec.cron, name, move || {
                        let store = store.clone();
                        async move { health_analysis::run(&store).await; }
                    }).await;
                }
                "etymology_generation" => {
                    add_job(scheduler, spec.cron, name, move || {
                        let store = store.clone();
                        async move { etymology_generation::run(&store).await; }
                    }).await;
                }
                "embedding_generation" => {
                    add_job(scheduler, spec.cron, name, move || {
                        let store = store.clone();
                        async move { embedding_generation::run(&store).await; }
                    }).await;
                }
                "word_clustering" => {
                    add_job(scheduler, spec.cron, name, move || {
                        let store = store.clone();
                        async move { word_clustering::run(&store).await; }
                    }).await;
                }
                "confusion_pair_cache" => {
                    add_job(scheduler, spec.cron, name, move || {
                        let store = store.clone();
                        async move { confusion_pair_cache::run(&store).await; }
                    }).await;
                }
                "weekly_report" => {
                    add_job(scheduler, spec.cron, name, move || {
                        let store = store.clone();
                        async move { weekly_report::run(&store).await; }
                    }).await;
                }
                "log_export" => {
                    add_job(scheduler, spec.cron, name, move || {
                        let store = store.clone();
                        async move { log_export::run(&store).await; }
                    }).await;
                }
                unknown => {
                    tracing::error!(name = unknown, "Unknown job name in planned_jobs");
                }
            }
            tracing::info!(name, cron = spec.cron, "Registered worker");
        }
    }
}

/// Add a job to the scheduler with an overlap guard and timeout wrapper.
async fn add_job<Fut, F>(scheduler: &JobScheduler, cron: &str, name: &'static str, mut run: F)
where
    F: FnMut() -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let running = Arc::new(AtomicBool::new(false));

    let job = Job::new_async(cron, move |_uuid, _lock| {
        let guard = running.clone();

        if guard.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            tracing::warn!(worker = name, "Skipping worker invocation: previous run still in progress");
            return Box::pin(async {});
        }

        let fut = run();
        Box::pin(async move {
            match tokio::time::timeout(WORKER_TIMEOUT, fut).await {
                Ok(()) => {}
                Err(_) => {
                    tracing::warn!(worker = name, timeout_secs = WORKER_TIMEOUT.as_secs(), "Worker timed out");
                }
            }
            guard.store(false, Ordering::SeqCst);
        })
    });

    match job {
        Ok(job) => {
            if let Err(err) = scheduler.add(job).await {
                tracing::error!(error=%err, cron, worker = name, "Failed to add worker job");
            }
        }
        Err(err) => tracing::error!(error=%err, cron, worker = name, "Failed to create worker job"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::Utc;
    use tokio::sync::broadcast;

    use crate::amas::config::AMASConfig;
    use crate::amas::engine::AMASEngine;
    use crate::config::Config;
    use crate::store::Store;

    use super::*;

    #[tokio::test]
    async fn leader_switch_controls_job_registration() {
        let cfg = Config::from_env();
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(Store::open(tmp.path().join("worker_test.sled").to_str().unwrap()).unwrap());
        let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
        let (tx, _) = broadcast::channel(2);

        let mut worker_cfg = cfg.worker.clone();
        worker_cfg.is_leader = false;

        let manager = WorkerManager::new(store, amas, tx.subscribe(), &worker_cfg);
        assert!(manager.planned_jobs().is_empty());
    }

    #[tokio::test]
    async fn shutdown_path_is_non_panicking() {
        let cfg = Config::from_env();
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(Store::open(tmp.path().join("worker_test_2.sled").to_str().unwrap()).unwrap());
        let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
        let (tx, _) = broadcast::channel(2);

        let mut worker_cfg = cfg.worker.clone();
        worker_cfg.is_leader = false;

        let manager = WorkerManager::new(store, amas, tx.subscribe(), &worker_cfg);
        // start() now returns Result; non-leader returns Ok(())
        manager.start().await.expect("non-leader start should succeed");

        let _ = Utc::now();
    }
}
