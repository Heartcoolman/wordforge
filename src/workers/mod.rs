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
pub mod password_reset_cleanup;
pub mod session_cleanup;
pub mod weekly_report;
pub mod word_clustering;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Parse timestamp (ms) from a record key formatted as `{user_id}:{reverse_ts:020}:{record_id}`.
pub fn parse_record_timestamp_ms(record_key: &[u8]) -> Option<i64> {
    let first_sep = record_key.iter().position(|b| *b == b':')?;
    let tail = &record_key[first_sep + 1..];
    let second_sep = tail.iter().position(|b| *b == b':')?;
    let reverse_ts_str = std::str::from_utf8(&tail[..second_sep]).ok()?;
    let reverse_ts = reverse_ts_str.parse::<u64>().ok()?;
    let ts_u64 = u64::MAX.checked_sub(reverse_ts)?;
    i64::try_from(ts_u64).ok()
}

/// Parse timestamp (ms) from a monitoring event key formatted as `{reverse_ts:020}:{event_id}`.
pub fn parse_monitoring_event_timestamp_ms(key: &[u8]) -> Option<i64> {
    let sep = key.iter().position(|b| *b == b':')?;
    let reverse_ts_str = std::str::from_utf8(&key[..sep]).ok()?;
    let reverse_ts = reverse_ts_str.parse::<u64>().ok()?;
    let ts_u64 = u64::MAX.checked_sub(reverse_ts)?;
    i64::try_from(ts_u64).ok()
}

use tokio::sync::broadcast;
use tokio_cron_scheduler::{Job, JobScheduler};

use crate::amas::engine::AMASEngine;
use crate::config::WorkerConfig;
use crate::store::Store;

/// Timeout for individual worker invocations (5 minutes).
const WORKER_TIMEOUT: Duration = Duration::from_secs(300);

/// Drain period before scheduler shutdown to let in-flight tasks complete.
#[cfg(test)]
const DRAIN_TIMEOUT: Duration = Duration::from_millis(10);
#[cfg(not(test))]
const DRAIN_TIMEOUT: Duration = Duration::from_secs(30);

/// 所有 worker 的枚举，消除字符串匹配，编译期保证完整性
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkerName {
    MetricsFlush,
    SessionCleanup,
    PasswordResetCleanup,
    MonitoringAggregate,
    LlmAdvisor,
    DelayedReward,
    ForgettingAlert,
    AlgorithmOptimization,
    CacheCleanup,
    DailyAggregation,
    HealthAnalysis,
    EtymologyGeneration,
    EmbeddingGeneration,
    WordClustering,
    ConfusionPairCache,
    WeeklyReport,
    LogExport,
}

