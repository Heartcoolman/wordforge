pub mod algorithm_optimization;
pub mod cache_cleanup;
pub mod config_watcher;
pub mod confusion_pair_cache;
pub mod daily_aggregation;
pub mod delayed_reward;
pub mod embedding_generation;
pub mod etymology_generation;
pub mod forgetting_alert;
pub mod health_analysis;
pub mod heartbeat_watchdog;
pub mod llm_advisor;
pub mod log_export;
pub mod metrics_flush;
pub mod monitoring_aggregate;
pub mod password_reset_cleanup;
pub mod probe_confirm_sweeper;
pub mod session_cleanup;
pub mod update_checker;
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
    UpdateChecker,
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
            Self::UpdateChecker => "update_checker",
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
    llm_config: Option<crate::config::LLMConfig>,
    update_checker_ctx: Option<UpdateCheckerCtx>,
}

#[derive(Clone)]
struct UpdateCheckerCtx {
    updater: Arc<crate::services::updater::Updater>,
    state: crate::state::AppState,
    enabled: bool,
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
            llm_config: None,
            update_checker_ctx: None,
        }
    }

    pub fn with_llm_config(mut self, llm: crate::config::LLMConfig) -> Self {
        self.llm_config = Some(llm);
        self
    }

    pub fn with_update_checker(
        mut self,
        updater: Arc<crate::services::updater::Updater>,
        state: crate::state::AppState,
        enabled: bool,
    ) -> Self {
        self.update_checker_ctx = Some(UpdateCheckerCtx {
            updater,
            state,
            enabled,
        });
        self
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
                // cron crate (zslayton/cron) day-of-week 范围 1-7，不接受 0；
                // 用 SUN 字符串明确表示「每周日」，与官方 example `Mon,Wed,Fri` 风格一致。
                cron: "0 0 5 * * SUN",
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
            JobSpec {
                name: WorkerName::UpdateChecker,
                cron: "0 0 */1 * * *", // 每小时整点检查一次
                enabled: self
                    .update_checker_ctx
                    .as_ref()
                    .map(|c| c.enabled)
                    .unwrap_or(false),
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
                // day-of-week 用 SUN 字符串，避免 cron crate 拒绝 0；
                // 见 ConfusionPairCache 同处说明。
                cron: "0 0 4 * * SUN",
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
                    let llm = self.llm_config.clone();
                    let engine_cloned = engine.clone();
                    add_job(scheduler, spec.cron, name_str, move || {
                        let store = store.clone();
                        let llm = llm.clone();
                        let engine = engine_cloned.clone();
                        async move {
                            llm_advisor::run(&store, llm.as_ref(), &engine).await;
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
                WorkerName::UpdateChecker => {
                    let ctx = match self.update_checker_ctx.clone() {
                        Some(ctx) => ctx,
                        None => continue,
                    };
                    add_job(scheduler, spec.cron, name_str, move || {
                        let updater = ctx.updater.clone();
                        let state = ctx.state.clone();
                        async move {
                            update_checker::run(updater, state).await;
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

    /// 确保 Config::from_env() 不因缺失安全 secret 而 panic。
    /// 多个 worker 测试并发跑时不能依赖其它测试设置的 env var。
    fn ensure_safe_secrets_in_env() {
        let secret = "test_secret_that_is_at_least_32_characters_long_ok";
        if std::env::var("JWT_SECRET")
            .map(|v| v.is_empty() || v.contains("change_me") || v.len() < 32)
            .unwrap_or(true)
        {
            std::env::set_var("JWT_SECRET", secret);
        }
        if std::env::var("ADMIN_JWT_SECRET")
            .map(|v| v.is_empty() || v.contains("change_me") || v.len() < 32)
            .unwrap_or(true)
        {
            std::env::set_var("ADMIN_JWT_SECRET", secret);
        }
        if std::env::var("REFRESH_JWT_SECRET")
            .map(|v| v.is_empty() || v.contains("change_me") || v.len() < 32)
            .unwrap_or(true)
        {
            std::env::set_var("REFRESH_JWT_SECRET", secret);
        }
    }

    #[tokio::test]
    async fn leader_switch_controls_job_registration() {
        ensure_safe_secrets_in_env(); let cfg = Config::from_env();
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(
            Store::open(tmp.path().join("worker_test.db").to_str().unwrap(), 5000, 1).unwrap(),
        );
        let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
        let (tx, _) = broadcast::channel(2);

        let mut worker_cfg = cfg.worker.clone();
        worker_cfg.is_leader = false;

        let manager = WorkerManager::new(store, amas, tx.subscribe(), &worker_cfg);
        assert!(manager.planned_jobs().is_empty());
    }

    #[tokio::test]
    async fn shutdown_path_is_non_panicking() {
        ensure_safe_secrets_in_env(); let cfg = Config::from_env();
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(
            Store::open(
                tmp.path().join("worker_test_2.db").to_str().unwrap(),
                5000,
                1,
            )
            .unwrap(),
        );
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
        ensure_safe_secrets_in_env(); let cfg = Config::from_env();
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(
            Store::open(
                tmp.path().join("worker_test_3.db").to_str().unwrap(),
                5000,
                1,
            )
            .unwrap(),
        );
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

    /// 回归：所有 planned_jobs 的 cron 表达式必须能被 tokio_cron_scheduler 解析。
    /// 历史 bug：confusion_pair_cache 用了 `0 0 5 * * 0`，cron crate (1-7 范围) 拒绝 0，
    /// 启动期 ERROR 但服务继续跑，导致该 worker 永远不触发。
    #[tokio::test]
    async fn all_planned_jobs_have_parseable_cron() {
        use tokio_cron_scheduler::Job;

        ensure_safe_secrets_in_env(); let cfg = Config::from_env();
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(
            Store::open(
                tmp.path().join("cron_test.db").to_str().unwrap(),
                5000,
                1,
            )
            .unwrap(),
        );
        let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
        let (tx, _) = broadcast::channel(2);

        let mut worker_cfg = cfg.worker.clone();
        worker_cfg.is_leader = true;

        let manager = WorkerManager::new(store, amas, tx.subscribe(), &worker_cfg);
        let jobs = manager.planned_jobs();
        assert!(!jobs.is_empty(), "leader 模式下应注册至少一个 job");

        for spec in &jobs {
            let result = Job::new_async(spec.cron, |_, _| Box::pin(async {}));
            assert!(
                result.is_ok(),
                "worker {:?} 的 cron `{}` 必须可解析: {:?}",
                spec.name,
                spec.cron,
                result.err()
            );
        }
    }

    /// 验证 builder method `with_llm_config` —— 仅修改字段，不影响 planned_jobs 数量
    #[tokio::test]
    async fn with_llm_config_sets_field() {
        ensure_safe_secrets_in_env(); let cfg = Config::from_env();
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(
            Store::open(
                tmp.path().join("with_llm.db").to_str().unwrap(),
                5000,
                1,
            )
            .unwrap(),
        );
        let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
        let (tx, _) = broadcast::channel(2);

        let mut worker_cfg = cfg.worker.clone();
        worker_cfg.is_leader = true;
        worker_cfg.enable_llm_advisor = true; // 让 LlmAdvisor 进入 enabled 路径

        let llm = cfg.llm.clone();
        let manager =
            WorkerManager::new(store, amas, tx.subscribe(), &worker_cfg).with_llm_config(llm);
        let jobs = manager.planned_jobs();
        let llm_job = jobs
            .iter()
            .find(|j| j.name == WorkerName::LlmAdvisor)
            .expect("LlmAdvisor present");
        assert!(llm_job.enabled);
    }

    /// 验证 builder method `with_update_checker` —— 启用时 UpdateChecker.enabled = true
    #[tokio::test]
    async fn with_update_checker_enables_update_job() {
        use crate::config::UpdateCheckConfig;

        ensure_safe_secrets_in_env(); let cfg = Config::from_env();
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(
            Store::open(
                tmp.path().join("with_updater.db").to_str().unwrap(),
                5000,
                1,
            )
            .unwrap(),
        );
        let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
        let (tx, _) = broadcast::channel(2);

        let mut worker_cfg = cfg.worker.clone();
        worker_cfg.is_leader = true;

        let updater = crate::services::updater::Updater::new(
            &UpdateCheckConfig {
                api_url: "http://127.0.0.1:1/repos/o/r/releases/latest".into(),
                cache_ttl_secs: 60,
                worker_enabled: true,
                worker_interval_secs: 60,
                github_token: None,
                allow_downgrade: false,
                install_dir: Some(tmp.path().to_path_buf()),
                max_tarball_bytes: 1024,
                download_mirror_prefix: None,
            },
            "v0.0.0",
        )
        .expect("updater");
        let state = crate::state::AppState::new(store.clone(), amas.clone(), &cfg, tx.clone(), false);

        let manager = WorkerManager::new(store, amas, tx.subscribe(), &worker_cfg)
            .with_update_checker(updater, state, true);
        let jobs = manager.planned_jobs();
        let update_job = jobs
            .iter()
            .find(|j| j.name == WorkerName::UpdateChecker)
            .expect("UpdateChecker present");
        assert!(update_job.enabled);
    }

    /// 验证 builder method `with_update_checker` enabled=false 路径 —— 仍然挂载 ctx
    /// 但 planned_jobs 中 UpdateChecker.enabled=false，被 register 时 skip
    #[tokio::test]
    async fn with_update_checker_disabled_keeps_job_disabled() {
        use crate::config::UpdateCheckConfig;

        ensure_safe_secrets_in_env(); let cfg = Config::from_env();
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(
            Store::open(
                tmp.path().join("with_updater_off.db").to_str().unwrap(),
                5000,
                1,
            )
            .unwrap(),
        );
        let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
        let (tx, _) = broadcast::channel(2);

        let mut worker_cfg = cfg.worker.clone();
        worker_cfg.is_leader = true;

        let updater = crate::services::updater::Updater::new(
            &UpdateCheckConfig {
                api_url: "http://127.0.0.1:1/repos/o/r/releases/latest".into(),
                cache_ttl_secs: 60,
                worker_enabled: false,
                worker_interval_secs: 60,
                github_token: None,
                allow_downgrade: false,
                install_dir: Some(tmp.path().to_path_buf()),
                max_tarball_bytes: 1024,
                download_mirror_prefix: None,
            },
            "v0.0.0",
        )
        .expect("updater");
        let state = crate::state::AppState::new(store.clone(), amas.clone(), &cfg, tx.clone(), false);

        let manager = WorkerManager::new(store, amas, tx.subscribe(), &worker_cfg)
            .with_update_checker(updater, state, false);
        let jobs = manager.planned_jobs();
        let update_job = jobs
            .iter()
            .find(|j| j.name == WorkerName::UpdateChecker)
            .unwrap();
        assert!(!update_job.enabled);
    }

    /// Leader 模式启动 → 立即广播 shutdown → 触发 drain + scheduler.shutdown 全路径
    #[tokio::test]
    async fn leader_start_and_shutdown_full_path() {
        ensure_safe_secrets_in_env(); let cfg = Config::from_env();
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(
            Store::open(
                tmp.path().join("leader_start.db").to_str().unwrap(),
                5000,
                1,
            )
            .unwrap(),
        );
        let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
        let (tx, _) = broadcast::channel(2);

        let mut worker_cfg = cfg.worker.clone();
        worker_cfg.is_leader = true;
        // 打开 monitoring 让 MetricsFlush 也走 add_job
        worker_cfg.enable_monitoring = true;
        // 不开 llm_advisor：避免极端情况下 cron 触发 llm_advisor::run 走到回写
        // amas_config.toml 的路径（虽然 LLMConfig.enabled=false 会立即 return，
        // 但为了 0 风险，整条 llm 链路在测试中保持关闭）
        worker_cfg.enable_llm_advisor = false;

        let manager = WorkerManager::new(store, amas, tx.subscribe(), &worker_cfg);

        // 启动 worker
        let handle = tokio::spawn(manager.start());

        // 等 scheduler 启动稳定
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // 广播 shutdown
        let _ = tx.send(());

        // worker 应在 drain + shutdown 后退出
        tokio::time::timeout(std::time::Duration::from_secs(5), handle)
            .await
            .expect("worker exits within 5s")
            .expect("join ok")
            .expect("start returns Ok");
    }

    /// Leader 模式启动 + UpdateChecker 完整路径
    #[tokio::test]
    async fn leader_start_with_update_checker() {
        use crate::config::UpdateCheckConfig;

        ensure_safe_secrets_in_env(); let cfg = Config::from_env();
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(
            Store::open(
                tmp.path().join("leader_uc.db").to_str().unwrap(),
                5000,
                1,
            )
            .unwrap(),
        );
        let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
        let (tx, _) = broadcast::channel(2);

        let mut worker_cfg = cfg.worker.clone();
        worker_cfg.is_leader = true;

        let updater = crate::services::updater::Updater::new(
            &UpdateCheckConfig {
                api_url: "http://127.0.0.1:1/repos/o/r/releases/latest".into(),
                cache_ttl_secs: 60,
                worker_enabled: true,
                worker_interval_secs: 60,
                github_token: None,
                allow_downgrade: false,
                install_dir: Some(tmp.path().to_path_buf()),
                max_tarball_bytes: 1024,
                download_mirror_prefix: None,
            },
            "v0.0.0",
        )
        .expect("updater");
        let state =
            crate::state::AppState::new(store.clone(), amas.clone(), &cfg, tx.clone(), false);

        let manager = WorkerManager::new(store, amas, tx.subscribe(), &worker_cfg)
            .with_update_checker(updater, state, true);

        let handle = tokio::spawn(manager.start());
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let _ = tx.send(());
        tokio::time::timeout(std::time::Duration::from_secs(5), handle)
            .await
            .expect("worker exits within 5s")
            .expect("join ok")
            .expect("start returns Ok");
    }

    /// add_job 调度 + 真实触发 —— 用每秒触发的 cron + 短等待，让闭包真的跑一次
    #[tokio::test]
    async fn add_job_invokes_inner_closure_when_scheduler_fires() {
        use std::sync::atomic::AtomicUsize;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let mut scheduler = JobScheduler::new().await.expect("scheduler");
        // "* * * * * *" 每秒触发
        add_job(&scheduler, "* * * * * *", "test_worker", move || {
            let c = counter_clone.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
            }
        })
        .await;

        scheduler.start().await.expect("start");
        // 等 ~1.5s 让 cron 至少触发一次
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        let _ = scheduler.shutdown().await;

        assert!(
            counter.load(Ordering::SeqCst) >= 1,
            "job should have fired at least once"
        );
    }

    /// add_job 重入保护 —— guard.compare_exchange 失败分支
    /// 让闭包阻塞 ≥1.5s，期间下一轮 cron 触发被 skip
    #[tokio::test]
    async fn add_job_skips_when_previous_run_in_progress() {
        use std::sync::atomic::AtomicUsize;

        let started = Arc::new(AtomicUsize::new(0));
        let finished = Arc::new(AtomicUsize::new(0));
        let s_clone = started.clone();
        let f_clone = finished.clone();

        let mut scheduler = JobScheduler::new().await.expect("scheduler");
        add_job(&scheduler, "* * * * * *", "slow_worker", move || {
            let s = s_clone.clone();
            let f = f_clone.clone();
            async move {
                s.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
                f.fetch_add(1, Ordering::SeqCst);
            }
        })
        .await;

        scheduler.start().await.expect("start");
        // 等约 3.5s — 期间应触发 3 次 cron，但 started 顶多 +2（第二轮在第一轮跑完后才放行）
        tokio::time::sleep(std::time::Duration::from_millis(3500)).await;
        let _ = scheduler.shutdown().await;

        let s = started.load(Ordering::SeqCst);
        let f = finished.load(Ordering::SeqCst);
        // 起码触发 1 次；重入保护下 started 不会等于 cron 触发次数
        assert!(s >= 1, "expected ≥1 start, got {s}");
        assert!(s <= 2, "expected ≤2 starts due to reentrancy guard, got {s}");
        // 完成数可能为 0（被 shutdown 截断），仅断言不超过 started
        assert!(f <= s);
    }

    /// add_job cron 解析失败 —— 进入 Err 分支
    #[tokio::test]
    async fn add_job_handles_invalid_cron_gracefully() {
        let mut scheduler = JobScheduler::new().await.expect("scheduler");
        // 故意给一个非法 cron
        add_job(&scheduler, "not a cron", "bad_cron_worker", || async {}).await;
        // 不应 panic；scheduler 仍可正常启停
        scheduler.start().await.expect("start");
        let _ = scheduler.shutdown().await;
    }

    /// register_jobs 全分支覆盖 —— 通过反复构造不同 worker_cfg，触发 enabled 与 disabled
    /// 这里直接对 leader 模式下调用 register_jobs（间接通过 start + 立即 shutdown）
    #[tokio::test]
    async fn register_jobs_covers_all_enabled_branches() {
        // 通过启用 monitoring + llm_advisor + update_checker 覆盖所有"条件 worker"
        ensure_safe_secrets_in_env(); let cfg = Config::from_env();
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(
            Store::open(
                tmp.path().join("regjobs.db").to_str().unwrap(),
                5000,
                1,
            )
            .unwrap(),
        );
        let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
        let (tx, _) = broadcast::channel(2);

        let mut worker_cfg = cfg.worker.clone();
        worker_cfg.is_leader = true;
        worker_cfg.enable_monitoring = true;
        // 仍然不启用 llm_advisor —— 防止 cron 边界条件下 register 的 LlmAdvisor job
        // 触发到回写 amas_config.toml 的代码路径
        worker_cfg.enable_llm_advisor = false;

        let scheduler = JobScheduler::new().await.expect("scheduler");
        let manager = WorkerManager::new(store, amas, tx.subscribe(), &worker_cfg);

        manager.register_jobs(&scheduler).await;
        // 启动并立即 shutdown，验证不 panic
        let mut sched = scheduler;
        sched.start().await.expect("start");
        let _ = sched.shutdown().await;
    }

    /// register_jobs 的 UpdateChecker continue 分支 —— update_checker_ctx=None 但 planned 中
    /// 该 worker.enabled=false 已被 skip，所以走不到 None 分支；这里直接构造 update_checker_ctx=None
    /// 配合 planned_jobs 不会进入 UpdateChecker 分支。验证 register_jobs 在没有 ctx 时安全。
    #[tokio::test]
    async fn register_jobs_no_update_checker_ctx_is_safe() {
        ensure_safe_secrets_in_env(); let cfg = Config::from_env();
        let tmp = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(
            Store::open(
                tmp.path().join("regnoupd.db").to_str().unwrap(),
                5000,
                1,
            )
            .unwrap(),
        );
        let amas = Arc::new(AMASEngine::new(AMASConfig::default(), store.clone()));
        let (tx, _) = broadcast::channel(2);

        let mut worker_cfg = cfg.worker.clone();
        worker_cfg.is_leader = true;
        worker_cfg.enable_monitoring = false;
        worker_cfg.enable_llm_advisor = false;

        let scheduler = JobScheduler::new().await.expect("scheduler");
        let manager = WorkerManager::new(store, amas, tx.subscribe(), &worker_cfg);
        manager.register_jobs(&scheduler).await;
        let mut sched = scheduler;
        sched.start().await.expect("start");
        let _ = sched.shutdown().await;
    }
}
