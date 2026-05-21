pub mod algorithm_optimization;
pub mod cache_cleanup;
pub mod config_watcher;
pub mod confusion_pair_cache;
pub mod daily_aggregation;
pub mod delayed_reward;
pub mod error_rate_watchdog;
pub mod forgetting_alert;
pub mod health_analysis;
pub mod heartbeat_watchdog;
pub mod llm_advisor;
pub mod log_export;
pub mod metrics_flush;
pub mod monitoring_retention;
pub mod password_reset_cleanup;
pub mod probe_cleanup;
pub mod probe_confirm_sweeper;
pub mod session_cleanup;
pub mod update_checker;
pub mod weekly_report;

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
    LlmAdvisor,
    DelayedReward,
    ForgettingAlert,
    AlgorithmOptimization,
    CacheCleanup,
    DailyAggregation,
    HealthAnalysis,
    ConfusionPairCache,
    WeeklyReport,
    LogExport,
    UpdateChecker,
    /// M0-P3：monitoring_events retention + 月度 VACUUM
    MonitoringRetention,
    /// M0-P4：滚动 5 分钟 5xx 错误率告警，超过 1% 广播 incident SSE 事件
    ErrorRateWatchdog,
}

impl WorkerName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MetricsFlush => "metrics_flush",
            Self::SessionCleanup => "session_cleanup",
            Self::PasswordResetCleanup => "password_reset_cleanup",
            Self::LlmAdvisor => "llm_advisor",
            Self::DelayedReward => "delayed_reward",
            Self::ForgettingAlert => "forgetting_alert",
            Self::AlgorithmOptimization => "algorithm_optimization",
            Self::CacheCleanup => "cache_cleanup",
            Self::DailyAggregation => "daily_aggregation",
            Self::HealthAnalysis => "health_analysis",
            Self::ConfusionPairCache => "confusion_pair_cache",
            Self::WeeklyReport => "weekly_report",
            Self::LogExport => "log_export",
            Self::UpdateChecker => "update_checker",
            Self::MonitoringRetention => "monitoring_retention",
            Self::ErrorRateWatchdog => "error_rate_watchdog",
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
    /// M0-P4：error_rate_watchdog 需要 AppState 广播 SSE incident 事件
    watchdog_state: Option<crate::state::AppState>,
    llm_advisor_state: Option<crate::state::AppState>,
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
            watchdog_state: None,
            llm_advisor_state: None,
        }
    }

    /// M0-P4：注入 AppState 以启用 error_rate_watchdog。
    pub fn with_watchdog_state(mut self, state: crate::state::AppState) -> Self {
        self.watchdog_state = Some(state);
        self
    }

    pub fn with_llm_config(mut self, llm: crate::config::LLMConfig) -> Self {
        self.llm_config = Some(llm);
        self
    }

    pub fn with_llm_advisor_state(mut self, state: crate::state::AppState) -> Self {
        self.llm_advisor_state = Some(state);
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
                cron: "0 */5 * * * *",
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
                // cron crate day-of-week 范围 1-7，用 SUN 字符串明确表示「每周日」
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
                name: WorkerName::LlmAdvisor,
                cron: "0 */20 * * * *",
                enabled: self.config.enable_llm_advisor,
            },
            JobSpec {
                name: WorkerName::UpdateChecker,
                cron: "0 0 */1 * * *",
                enabled: self
                    .update_checker_ctx
                    .as_ref()
                    .map(|c| c.enabled)
                    .unwrap_or(false),
            },
            // M0-P3：monitoring_events 30 天 retention + 月度 VACUUM（每月 1 日 UTC 03:00）
            JobSpec {
                name: WorkerName::MonitoringRetention,
                cron: "0 0 3 1 * *",
                enabled: true,
            },
            // M0-P4：5xx 错误率滚动监控，每分钟采样，超 1% 广播 incident SSE 事件
            JobSpec {
                name: WorkerName::ErrorRateWatchdog,
                cron: "0 * * * * *",
                enabled: self.watchdog_state.is_some(),
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
                WorkerName::LlmAdvisor => {
                    let llm = self.llm_config.clone();
                    let engine_cloned = engine.clone();
                    add_job(scheduler, spec.cron, name_str, move || {
                        let store = store.clone();
                        let llm = llm.clone();
                        let engine = engine_cloned.clone();
                        async move {
                            llm_advisor::run(&store, llm.as_ref(), &engine, None).await;
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
                // M0-P3：monitoring_events retention + 月度 VACUUM
                WorkerName::MonitoringRetention => {
                    add_job(scheduler, spec.cron, name_str, move || {
                        let store = store.clone();
                        async move {
                            monitoring_retention::run(&store).await;
                        }
                    })
                    .await;
                }
                // M0-P4：5xx 错误率 watchdog
                WorkerName::ErrorRateWatchdog => {
                    let watchdog_state = match self.watchdog_state.clone() {
                        Some(s) => s,
                        None => continue,
                    };
                    add_job(scheduler, spec.cron, name_str, move || {
                        let state = watchdog_state.clone();
                        async move {
                            error_rate_watchdog::run(&state).await;
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
        manager
            .start()
            .await
            .expect("non-leader start should succeed");

        let _ = Utc::now();
    }

    #[tokio::test]
    async fn stub_workers_disabled_by_default() {
        // M1-A3：4 个 stub worker 已删除；验证 LlmAdvisor 条件禁用
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

        let spec = jobs.iter().find(|j| j.name == WorkerName::LlmAdvisor);
        assert!(
            spec.map_or(true, |s| !s.enabled),
            "LlmAdvisor should be disabled"
        );
    }

    #[tokio::test]
    async fn all_worker_names_have_str() {
        let names = [
            WorkerName::MetricsFlush,
            WorkerName::SessionCleanup,
            WorkerName::PasswordResetCleanup,
            WorkerName::LlmAdvisor,
            WorkerName::DelayedReward,
            WorkerName::ForgettingAlert,
            WorkerName::AlgorithmOptimization,
            WorkerName::CacheCleanup,
            WorkerName::DailyAggregation,
            WorkerName::HealthAnalysis,
            WorkerName::ConfusionPairCache,
            WorkerName::WeeklyReport,
            WorkerName::LogExport,
        ];

        for name in &names {
            assert!(!name.as_str().is_empty(), "{:?} has empty str", name);
        }
    }

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
        worker_cfg.enable_llm_advisor = true;

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
        worker_cfg.enable_monitoring = true;
        worker_cfg.enable_llm_advisor = false;

        let manager = WorkerManager::new(store, amas, tx.subscribe(), &worker_cfg);

        let handle = tokio::spawn(manager.start());
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let _ = tx.send(());
        tokio::time::timeout(std::time::Duration::from_secs(5), handle)
            .await
            .expect("worker exits within 5s")
            .expect("join ok")
            .expect("start returns Ok");
    }

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

    #[tokio::test]
    async fn add_job_invokes_inner_closure_when_scheduler_fires() {
        use std::sync::atomic::AtomicUsize;

        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let mut scheduler = JobScheduler::new().await.expect("scheduler");
        add_job(&scheduler, "* * * * * *", "test_worker", move || {
            let c = counter_clone.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
            }
        })
        .await;

        scheduler.start().await.expect("start");
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        let _ = scheduler.shutdown().await;

        assert!(
            counter.load(Ordering::SeqCst) >= 1,
            "job should have fired at least once"
        );
    }

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
        tokio::time::sleep(std::time::Duration::from_millis(3500)).await;
        let _ = scheduler.shutdown().await;

        let s = started.load(Ordering::SeqCst);
        let f = finished.load(Ordering::SeqCst);
        assert!(s >= 1, "expected >=1 start, got {s}");
        assert!(s <= 2, "expected <=2 starts due to reentrancy guard, got {s}");
        assert!(f <= s);
    }

    #[tokio::test]
    async fn add_job_handles_invalid_cron_gracefully() {
        let mut scheduler = JobScheduler::new().await.expect("scheduler");
        add_job(&scheduler, "not a cron", "bad_cron_worker", || async {}).await;
        scheduler.start().await.expect("start");
        let _ = scheduler.shutdown().await;
    }

    #[tokio::test]
    async fn register_jobs_covers_all_enabled_branches() {
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
        worker_cfg.enable_llm_advisor = false;

        let scheduler = JobScheduler::new().await.expect("scheduler");
        let manager = WorkerManager::new(store, amas, tx.subscribe(), &worker_cfg);

        manager.register_jobs(&scheduler).await;
        let mut sched = scheduler;
        sched.start().await.expect("start");
        let _ = sched.shutdown().await;
    }

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