impl WorkerName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MetricsFlush => "metrics_flush",
            Self::SessionCleanup => "session_cleanup",
            Self::PasswordResetCleanup => "password_reset_cleanup",
            Self::MonitoringAggregate => "monitoring_aggregate",
            Self::LlmAdvisor => "llm_advisor",
            Self::DelayedReward => "delayed_reward",
            Self::ForgettingAlert => "forgetting_alert",
            Self::AlgorithmOptimization => "algorithm_optimization",
            Self::CacheCleanup => "cache_cleanup",
            Self::DailyAggregation => "daily_aggregation",
            Self::HealthAnalysis => "health_analysis",
            Self::EtymologyGeneration => "etymology_generation",
            Self::EmbeddingGeneration => "embedding_generation",
            Self::WordClustering => "word_clustering",
            Self::ConfusionPairCache => "confusion_pair_cache",
            Self::WeeklyReport => "weekly_report",
            Self::LogExport => "log_export",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobSpec {
    pub name: WorkerName,
    pub cron: &'static str,
    pub enabled: bool,
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

        vec![
            // 核心 worker —— 始终启用
            JobSpec {
                name: WorkerName::SessionCleanup,
                cron: "0 0 * * * *",
                enabled: true,
            },
            JobSpec {
                name: WorkerName::PasswordResetCleanup,
                cron: "0 30 * * * *",
                enabled: true,
            },
            JobSpec {
                name: WorkerName::DelayedReward,
                cron: "0 */5 * * * *", // 降频: 每分钟 -> 每5分钟
                enabled: true,
            },
            JobSpec {
                name: WorkerName::ForgettingAlert,
                cron: "0 30 6 * * *",
                enabled: true,
            },
            JobSpec {
                name: WorkerName::AlgorithmOptimization,
                cron: "0 0 0 * * *",
                enabled: true,
            },
            JobSpec {
                name: WorkerName::CacheCleanup,
                cron: "0 */10 * * * *",
                enabled: true,
            },
            JobSpec {
                name: WorkerName::DailyAggregation,
                cron: "0 0 1 * * *",
                enabled: true,
            },
            JobSpec {
                name: WorkerName::HealthAnalysis,
                cron: "0 0 5 * * 1",
                enabled: true,
            },
            JobSpec {
                name: WorkerName::ConfusionPairCache,
                cron: "0 0 5 * * 0",
                enabled: true,
            },
            JobSpec {
                name: WorkerName::WeeklyReport,
                cron: "0 30 6 * * 1",
                enabled: true,
            },
            JobSpec {
                name: WorkerName::LogExport,
                cron: "0 0 * * * *",
                enabled: true,
            },
            // 条件启用 worker
            JobSpec {
                name: WorkerName::MetricsFlush,
                cron: "0 */5 * * * *",
                enabled: self.config.enable_monitoring,
            },
            JobSpec {
                name: WorkerName::MonitoringAggregate,
                cron: "0 */15 * * * *",
                // WIP: 待监控聚合实现完成后启用
                enabled: false,
            },
            JobSpec {
                name: WorkerName::LlmAdvisor,
                cron: "0 */20 * * * *",
                enabled: self.config.enable_llm_advisor,
            },
            // Stub workers —— 默认禁用
            JobSpec {
                name: WorkerName::EtymologyGeneration,
                cron: "0 30 3 * * *",
                // WIP: 待 LLM provider 就绪后启用
                enabled: false,
            },
            JobSpec {
                name: WorkerName::EmbeddingGeneration,
                cron: "0 */5 * * * *",
                // WIP: 待 LLM provider 就绪后启用
                enabled: false,
            },
            JobSpec {
                name: WorkerName::WordClustering,
                cron: "0 0 4 * * 0",
                // WIP: 待 LLM provider 就绪后启用
                enabled: false,
            },
        ]
    }

    /// Start the worker scheduler. Returns an error if the scheduler cannot be created or started.
    pub async fn start(mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !self.config.is_leader {
            tracing::info!("Worker leader disabled; skipping worker startup");
            return Ok(());
        }

        let mut scheduler = JobScheduler::new().await?;

        self.register_jobs(&scheduler).await;

        scheduler.start().await?;

        tracing::info!("Worker manager started");
        let _ = self.shutdown_rx.recv().await;

        tracing::info!(
            "Worker manager shutting down, draining for {}s",
            DRAIN_TIMEOUT.as_secs()
        );
        tokio::time::sleep(DRAIN_TIMEOUT).await;
        let _ = scheduler.shutdown().await;
        Ok(())
    }

    /// Register all jobs with the scheduler, using `planned_jobs()` as the single source of truth.
    async fn register_jobs(&self, scheduler: &JobScheduler) {
        let specs = self.planned_jobs();

        for spec in &specs {
            if !spec.enabled {
                tracing::info!(name = spec.name.as_str(), "Skipping disabled worker");
                continue;
            }

            let store = self.store.clone();
            let engine = self.amas_engine.clone();
            let name_str = spec.name.as_str();

            match spec.name {
                WorkerName::MetricsFlush => {
                    let registry = engine.metrics_registry().clone();
                    add_job(scheduler, spec.cron, name_str, move || {
                        let store = store.clone();
                        let registry = registry.clone();
                        async move {
                            metrics_flush::run(&registry, &store).await;
                        }
                    })
                    .await;
                }
                WorkerName::SessionCleanup => {
                    add_job(scheduler, spec.cron, name_str, move || {
                        let store = store.clone();
                        async move {
                            session_cleanup::run(&store).await;
                        }
                    })
                    .await;
                }
                WorkerName::PasswordResetCleanup => {
                    add_job(scheduler, spec.cron, name_str, move || {
                        let store = store.clone();
                        async move {
                            password_reset_cleanup::run(&store).await;
                        }
                    })
                    .await;
                }
                WorkerName::MonitoringAggregate => {
                    add_job(scheduler, spec.cron, name_str, move || {
                        let store = store.clone();
                        async move {
                            monitoring_aggregate::run(&store).await;
                        }
                    })
                    .await;
                }
                WorkerName::LlmAdvisor => {
                    add_job(scheduler, spec.cron, name_str, move || {
                        let store = store.clone();
                        async move {
                            llm_advisor::run(&store).await;
                        }
                    })
                    .await;
                }
                WorkerName::DelayedReward => {
                    add_job(scheduler, spec.cron, name_str, move || {
                        let store = store.clone();
                        async move {
                            delayed_reward::run(&store).await;
                        }
                    })
                    .await;
                }
                WorkerName::ForgettingAlert => {
                    add_job(scheduler, spec.cron, name_str, move || {
                        let store = store.clone();
                        async move {
                            forgetting_alert::run(&store).await;
                        }
                    })
                    .await;
                }
                WorkerName::AlgorithmOptimization => {
                    add_job(scheduler, spec.cron, name_str, move || {
                        let store = store.clone();
                        let engine = engine.clone();
                        async move {
                            algorithm_optimization::run(&store, &engine).await;
                        }
                    })
                    .await;
                }
                WorkerName::CacheCleanup => {
                    add_job(scheduler, spec.cron, name_str, move || {
                        let store = store.clone();
                        async move {
                            cache_cleanup::run(&store).await;
                        }
                    })
                    .await;
                }
                WorkerName::DailyAggregation => {
                    add_job(scheduler, spec.cron, name_str, move || {
                        let store = store.clone();
                        async move {
                            daily_aggregation::run(&store).await;
                        }
                    })
                    .await;
                }
                WorkerName::HealthAnalysis => {
                    add_job(scheduler, spec.cron, name_str, move || {
                        let store = store.clone();
                        async move {
                            health_analysis::run(&store).await;
                        }
                    })
                    .await;
                }
                WorkerName::EtymologyGeneration => {
                    add_job(scheduler, spec.cron, name_str, move || {
                        let store = store.clone();
                        async move {
                            etymology_generation::run(&store).await;
                        }
                    })
                    .await;
                }
                WorkerName::EmbeddingGeneration => {
                    add_job(scheduler, spec.cron, name_str, move || {
                        let store = store.clone();
                        async move {
                            embedding_generation::run(&store).await;
                        }
                    })
                    .await;
                }
                WorkerName::WordClustering => {
                    add_job(scheduler, spec.cron, name_str, move || {
                        let store = store.clone();
                        async move {
                            word_clustering::run(&store).await;
                        }
                    })
                    .await;
                }
                WorkerName::ConfusionPairCache => {
                    add_job(scheduler, spec.cron, name_str, move || {
                        let store = store.clone();
                        async move {
                            confusion_pair_cache::run(&store).await;
                        }
                    })
                    .await;
                }
                WorkerName::WeeklyReport => {
                    add_job(scheduler, spec.cron, name_str, move || {
                        let store = store.clone();
                        async move {
                            weekly_report::run(&store).await;
                        }
                    })
                    .await;
                }
                WorkerName::LogExport => {
                    add_job(scheduler, spec.cron, name_str, move || {
                        let store = store.clone();
                        async move {
                            log_export::run(&store).await;
                        }
                    })
                    .await;
                }
            }
            tracing::info!(name = name_str, cron = spec.cron, "Registered worker");
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

        if guard
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            tracing::warn!(
                worker = name,
                "Skipping worker invocation: previous run still in progress"
            );
            return Box::pin(async {});
        }

        let fut = run();
        Box::pin(async move {
            match tokio::time::timeout(WORKER_TIMEOUT, fut).await {
                Ok(()) => {}
                Err(_) => {
                    tracing::error!(
                        worker = name,
                        timeout_secs = WORKER_TIMEOUT.as_secs(),
                        "Worker timed out"
                    );
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
        let store =
            Arc::new(Store::open(tmp.path().join("worker_test.sled").to_str().unwrap()).unwrap());
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
        let store =
            Arc::new(Store::open(tmp.path().join("worker_test_2.sled").to_str().unwrap()).unwrap());
        let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
        let (tx, _) = broadcast::channel(2);

        let mut worker_cfg = cfg.worker.clone();
        worker_cfg.is_leader = false;

        let manager = WorkerManager::new(store, amas, tx.subscribe(), &worker_cfg);
        // start() now returns Result; non-leader returns Ok(())
        manager
            .start()
            .await
            .expect("non-leader start should succeed");

        let _ = Utc::now();
    }

    #[tokio::test]
    async fn stub_workers_disabled_by_default() {
        let cfg = Config::from_env();
        let tmp = tempfile::tempdir().expect("tempdir");
        let store =
            Arc::new(Store::open(tmp.path().join("worker_test_3.sled").to_str().unwrap()).unwrap());
        let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
        let (tx, _) = broadcast::channel(2);

        let mut worker_cfg = cfg.worker.clone();
        worker_cfg.is_leader = true;
        worker_cfg.enable_monitoring = false;
        worker_cfg.enable_llm_advisor = false;

        let manager = WorkerManager::new(store, amas, tx.subscribe(), &worker_cfg);
        let jobs = manager.planned_jobs();

        let stub_names = [
            WorkerName::EtymologyGeneration,
            WorkerName::EmbeddingGeneration,
            WorkerName::WordClustering,
            WorkerName::MonitoringAggregate,
            WorkerName::LlmAdvisor,
        ];

        for stub in &stub_names {
            let spec = jobs.iter().find(|j| j.name == *stub);
            assert!(
                spec.map_or(true, |s| !s.enabled),
                "{:?} should be disabled",
                stub
            );
        }
    }

    #[tokio::test]
    async fn all_worker_names_have_str() {
        // 确保 WorkerName 枚举的每个变体都有对应的 as_str 映射
        let names = [
            WorkerName::MetricsFlush,
            WorkerName::SessionCleanup,
            WorkerName::PasswordResetCleanup,
            WorkerName::MonitoringAggregate,
            WorkerName::LlmAdvisor,
            WorkerName::DelayedReward,
            WorkerName::ForgettingAlert,
            WorkerName::AlgorithmOptimization,
            WorkerName::CacheCleanup,
            WorkerName::DailyAggregation,
            WorkerName::HealthAnalysis,
            WorkerName::EtymologyGeneration,
            WorkerName::EmbeddingGeneration,
            WorkerName::WordClustering,
            WorkerName::ConfusionPairCache,
            WorkerName::WeeklyReport,
            WorkerName::LogExport,
        ];

        for name in &names {
            assert!(!name.as_str().is_empty(), "{:?} has empty str", name);
        }
    }
}
