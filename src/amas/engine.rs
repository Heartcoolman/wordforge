use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};

use crate::amas::config::AMASConfig;
use crate::amas::constants::{SIGNAL_THRESHOLD, TREND_BASELINE, USER_LOCK_CLEANUP_THRESHOLD};
use crate::amas::decision::{ensemble, heuristic, ige, swd};
use crate::amas::experiment::ActiveExperiment;
use crate::amas::memory::{evm, mastery, mdm, ssp};
use crate::amas::metrics;
use crate::amas::monitoring;
use crate::amas::types::*;
use crate::response::AppError;
use crate::store::Store;

fn sanitize_float(value: f64, default: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        default
    }
}

#[derive(Clone)]
pub struct AMASEngine {
    config: Arc<RwLock<Arc<AMASConfig>>>,
    config_hash: Arc<RwLock<String>>,
    store: Arc<Store>,
    user_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    metrics_registry: Arc<metrics::MetricsRegistry>,
    ssp_policy: Arc<RwLock<Option<Arc<ssp::SspPolicy>>>>,
    /// 热路径快路:无任何 active canary 时跳过 effective_config_for_user 的 1-2 次 SQLite 查询。
    /// 安全不变式:**永不会错误地为 false**——仅在一次查询确认零 active canary 后置 false;
    /// 任何激活 canary 的写路径(set_canary / create_canary / scale_canary)必须调
    /// mark_canary_active() 置 true。stale-true 只是多查一次(无害),stale-false 才会漏路由。
    canary_active: Arc<AtomicBool>,
    /// #22:canary 激活代际计数。mark_canary_active() 每次自增;effective_config_for_user 在两次
    /// DB 读之前抓代际快照,确认零 active 后仅当代际未变才自清快路标志。激活路径"先提交 DB 行、
    /// 后置标志"的时序下,若清理事件读表恰在激活提交之前发生,激活会顺带 bump 代际,使这次基于旧
    /// 视图的自清因代际不符而放弃——封死"误清刚激活 canary"的丢更新竞态。
    canary_generation: Arc<AtomicU64>,
    /// T1.3 A/B：运行中实验的精简快照（供热路径按桶定 arm）。None=无 running 实验。
    /// 与 canary 路由解耦：A/A（同配置两桶）即便不改路由配置，也需按桶分 arm 入组。
    active_experiment: Arc<RwLock<Option<ActiveExperiment>>>,
    /// 快路门：无 running 实验时跳过实验入组。register/conclude 后经 reload_active_experiment 维护。
    experiment_active: Arc<AtomicBool>,
}

/// 路由结果：本次请求生效配置 + 用户桶号 + 命中的 canary version_hash（None=stable）。
pub struct RouteResult {
    pub config: Arc<AMASConfig>,
    pub bucket: u32,
    pub hit_version_hash: Option<String>,
}

/// ExperimentRow → 引擎侧 ActiveExperiment 快照（含 A/A 判定：canary 与 baseline 同配置版本）。
fn active_exp_snapshot(
    e: crate::store::operations::amas_experiment::ExperimentRow,
) -> ActiveExperiment {
    let aa = e.canary_version_hash == e.baseline_version_hash;
    ActiveExperiment {
        experiment_id: e.experiment_id,
        canary_cohort_lo: e.canary_cohort_lo,
        canary_cohort_hi: e.canary_cohort_hi,
        canary_version_hash: e.canary_version_hash,
        aa,
    }
}

#[derive(Debug, Clone, Default)]
pub struct AlgoStates {
    pub ige: ige::IgeState,
    pub swd: swd::SwdState,
    pub trust_scores: ensemble::TrustScores,
}

#[derive(Debug, Clone)]
struct MemoryFeedback {
    decision: WordMasteryDecision,
    scheduled_recall: f64,
    #[allow(dead_code)]
    desired_retention: f64,
    /// T1.3:本次答题导致的 mastery 态转移("mastered"/"forgotten"/None)，用于落 word_mastery_events
    /// 支撑"长期 masteredCount"。仅记进入 MASTERED / 进入 FORGOTTEN 的边沿。
    mastery_transition: Option<&'static str>,
}

struct ProcessingContext {
    config: Arc<AMASConfig>,
    user_state: UserState,
    algo_states: AlgoStates,
    feature: FeatureVector,
    cold_start_phase: Option<ColdStartPhase>,
}

struct StrategySelection {
    final_strategy: StrategyParams,
    constrained_strategy: StrategyParams,
    candidates: Vec<DecisionCandidate>,
    weights: HashMap<AlgorithmId, f64>,
}

struct MemoryScoring {
    word_mastery: Option<MemoryFeedback>,
    reward: Reward,
    objective: ObjectiveEvaluation,
    /// W1-1：update_memory 期间累积的 per-word 状态（mastery/IAD/MTP/EVM），延后由
    /// persist_state 与幂等标记同一 tx 原子写入，消除崩溃重试二次累加记忆模型的窗口。
    pending_algo: Vec<(String, serde_json::Value)>,
}

impl AMASEngine {
    pub fn new(config: AMASConfig, store: Arc<Store>) -> Self {
        let hash = monitoring::compute_config_hash(&config);
        let ssp_policy = if config.feature_flags.ssp_enabled {
            let result = ssp::precompute(&config.ssp, &config.memory_model);
            // T1.4：保留 DP 的 (S,D)→最优 R 曲面（Cost-ADR 状态相关 DR），供查询/对拍。
            Some(Arc::new(
                if result.dual_grid {
                    ssp::SspPolicy::from_tables_with_bins(result.tables, result.stability_list)
                } else {
                    ssp::SspPolicy::from_tables(result.tables, &config.ssp)
                }
                .with_retention_tables(result.optimal_r),
            ))
        } else {
            None
        };
        // T1.3：构造时载入 running 实验快照（重启后恢复实验入组，无需等首个 register）。
        let active_exp = store.get_running_experiment().ok().flatten().map(active_exp_snapshot);
        let exp_active = active_exp.is_some();
        Self {
            config: Arc::new(RwLock::new(Arc::new(config))),
            config_hash: Arc::new(RwLock::new(hash)),
            store,
            user_locks: Arc::new(Mutex::new(HashMap::new())),
            metrics_registry: Arc::new(metrics::MetricsRegistry::new()),
            ssp_policy: Arc::new(RwLock::new(ssp_policy)),
            // 初值 true:首个事件会真查一次并自校正为 false(若无 canary),不漏判。
            canary_active: Arc::new(AtomicBool::new(true)),
            canary_generation: Arc::new(AtomicU64::new(0)),
            active_experiment: Arc::new(RwLock::new(active_exp)),
            experiment_active: Arc::new(AtomicBool::new(exp_active)),
        }
    }

    /// T1.3：重载运行中实验快照。register_experiment / conclude_experiment 之后调用，
    /// 使热路径的实验入组门即时生效。
    pub fn reload_active_experiment(&self) {
        let active = self
            .store
            .get_running_experiment()
            .ok()
            .flatten()
            .map(active_exp_snapshot);
        self.experiment_active
            .store(active.is_some(), Ordering::Release);
        *self.active_experiment.write() = active;
    }

    /// 标记"可能存在 active canary",激活 canary 的写路径调用后保证热路径不再跳过查询。
    /// #22:先 bump 代际再置标志,使任何"基于激活前 DB 视图"的在途自清因代际不符而作废。
    pub fn mark_canary_active(&self) {
        self.canary_generation.fetch_add(1, Ordering::AcqRel);
        self.canary_active.store(true, Ordering::Release);
    }

    pub fn reload_config(&self, new_config: AMASConfig) -> Result<(), String> {
        new_config.validate()?;
        let hash = monitoring::compute_config_hash(&new_config);
        let new_ssp = if new_config.feature_flags.ssp_enabled {
            let result = ssp::precompute(&new_config.ssp, &new_config.memory_model);
            // T1.4：保留 DP 的 (S,D)→最优 R 曲面（Cost-ADR 状态相关 DR），供查询/对拍。
            Some(Arc::new(
                if result.dual_grid {
                    ssp::SspPolicy::from_tables_with_bins(result.tables, result.stability_list)
                } else {
                    ssp::SspPolicy::from_tables(result.tables, &new_config.ssp)
                }
                .with_retention_tables(result.optimal_r),
            ))
        } else {
            None
        };
        *self.config.write() = Arc::new(new_config);
        *self.config_hash.write() = hash;
        *self.ssp_policy.write() = new_ssp;
        tracing::info!("AMAS config reloaded");
        Ok(())
    }

    pub fn ssp_policy(&self) -> Option<Arc<ssp::SspPolicy>> {
        self.ssp_policy.read().clone()
    }

    pub fn get_config(&self) -> AMASConfig {
        self.config.read().as_ref().clone()
    }

    /// C6:按 user_id 解析"有效配置"(per-patch canary 子系统)。
    ///   1. 先遍历 get_active_patch_canaries(),按 hash(user_id)%100 ∈ [cohort_lo,cohort_hi)
    ///      命中其一 → 从 amas_config_versions 加载该 version snapshot。
    ///   2. 未命中任何 active patch canary → 回退到既有单 active `amas_canary_config` 路由
    ///      (m022 逻辑,保持兼容)。
    ///   3. 任一环节反序列化失败 / version 缺失 / 查表失败 → 回退 stable(打 warn log 不抛错)。
    ///
    /// process_event_blocking 在入口调一次此方法,后续整个 request 内的 ProcessingContext.config
    /// 都用同一份 Arc,保证一次请求内 config 一致。
    pub fn effective_config_for_user(&self, user_id: &str) -> Arc<AMASConfig> {
        self.route_for_user(user_id).config
    }

    /// hash(user_id) % 100 桶号；patch canary / m022 单 active / 实验 arm 共用同一散列。
    fn bucket_for(user_id: &str) -> u32 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        user_id.hash(&mut h);
        (h.finish() % 100) as u32
    }

    /// C6 路由 + 桶身份。返回本次请求生效配置、用户桶号、命中的 canary version_hash。
    /// `effective_config_for_user` 是其薄封装。canary 快路 / 代际自清不变式原样保留（仅返回类型
    /// 从 Arc<AMASConfig> 扩为携带 bucket+hit）。
    pub fn route_for_user(&self, user_id: &str) -> RouteResult {
        let stable: Arc<AMASConfig> = Arc::clone(&self.config.read());
        let bucket = Self::bucket_for(user_id);
        // 快路:确认无 active canary 时跳过下面 1-2 次 SQLite 查询(常态)。
        if !self.canary_active.load(Ordering::Acquire) {
            return RouteResult {
                config: stable,
                bucket,
                hit_version_hash: None,
            };
        }
        // #22:抓代际快照(必须在两次 DB 读之前)。自清仅在代际未被 mark_canary_active bump 时才允许。
        let gen_snapshot = self.canary_generation.load(Ordering::Acquire);

        // any_active:本轮是否观测到任一 active canary。仅当确认零 active 时才把快路标志置 false。
        // 查询出错时保守视为 true(不清标志),避免错误跳过。
        let mut any_active = false;

        // ① per-patch canary:多条并行 active,cohort 区间命中其一
        match self.store.get_active_patch_canaries() {
            Ok(canaries) => {
                any_active |= !canaries.is_empty();
                if let Some(hit) = canaries
                    .iter()
                    .find(|c| bucket >= c.cohort_lo && bucket < c.cohort_hi)
                {
                    let config = self.load_canary_snapshot(&hit.version_hash, &stable);
                    return RouteResult {
                        config,
                        bucket,
                        hit_version_hash: Some(hit.version_hash.clone()),
                    };
                }
            }
            Err(e) => {
                any_active = true;
                tracing::warn!(error=%e, "route_for_user: 查 patch canary 失败,回退既有路由");
            }
        }

        // ② 兼容既有单 active amas_canary_config(m022)
        let canary = match self.store.get_active_amas_canary() {
            Ok(Some(c)) => c,
            Ok(None) => {
                // 两类 canary 均无 active → 清快路标志,后续事件直接走快路。
                // #22:仅当代际自抓取快照以来未被 mark_canary_active bump(即两次 DB 读期间无并发激活),
                // 且标志仍为 true 时才自清。代际守护封死"读到激活提交前的空表→误清刚激活 canary"窗口;
                // CAS(true→false) 再叠一层防 store 丢更新。代际变了就放弃自清,下个事件重查(无害多查一次)。
                if !any_active
                    && self.canary_generation.load(Ordering::Acquire) == gen_snapshot
                {
                    let _ = self.canary_active.compare_exchange(
                        true,
                        false,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    );
                }
                return RouteResult {
                    config: stable,
                    bucket,
                    hit_version_hash: None,
                };
            }
            Err(e) => {
                tracing::warn!(error=%e, "route_for_user: 查 canary 失败,回退 stable");
                return RouteResult {
                    config: stable,
                    bucket,
                    hit_version_hash: None,
                };
            }
        };
        let is_forced = canary.force_user_ids.iter().any(|id| id == user_id);
        let in_bucket = (bucket as u64) < canary.percent as u64;
        if !is_forced && !in_bucket {
            return RouteResult {
                config: stable,
                bucket,
                hit_version_hash: None,
            };
        }
        // m035 crowd_filters 接线:非强制命中的用户,还须落入人群过滤(平台/账龄/活跃)。
        // force_user_ids 优先,无条件命中。查询出错保守回退 stable(不误推 canary)。
        if !is_forced {
            if let Some(filters) = &canary.crowd_filters {
                match self.store.user_matches_crowd_filters(user_id, filters) {
                    Ok(true) => {}
                    Ok(false) => {
                        return RouteResult {
                            config: stable,
                            bucket,
                            hit_version_hash: None,
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error=%e, "route_for_user: 查 crowd_filters 失败,回退 stable");
                        return RouteResult {
                            config: stable,
                            bucket,
                            hit_version_hash: None,
                        };
                    }
                }
            }
        }
        let config = self.load_canary_snapshot(&canary.version_hash, &stable);
        RouteResult {
            config,
            bucket,
            hit_version_hash: Some(canary.version_hash.clone()),
        }
    }

    /// T1.3：有 running 实验时按桶定 arm 并冻结入组（INSERT OR IGNORE），返回 (experiment_id, arm)。
    /// 无 running 实验返回 None。须在用户锁内调用并把结果传给 emit，避免 drop 锁后读到变更后的实验状态。
    fn assign_experiment_arm(
        &self,
        user_id: &str,
        bucket: u32,
        hit_version_hash: Option<&str>,
        now: &chrono::DateTime<chrono::Utc>,
    ) -> Option<(String, &'static str)> {
        if !self.experiment_active.load(Ordering::Acquire) {
            return None;
        }
        let exp = self.active_experiment.read().clone()?;
        // 按本次实际服务的配置定 arm（A/B 防 cohort/percent 错配静默误归因；A/A 按 cohort 切）。
        let arm = exp.arm_for(bucket, hit_version_hash);
        if let Err(e) = self.store.enroll_user_bucket(
            user_id,
            &exp.experiment_id,
            arm.as_str(),
            bucket,
            &now.to_rfc3339(),
        ) {
            tracing::warn!(error=%e, "assign_experiment_arm: enroll_user_bucket 失败");
        }
        Some((exp.experiment_id, arm.as_str()))
    }

    /// 从 amas_config_versions 拉 version snapshot,失败/缺失回退 stable + warn。
    fn load_canary_snapshot(
        &self,
        version_hash: &str,
        stable: &Arc<AMASConfig>,
    ) -> Arc<AMASConfig> {
        match self.store.get_amas_config_version(version_hash) {
            Ok(Some(version)) => {
                match serde_json::from_value::<AMASConfig>(version.snapshot_json) {
                    Ok(cfg) => Arc::new(cfg),
                    Err(e) => {
                        tracing::warn!(
                            version_hash = %version_hash,
                            error = %e,
                            "effective_config_for_user: 反序列化 canary snapshot 失败,回退 stable"
                        );
                        Arc::clone(stable)
                    }
                }
            }
            Ok(None) => {
                tracing::warn!(
                    version_hash = %version_hash,
                    "effective_config_for_user: canary version_hash 在 amas_config_versions 中不存在,回退 stable"
                );
                Arc::clone(stable)
            }
            Err(e) => {
                tracing::warn!(error=%e, "effective_config_for_user: 加载 canary version 失败,回退 stable");
                Arc::clone(stable)
            }
        }
    }

    pub fn metrics_registry(&self) -> &Arc<metrics::MetricsRegistry> {
        &self.metrics_registry
    }

    pub fn is_healthy(&self) -> bool {
        if self.get_config().validate().is_err() {
            return false;
        }
        let ssp_enabled = self.get_config().feature_flags.ssp_enabled;
        if ssp_enabled && self.ssp_policy().is_none() {
            return false;
        }
        true
    }

    fn acquire_user_lock_blocking(&self, user_id: &str) -> Arc<Mutex<()>> {
        let mut locks = self.user_locks.lock();

        if locks.len() > USER_LOCK_CLEANUP_THRESHOLD {
            let before = locks.len();
            locks.retain(|_, v| Arc::strong_count(v) > 1);
            let removed = before - locks.len();
            if removed > 0 {
                tracing::info!(
                    before_count = before,
                    after_count = locks.len(),
                    removed_count = removed,
                    "清理空闲用户锁"
                );
            }
        }

        locks
            .entry(user_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    pub async fn process_event(
        &self,
        user_id: &str,
        raw_event: RawEvent,
    ) -> Result<ProcessResult, AppError> {
        let engine = self.clone();
        let user_id = user_id.to_string();
        let result = crate::blocking::run_blocking("amas.process_event", move || {
            engine.process_event_blocking(&user_id, raw_event, None)
        })
        .await??;
        // 非幂等路径不传幂等键，persist_state 恒返回 true（无标记可竞争），故必为 Some。
        result.ok_or_else(|| AppError::internal("process_event returned None without idempotency key"))
    }

    /// W1-1：幂等版 process_event。把 `client_record_id` 作为幂等键，在 AMAS 状态落库的
    /// 同一 tx 内原子写入 `processed_events` 标记。配合路由侧 `is_event_processed` 前置预检
    /// （命中即不调本方法），保证 outbox 重放 / 客户端重试不二次累加 AMAS 状态。
    /// 与 [`Self::process_event`] 唯一差别是落库时附带幂等标记；处理逻辑零分叉。
    /// 返回 `Ok(None)` 表示幂等标记已被并发同 `client_record_id` 请求抢先写入、本次 AMAS 增量已
    /// 整笔回滚未生效——调用方应据此走"裸记录回放"而非二次累加 ELO/mastery/trust。
    pub async fn process_event_idempotent(
        &self,
        user_id: &str,
        raw_event: RawEvent,
        client_record_id: &str,
    ) -> Result<Option<ProcessResult>, AppError> {
        let engine = self.clone();
        let user_id = user_id.to_string();
        let key = client_record_id.to_string();
        crate::blocking::run_blocking("amas.process_event", move || {
            engine.process_event_blocking(&user_id, raw_event, Some(&key))
        })
        .await?
    }

    /// 返回 `Ok(None)` 仅当传入幂等键且其标记已被并发请求抢先写入——本次 AMAS 增量已整笔回滚、
    /// 未生效，调用方应据此走"裸记录回放"而非二次累加。`idempotency_key` 为 `None` 时恒 `Some`。
    fn process_event_blocking(
        &self,
        user_id: &str,
        raw_event: RawEvent,
        idempotency_key: Option<&str>,
    ) -> Result<Option<ProcessResult>, AppError> {
        let start = std::time::Instant::now();

        let user_lock = self.acquire_user_lock_blocking(user_id);
        let _guard = user_lock.lock();

        // m022:按 user_id 做 canary 抽样,可能返回 canary version 而非全局 stable。
        let route = self.route_for_user(user_id);
        let config = Arc::clone(&route.config);
        let now = chrono::Utc::now();
        // T1.3:有 running 实验时按桶冻结 arm 入组,捕获 (experiment_id, arm) 供 emit 透传。
        // 在用户锁内捕获,避免 drop(_guard) 后读到变更后的实验/canary 状态。arm 按本次实际命中的
        // 配置(route.hit_version_hash)判定,杜绝 cohort/percent 错配把 stable 结果误归 canary 臂。
        let experiment =
            self.assign_experiment_arm(user_id, route.bucket, route.hit_version_hash.as_deref(), &now);
        let mut context = self.prepare_processing_context(user_id, &raw_event, config, now)?;
        let strategy = self.select_strategy(&mut context);
        let mut scoring = self.apply_memory_and_score(user_id, &raw_event, &context, &strategy)?;
        // W1-1：取出 per-word 待写状态，交由 persist_state 与幂等标记同 tx 原子提交。
        let pending_algo = std::mem::take(&mut scoring.pending_algo);
        // T1.3:本次 mastery 态边沿(在 scoring 被后续消费前捕获)。
        let mastery_transition = scoring.word_mastery.as_ref().and_then(|f| f.mastery_transition);

        self.update_trust_scores(
            &mut context.algo_states,
            &strategy.candidates,
            scoring.reward.value,
            scoring.objective.score,
            &context.user_state,
            &strategy.weights,
            &context.config,
        );

        Self::update_session_counters(&mut context.user_state, &raw_event, now);
        // #11/#14/#39：ELO 的 read-modify-write 收进 persist_state 的同一原子 tx，与 user_state/
        // algo_states/幂等标记全有或全无。**仅幂等路径（携带 client_record_id 的 records 流）应用 ELO**：
        // word_elo 是全局共享、被全员选词读取的写，只应由正规学习事件驱动。非幂等诊断端点
        // （/api/amas/process-event、/batch-process）不带幂等键，既无重试去重也无回滚快照——若在此写
        // ELO，重试会双重累加 user_elo、并污染全局 word_elo（本 PR 前这些端点对 ELO 零副作用）。
        // 故 idempotency_key 为 None 时不带 ELO；word_id 为空（纯 quit 事件）同样不带。
        let elo_spec = (idempotency_key.is_some() && !raw_event.word_id.is_empty()).then(|| {
            // #14 抗投毒上限固定取 **stable 基线** ELO 参数派生（非 per-event effective config）：把单
            // 用户对某词全局评分的累计净位移钳在该词单次最大可能位移（novice 期 k_word）的 4 倍内。账本
            // word_elo_user_contrib 是 per-(user,word) 全局单一累加器、无配置版本维度；若 cap 改用 canary
            // 的 effective config，用户在 canary/stable 间往返会按浮动 cap 累计——高 cap canary 段可越过
            // stable 投毒边界，低 cap stable 段又会给正确作答施加伪负位移。cap 钉死 stable 才保证不变式一致。
            let stable_cap = {
                let stable = self.config.read();
                stable.elo.k_factor
                    * stable.elo.word_k_factor_ratio
                    * stable.elo.novice_k_multiplier
                    * 4.0
            };
            crate::store::operations::engine::EloUpdateSpec {
                word_id: raw_event.word_id.clone(),
                is_correct: raw_event.is_correct,
                config: context.config.elo.clone(),
                max_user_word_displacement: stable_cap,
            }
        });
        // W1-1 并发收口：标记已被并发请求抢先写入则整笔回滚，返回 None 让调用方走裸记录回放。
        if !self.persist_state(
            user_id,
            &mut context.user_state,
            &context.algo_states,
            pending_algo,
            elo_spec.as_ref(),
            idempotency_key,
        )? {
            return Ok(None);
        }

        // T1.3:落 mastery 态边沿事件(支撑"长期 masteredCount")。仅幂等学习路径(与 ELO 同口径),
        // 避免诊断端点重试双计;持久化成功后 best-effort 落库,失败仅 warn 不影响主流程。
        // 已知 caveat(low):此写与 persist_state 非同一 tx——commit 后、本 INSERT 前崩溃会永久丢该事件
        // (幂等键已写,重试被拦无法补)。masteredCount 因此在崩溃下略偏低,但丢失对 canary/baseline
        // 两臂对称(同代码路径),不扭曲 A/B 相对比较,仅绝对值低估;作为纯分析指标可接受。
        if let Some(event) = mastery_transition {
            if idempotency_key.is_some() && !raw_event.word_id.is_empty() {
                if let Err(e) = self.store.insert_word_mastery_event(
                    user_id,
                    &raw_event.word_id,
                    event,
                    &now.to_rfc3339(),
                ) {
                    tracing::warn!(error=%e, "insert_word_mastery_event 失败");
                }
            }
        }

        let explanation = self.build_explanation(
            &strategy.constrained_strategy,
            &context.user_state,
            &strategy.weights,
        );
        let result = Self::build_process_result(
            user_id,
            &raw_event,
            strategy.constrained_strategy,
            explanation,
            context.user_state,
            scoring.word_mastery.as_ref(),
            scoring.reward,
            context.cold_start_phase,
        );

        let elapsed = start.elapsed();
        let latency_ms = elapsed.as_millis() as i64;
        // BA3b：Amas 阶段 = 单次 AMAS 决策（process_event_blocking）端到端耗时（含锁内
        // 原子持久化），sub-ms 精度。is_error=false：到此点决策已成功产出。
        crate::stage_metrics::stage_observe(
            crate::stage_metrics::Stage::Amas,
            elapsed.as_secs_f64() * 1000.0,
            false,
        );
        // 用本次请求实际生效的配置算 hash（命中 canary 时即 canary 配置），与 create_canary
        // 落库的 version_hash 同源，保证 canary 切片可被 aggregate_amas_version_slice 聚合、
        // 自动回滚/实测指标管线生效；否则恒打 stable hash 会让 canary 切片永远为空。
        let config_version = monitoring::compute_config_hash(&context.config);
        drop(_guard);
        self.emit_monitoring(
            user_id,
            &result,
            latency_ms,
            &context.config,
            &strategy.final_strategy,
            &config_version,
            &strategy.weights,
            raw_event.is_correct,
            experiment.as_ref().map(|(id, arm)| (id.as_str(), *arm)),
        );

        Ok(Some(result))
    }

    fn prepare_processing_context(
        &self,
        user_id: &str,
        raw_event: &RawEvent,
        config: Arc<AMASConfig>,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<ProcessingContext, AppError> {
        let mut user_state = self.load_or_init_state(user_id)?;
        let algo_states = self.load_algo_states(user_id, &config)?;
        let feature = self.build_feature_vector(raw_event, &user_state, &config, now);
        self.update_modeling(&mut user_state, &feature, &config);
        let cold_start_phase = self.determine_cold_start_phase(&user_state, &config);

        Ok(ProcessingContext {
            config,
            user_state,
            algo_states,
            feature,
            cold_start_phase,
        })
    }

    fn select_strategy(&self, context: &mut ProcessingContext) -> StrategySelection {
        let candidates = self.generate_candidates(
            &context.user_state,
            &context.feature,
            &mut context.algo_states,
            &context.config,
        );
        let (final_strategy, weights) = self.ensemble_or_fallback(
            &candidates,
            &context.user_state,
            &context.algo_states,
            &context.config,
        );
        let constrained_strategy =
            self.apply_constraints(final_strategy.clone(), &context.user_state, &context.config);

        StrategySelection {
            final_strategy,
            constrained_strategy,
            candidates,
            weights,
        }
    }

    fn apply_memory_and_score(
        &self,
        user_id: &str,
        raw_event: &RawEvent,
        context: &ProcessingContext,
        strategy: &StrategySelection,
    ) -> Result<MemoryScoring, AppError> {
        let ssp_arc = self.ssp_policy.read().clone();
        let mut pending_algo: Vec<(String, serde_json::Value)> = Vec::new();
        let word_mastery = self.update_memory(
            user_id,
            raw_event,
            &context.feature,
            &strategy.final_strategy,
            &context.user_state,
            &context.config,
            ssp_arc.as_deref(),
            &mut pending_algo,
        )?;
        let retention_signal = word_mastery
            .as_ref()
            .map(|feedback| feedback.scheduled_recall)
            .unwrap_or(0.0);
        let reward = self.compute_reward(
            &context.feature,
            &context.user_state,
            retention_signal,
            &context.config,
        );
        let objective = self.evaluate_objective(&reward, retention_signal, &context.config);

        Ok(MemoryScoring {
            word_mastery,
            reward,
            objective,
            pending_algo,
        })
    }

    fn update_session_counters(
        user_state: &mut UserState,
        raw_event: &RawEvent,
        now: chrono::DateTime<chrono::Utc>,
    ) {
        user_state.session_event_count += 1;
        user_state.total_event_count += 1;
        user_state.last_active_at = Some(now);

        let current_session_id = raw_event.session_id.as_deref().unwrap_or("");
        if current_session_id.is_empty() {
            return;
        }

        let session_changed = !user_state
            .last_session_id
            .as_deref()
            .is_some_and(|prev| prev == current_session_id);
        if session_changed {
            user_state.session_event_count = 1;
            user_state.last_session_id = Some(current_session_id.to_string());
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_process_result(
        user_id: &str,
        raw_event: &RawEvent,
        strategy: StrategyParams,
        explanation: Explanation,
        user_state: UserState,
        word_mastery: Option<&MemoryFeedback>,
        reward: Reward,
        cold_start_phase: Option<ColdStartPhase>,
    ) -> ProcessResult {
        let session_id = raw_event
            .session_id
            .clone()
            .unwrap_or_else(|| format!("{user_id}-session"));

        ProcessResult {
            session_id,
            strategy,
            explanation,
            state: user_state,
            word_mastery: word_mastery.map(|feedback| feedback.decision.clone()),
            reward,
            cold_start_phase,
        }
    }

    pub async fn update_visual_fatigue(
        &self,
        user_id: &str,
        visual_score: f64,
    ) -> Result<UserState, AppError> {
        let engine = self.clone();
        let user_id = user_id.to_string();
        crate::blocking::run_blocking("amas.update_visual_fatigue", move || {
            engine.update_visual_fatigue_blocking(&user_id, visual_score)
        })
        .await?
    }

    fn update_visual_fatigue_blocking(
        &self,
        user_id: &str,
        visual_score: f64,
    ) -> Result<UserState, AppError> {
        let user_lock = self.acquire_user_lock_blocking(user_id);
        let _guard = user_lock.lock();

        let config = {
            let guard = self.config.read();
            Arc::clone(&guard)
        };
        let mut user_state = self.load_or_init_state(user_id)?;

        let visual_fatigue = (visual_score / 100.0).clamp(0.0, 1.0);

        // 混合公式：behavioral_weight * 行为疲劳 + visual_weight * 视觉疲劳
        let w = config.modeling.visual_fatigue_weight;
        user_state.fatigue = ((1.0 - w) * user_state.fatigue + w * visual_fatigue).clamp(0.0, 1.0);

        // 持久化前清理浮点数值
        user_state.fatigue = sanitize_float(user_state.fatigue, 0.0).clamp(0.0, 1.0);

        // 持久化
        let user_state_json =
            serde_json::to_value(&user_state).map_err(|e| AppError::internal(&e.to_string()))?;
        self.store
            .set_engine_user_state(user_id, &user_state_json)
            .map_err(|e| AppError::internal(&e.to_string()))?;

        Ok(user_state)
    }

    pub fn get_user_state(&self, user_id: &str) -> Result<UserState, AppError> {
        self.load_or_init_state(user_id)
    }

    pub async fn get_user_state_async(&self, user_id: &str) -> Result<UserState, AppError> {
        let engine = self.clone();
        let user_id = user_id.to_string();
        crate::blocking::run_blocking("amas.get_user_state", move || {
            engine.load_or_init_state(&user_id)
        })
        .await?
    }

    pub fn compute_strategy_from_state(&self, user_state: &UserState) -> StrategyParams {
        // 注意：使用 try_read 可能在写锁期间回退默认值。
        // 对于精确结果，调用方应使用 compute_strategy_from_state_with_config 并传入已获取的 config。
        let config = self
            .config
            .try_read()
            .map(|c| Arc::clone(&c))
            .unwrap_or_else(|| Arc::new(AMASConfig::default()));
        self.compute_strategy_from_state_with_config(user_state, &config)
    }

    pub fn compute_strategy_from_state_with_config(
        &self,
        user_state: &UserState,
        config: &AMASConfig,
    ) -> StrategyParams {
        let ls = &config.learning_strategy;
        let mut strategy = StrategyParams::default();

        // Adjust difficulty based on user confidence and motivation
        if user_state.confidence > ls.confidence_boost_threshold {
            strategy.difficulty = (strategy.difficulty + ls.confidence_difficulty_boost).min(1.0);
        }
        if user_state.motivation > ls.motivation_ratio_threshold {
            strategy.new_ratio = (strategy.new_ratio + ls.motivation_ratio_boost).min(1.0);
        }
        if user_state.fatigue > ls.fatigue_reduction_threshold {
            strategy.batch_size =
                (strategy.batch_size as f64 * ls.fatigue_batch_scale).max(3.0) as u32;
            strategy.difficulty = (strategy.difficulty - ls.fatigue_difficulty_drop).max(0.1);
        }

        strategy
    }

    pub async fn get_phase(&self, user_id: &str) -> Result<Option<ColdStartPhase>, AppError> {
        let engine = self.clone();
        let user_id = user_id.to_string();
        crate::blocking::run_blocking("amas.get_phase", move || {
            let state = engine.load_or_init_state(&user_id)?;
            let config = {
                let guard = engine.config.read();
                Arc::clone(&guard)
            };
            Ok(engine.determine_cold_start_phase(&state, &config))
        })
        .await?
    }

    /// #23/#24：完整重置用户引擎态。持 per-user 锁串行化于 process_event（不再与并发上报交错出
    /// 混合态），并在单个原子 tx 内重置 user_state + 删除**全部** algo 状态（含每词 mastery/EVM
    /// 与全局 IAD/MTP）+ 清 user_elo，杜绝原"只删 ige/swd/trust、ELO/per-word 记忆残留"的假重置。
    pub fn reset_user_state(&self, user_id: &str) -> Result<(), AppError> {
        let user_lock = self.acquire_user_lock_blocking(user_id);
        let _guard = user_lock.lock();

        let default_state = serde_json::to_value(UserState::default())
            .map_err(|e| AppError::internal(&e.to_string()))?;
        self.store
            .reset_engine_state_atomic(user_id, &default_state)
            .map_err(|e| AppError::internal(&e.to_string()))
    }

    pub async fn reset_user_state_async(&self, user_id: &str) -> Result<(), AppError> {
        let engine = self.clone();
        let user_id = user_id.to_string();
        crate::blocking::run_blocking("amas.reset_user_state", move || {
            engine.reset_user_state(&user_id)
        })
        .await?
    }

    pub async fn update_temporal_profile(
        &self,
        user_id: &str,
        hour: u8,
        accuracy: f64,
        avg_response_time_ms: f64,
        mastery_efficiency: f64,
    ) -> Result<(), AppError> {
        let engine = self.clone();
        let user_id = user_id.to_string();
        crate::blocking::run_blocking("amas.update_temporal_profile", move || {
            engine.update_temporal_profile_blocking(
                &user_id,
                hour,
                accuracy,
                avg_response_time_ms,
                mastery_efficiency,
            )
        })
        .await?
    }

    fn update_temporal_profile_blocking(
        &self,
        user_id: &str,
        hour: u8,
        accuracy: f64,
        avg_response_time_ms: f64,
        mastery_efficiency: f64,
    ) -> Result<(), AppError> {
        // #48：与 persist_state/update_visual_fatigue 一致的入参卫生——先 sanitize(NaN/Inf) 再
        // clamp，防止任何调用方传入非有限/越界浮点污染 EMA（NaN 写入后永不恢复，并经 get_temporal_boost
        // 透传至 study 下游）。accuracy/mastery_efficiency ∈ [0,1]，响应时长非负。
        let accuracy = sanitize_float(accuracy, 0.0).clamp(0.0, 1.0);
        let mastery_efficiency = sanitize_float(mastery_efficiency, 0.0).clamp(0.0, 1.0);
        let avg_response_time_ms = sanitize_float(avg_response_time_ms, 0.0).max(0.0);

        let user_lock = self.acquire_user_lock_blocking(user_id);
        let _guard = user_lock.lock();

        let config = {
            let guard = self.config.read();
            Arc::clone(&guard)
        };
        let mut user_state = self.load_or_init_state(user_id)?;
        let stats = &mut user_state.habit_profile.temporal_performance;
        // #49：用 saturating get/pad 替代裸索引，防止持久化的 hourly_stats 短于 24 项时越界 panic。
        let idx = (hour as usize).min(23);
        if stats.hourly_stats.len() <= idx {
            stats
                .hourly_stats
                .resize_with(idx + 1, Default::default);
        }
        let h = &mut stats.hourly_stats[idx];

        // EMA 指数平滑
        let alpha = config.feature.temporal_profile_alpha;
        if h.session_count == 0 {
            h.avg_accuracy = accuracy;
            h.avg_response_time_ms = avg_response_time_ms;
            h.mastery_efficiency = mastery_efficiency;
        } else {
            h.avg_accuracy = h.avg_accuracy * (1.0 - alpha) + accuracy * alpha;
            h.avg_response_time_ms =
                h.avg_response_time_ms * (1.0 - alpha) + avg_response_time_ms * alpha;
            h.mastery_efficiency =
                h.mastery_efficiency * (1.0 - alpha) + mastery_efficiency * alpha;
        }
        h.session_count += 1;
        stats.total_sessions += 1;

        // 持久化
        let user_state_json =
            serde_json::to_value(&user_state).map_err(|e| AppError::internal(&e.to_string()))?;
        self.store
            .set_engine_user_state(user_id, &user_state_json)
            .map_err(|e| AppError::internal(&e.to_string()))?;
        Ok(())
    }

    pub fn get_temporal_boost(&self, user_id: &str, hour: u8) -> Result<f64, AppError> {
        let config = self
            .config
            .try_read()
            .map(|c| Arc::clone(&c))
            .unwrap_or_else(|| Arc::new(AMASConfig::default()));
        let state = self.load_or_init_state(user_id)?;
        let stats = &state.habit_profile.temporal_performance;
        let idx = (hour as usize).min(23);
        // #49：持久化的 hourly_stats 若短于 24 项（旧/手改/迁移态），裸索引会 panic 阻断请求；
        // 缺项视作无样本，退回中性 boost 1.0。
        let Some(h) = stats.hourly_stats.get(idx) else {
            return Ok(1.0);
        };

        if h.session_count == 0 {
            return Ok(1.0);
        }

        let f = &config.feature;
        let boost = f.temporal_boost_base + h.mastery_efficiency * f.temporal_boost_scale;
        Ok(boost.clamp(f.temporal_boost_min, f.temporal_boost_max))
    }

    pub async fn get_temporal_boost_async(&self, user_id: &str, hour: u8) -> Result<f64, AppError> {
        let engine = self.clone();
        let user_id = user_id.to_string();
        crate::blocking::run_blocking("amas.get_temporal_boost", move || {
            engine.get_temporal_boost(&user_id, hour)
        })
        .await?
    }

    fn load_or_init_state(&self, user_id: &str) -> Result<UserState, AppError> {
        match self
            .store
            .get_engine_user_state(user_id)
            .map_err(|e| AppError::internal(&e.to_string()))?
        {
            Some(json) => serde_json::from_value(json)
                .map_err(|e| AppError::internal(&format!("State deserialize: {e}"))),
            None => Ok(UserState::default()),
        }
    }

    fn load_algo_states(&self, user_id: &str, config: &AMASConfig) -> Result<AlgoStates, AppError> {
        let mut states = AlgoStates::default();

        if let Some(v) = self
            .store
            .get_engine_algo_state(user_id, "ige")
            .map_err(|e| AppError::internal(&e.to_string()))?
        {
            states.ige = match serde_json::from_value(v) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(user_id, algo = "ige", error = %e, "Algo state deserialization failed, using default");
                    ige::IgeState::new(&config.ige)
                }
            };
        } else {
            states.ige = ige::IgeState::new(&config.ige);
        }

        if let Some(v) = self
            .store
            .get_engine_algo_state(user_id, "swd")
            .map_err(|e| AppError::internal(&e.to_string()))?
        {
            states.swd = match serde_json::from_value(v) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(user_id, algo = "swd", error = %e, "Algo state deserialization failed, using default");
                    swd::SwdState::default()
                }
            };
        }

        if let Some(v) = self
            .store
            .get_engine_algo_state(user_id, "trust")
            .map_err(|e| AppError::internal(&e.to_string()))?
        {
            states.trust_scores = match serde_json::from_value(v) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(user_id, algo = "trust", error = %e, "Algo state deserialization failed, using default");
                    ensemble::TrustScores::default()
                }
            };
        }

        Ok(states)
    }

    fn build_feature_vector(
        &self,
        event: &RawEvent,
        state: &UserState,
        config: &AMASConfig,
        now: chrono::DateTime<chrono::Utc>,
    ) -> FeatureVector {
        let m = &config.modeling;
        let accuracy = if event.is_correct { 1.0 } else { 0.0 };
        let response_speed = (1.0
            - (event.response_time_ms.max(0) as f64 / m.response_speed_max_ms))
            .clamp(0.0, 1.0);
        let f = &config.feature;
        let hint_penalty = if event.hint_used { f.hint_penalty } else { 0.0 };
        let quality = (accuracy * f.quality_accuracy_weight
            + response_speed * f.quality_speed_weight
            - hint_penalty)
            .clamp(0.0, 1.0);
        let quality = if event.is_correct {
            quality
        } else {
            quality * f.incorrect_quality_scale
        };
        let engagement = Self::compute_engagement(event, m);

        let time_since_last = match state.last_active_at {
            Some(last) => (now - last).num_seconds() as f64,
            None => 0.0,
        };

        FeatureVector {
            accuracy,
            response_speed,
            quality,
            engagement,
            hint_penalty,
            time_since_last_event_secs: time_since_last,
            session_event_count: state.session_event_count,
            is_quit: event.is_quit,
        }
    }

    fn compute_engagement(event: &RawEvent, m: &crate::amas::config::ModelingConfig) -> f64 {
        let mut score = 1.0;
        if let Some(pause) = event.pause_count {
            score -= (pause.max(0) as f64 * m.engagement_pause_penalty)
                .min(m.engagement_pause_penalty_max);
        }
        if let Some(switch) = event.switch_count {
            score -= (switch.max(0) as f64 * m.engagement_switch_penalty)
                .min(m.engagement_switch_penalty_max);
        }
        if let Some(focus_loss) = event.focus_loss_duration_ms {
            score -= (focus_loss.max(0) as f64 / m.engagement_focus_loss_base_ms)
                .min(m.engagement_focus_loss_penalty_max);
        }
        score.clamp(0.0, 1.0)
    }

    fn update_modeling(&self, state: &mut UserState, feature: &FeatureVector, config: &AMASConfig) {
        let m = &config.modeling;

        state.attention = state.attention * (1.0 - m.attention_smoothing)
            + feature.engagement * m.attention_smoothing;
        state.attention = state.attention.clamp(0.0, 1.0);

        // 先执行时间衰减，再增加新的疲劳值
        if feature.time_since_last_event_secs >= config.fatigue_decay.full_reset_threshold_secs {
            // >= full_reset_threshold: 完全重置
            state.fatigue = 0.0;
        } else if feature.time_since_last_event_secs
            > config.fatigue_decay.decay_start_threshold_secs
        {
            // decay_start ~ full_reset: 指数衰减（只对超过阈值的部分衰减）
            let elapsed_in_decay = feature.time_since_last_event_secs
                - config.fatigue_decay.decay_start_threshold_secs;
            let decay_factor =
                (-elapsed_in_decay / config.fatigue_decay.decay_time_constant_secs).exp();
            state.fatigue *= decay_factor;
        }

        if feature.is_quit {
            state.fatigue = (state.fatigue + m.fatigue_quit_increase).min(1.0);
        } else {
            state.fatigue = (state.fatigue + m.fatigue_increase_rate).min(1.0);
        }

        let motivation_signal = if feature.accuracy > SIGNAL_THRESHOLD {
            config.feature.motivation_positive_signal
        } else {
            config.feature.motivation_negative_signal
        };
        state.motivation = state.motivation * (1.0 - m.motivation_momentum)
            + motivation_signal * m.motivation_momentum;
        state.motivation = state.motivation.clamp(-1.0, 1.0);

        let confidence_signal = if feature.quality > SIGNAL_THRESHOLD {
            config.feature.confidence_positive_signal
        } else {
            config.feature.confidence_negative_signal
        };
        state.confidence = (state.confidence * m.confidence_decay + confidence_signal)
            .clamp(m.min_confidence, 1.0);

        // 更新认知画像
        let alpha = m.cognitive_profile_alpha;
        state.cognitive_profile.processing_speed = state.cognitive_profile.processing_speed
            * (1.0 - alpha)
            + feature.response_speed * alpha;
        state.cognitive_profile.memory_capacity =
            state.cognitive_profile.memory_capacity * (1.0 - alpha) + feature.accuracy * alpha;
        state.cognitive_profile.stability =
            state.cognitive_profile.stability * (1.0 - alpha) + feature.quality * alpha;

        // 更新趋势状态
        let trend_alpha = m.trend_alpha;
        state.trend_state.accuracy_trend = state.trend_state.accuracy_trend * (1.0 - trend_alpha)
            + (feature.accuracy - TREND_BASELINE) * trend_alpha;
        state.trend_state.speed_trend = state.trend_state.speed_trend * (1.0 - trend_alpha)
            + (feature.response_speed - TREND_BASELINE) * trend_alpha;
        state.trend_state.engagement_trend = state.trend_state.engagement_trend
            * (1.0 - trend_alpha)
            + (feature.engagement - TREND_BASELINE) * trend_alpha;
    }

    fn determine_cold_start_phase(
        &self,
        state: &UserState,
        config: &AMASConfig,
    ) -> Option<ColdStartPhase> {
        let cs = &config.cold_start;
        if state.total_event_count < cs.classify_to_explore_events {
            Some(ColdStartPhase::Classify)
        } else if state.total_event_count < cs.explore_to_exploit_events {
            // B28: Enhanced with AUC-based learner type classification
            Some(ColdStartPhase::Explore)
        } else {
            None
        }
    }

    /// B28: Classify learner type based on performance profile
    pub fn classify_learner_type(&self, user_id: &str) -> Result<LearnerType, AppError> {
        let config = self
            .config
            .try_read()
            .map(|c| Arc::clone(&c))
            .unwrap_or_else(|| Arc::new(AMASConfig::default()));
        let state = self.load_or_init_state(user_id)?;
        let cp = &state.cognitive_profile;
        let cl = &config.classifier;

        let auc = cp.processing_speed * cl.processing_speed_weight
            + cp.memory_capacity * cl.memory_capacity_weight
            + cp.stability * cl.stability_weight;
        if auc > cl.fast_learner_threshold {
            Ok(LearnerType::Fast)
        } else if auc > cl.stable_learner_threshold {
            Ok(LearnerType::Stable)
        } else {
            Ok(LearnerType::Cautious)
        }
    }

    pub async fn classify_learner_type_async(
        &self,
        user_id: &str,
    ) -> Result<LearnerType, AppError> {
        let engine = self.clone();
        let user_id = user_id.to_string();
        crate::blocking::run_blocking("amas.classify_learner_type", move || {
            engine.classify_learner_type(&user_id)
        })
        .await?
    }

    fn generate_candidates(
        &self,
        user_state: &UserState,
        feature: &FeatureVector,
        algo_states: &mut AlgoStates,
        config: &AMASConfig,
    ) -> Vec<DecisionCandidate> {
        let mut candidates = Vec::new();

        if config.feature_flags.heuristic_enabled {
            let start = std::time::Instant::now();
            candidates.push(heuristic::generate(user_state, feature, config));
            self.metrics_registry.record_call(
                AlgorithmId::Heuristic,
                start.elapsed().as_micros() as u64,
                false,
            );
        }

        if config.feature_flags.ige_enabled {
            let start = std::time::Instant::now();
            candidates.push(ige::generate(user_state, feature, &algo_states.ige, config));
            self.metrics_registry.record_call(
                AlgorithmId::Ige,
                start.elapsed().as_micros() as u64,
                false,
            );
        }

        if config.feature_flags.swd_enabled {
            let start = std::time::Instant::now();
            candidates.push(swd::generate(user_state, &algo_states.swd, config));
            self.metrics_registry.record_call(
                AlgorithmId::Swd,
                start.elapsed().as_micros() as u64,
                false,
            );
        }

        candidates
    }

    fn ensemble_or_fallback(
        &self,
        candidates: &[DecisionCandidate],
        user_state: &UserState,
        algo_states: &AlgoStates,
        config: &AMASConfig,
    ) -> (StrategyParams, HashMap<AlgorithmId, f64>) {
        if candidates.is_empty() {
            return (StrategyParams::default(), HashMap::new());
        }

        if config.feature_flags.ensemble_enabled && candidates.len() > 1 {
            let weights = ensemble::get_weights_for_candidates(
                candidates,
                user_state.total_event_count,
                &algo_states.trust_scores,
                &config.ensemble,
            );
            let strategy = ensemble::merge(candidates, &weights);
            return (strategy, weights);
        }

        let chosen = candidates
            .iter()
            .filter(|c| c.confidence.is_finite())
            .max_by(|a, b| {
                a.confidence
                    .partial_cmp(&b.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or_else(|| candidates.first().unwrap());
        let mut weights = HashMap::new();
        weights.insert(chosen.algorithm_id, 1.0);
        (chosen.strategy.clone(), weights)
    }

    fn compute_reward(
        &self,
        feature: &FeatureVector,
        state: &UserState,
        recall_prob: f64,
        config: &AMASConfig,
    ) -> Reward {
        let r = &config.reward;
        let accuracy_reward = feature.accuracy;
        let speed_reward = feature.response_speed * r.speed_reward_scale;
        let fatigue_penalty = if state.fatigue > r.fatigue_penalty_threshold {
            state.fatigue * r.fatigue_penalty_scale
        } else {
            0.0
        };
        let frustration_penalty = if state.motivation < r.frustration_penalty_threshold {
            (-state.motivation) * r.frustration_penalty_scale
        } else {
            0.0
        };
        let expected_forget_cost = (1.0 - recall_prob) * r.expected_forget_cost_weight;

        let value = accuracy_reward + speed_reward
            - fatigue_penalty
            - frustration_penalty
            - expected_forget_cost;

        Reward {
            value: value.clamp(-1.0, 1.0),
            components: RewardComponents {
                accuracy_reward,
                speed_reward,
                fatigue_penalty,
                frustration_penalty,
                expected_forget_cost,
            },
        }
    }

    fn evaluate_objective(
        &self,
        reward: &Reward,
        retention_signal: f64,
        config: &AMASConfig,
    ) -> ObjectiveEvaluation {
        let w = &config.objective_weights;
        let score = reward.components.accuracy_reward * w.accuracy
            + reward.components.speed_reward * w.speed
            + retention_signal * w.retention
            - reward.components.fatigue_penalty * w.fatigue
            - reward.components.frustration_penalty * w.frustration;

        ObjectiveEvaluation {
            score,
            retention_gain: retention_signal,
            accuracy_gain: reward.components.accuracy_reward,
            speed_gain: reward.components.speed_reward,
            fatigue_penalty: reward.components.fatigue_penalty,
            frustration_penalty: reward.components.frustration_penalty,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn update_memory(
        &self,
        user_id: &str,
        raw_event: &RawEvent,
        feature: &FeatureVector,
        strategy: &StrategyParams,
        user_state: &UserState,
        config: &AMASConfig,
        ssp_policy: Option<&ssp::SspPolicy>,
        pending_algo: &mut Vec<(String, serde_json::Value)>,
    ) -> Result<Option<MemoryFeedback>, AppError> {
        if raw_event.word_id.is_empty() {
            return Ok(None);
        }

        // ③ 多痕迹（Phase 2）：按 question_mode 派生键（flag 关/未知 mode → 单痕迹 legacy）。
        // 与 records 路由快照/回滚路径共用 mastery_state_key，保证键一致。
        let key = mastery::mastery_state_key(
            &raw_event.word_id,
            raw_event.question_mode.as_deref(),
            config.feature_flags.multi_trace_enabled,
        );
        let mut state = match self
            .store
            .get_engine_algo_state(user_id, &key)
            .map_err(|e| AppError::internal(&e.to_string()))?
        {
            Some(value) => match serde_json::from_value(value) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(word_id = %raw_event.word_id, error = %e, "Mastery state deserialization failed, creating new");
                    mastery::WordMasteryState::new(&raw_event.word_id)
                }
            },
            None => mastery::WordMasteryState::new(&raw_event.word_id),
        };

        // 调度间隔缩放基线（下方 EVM 调制）。
        // IAD(B38 混淆干扰) / MTP(B37 词素迁移) 算法已移除（2026-06-13）：经法证确认二者 flag 默认关、
        // 对预测维度残差零贡献、生产从未启用 → 当前算法零贡献。confused_with API 字段保留为「接受但忽略」
        // （iOS 仍上报，契约不破）；word_morphemes 表 + /api/content/morphemes 端点保留（词根展示在用）。
        let mut adjusted_interval_scale = strategy.interval_scale;

        // B39: EVM - Encoding Variability Model
        {
            let evm_key = format!("evm:{}", raw_event.word_id);
            let mut evm_state: evm::EvmState = self
                .store
                .get_engine_algo_state(user_id, &evm_key)
                .map_err(|e| AppError::internal(&e.to_string()))?
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();

            // #46：传入 session_id，由 record_context 按"是否首见该 session"判定 distinct 上下文，
            // 而非仅凭"有无 session"每次都 +1。
            evm::record_context(&mut evm_state, raw_event.session_id.as_deref(), &config.evm);
            adjusted_interval_scale *= evm::interval_modifier(&evm_state, &config.evm);

            if let Ok(val) = serde_json::to_value(&evm_state) {
                pending_algo.push((evm_key.clone(), val));
            }
        }

        // B40: 自适应目标保持率
        let desired_retention = mdm::adaptive_desired_retention(
            config.memory_model.base_desired_retention,
            feature.accuracy,
            user_state.fatigue,
            user_state.motivation,
            &config.memory_model,
        );
        let now_ms = chrono::Utc::now().timestamp_millis();

        // 冷启动难度先验（Phase 1a）：仅首评（review_count==0）构建并消费，避免每次复习多查 DB。
        // 默认权重全 0 → ColdStartPriors::deltas=(0,0) → bit-exact legacy。
        let cold_start = if state.mdm.review_count == 0 {
            self.build_cold_start_priors(&raw_event.word_id, &config.memory_model)
        } else {
            None
        };

        // T1.3:记录更新前的 mastery 态，update 后判定是否发生进入 MASTERED / FORGOTTEN 的边沿。
        let prev_level = state.mastery_level.clone();
        let decision = mastery::update_mastery_at(
            &mut state,
            raw_event.is_correct,
            feature.quality,
            adjusted_interval_scale,
            desired_retention,
            now_ms,
            &config.memory_model,
            ssp_policy,
            cold_start.as_ref(),
        );
        let mastery_transition = match decision.mastery_level {
            MasteryLevel::Mastered if prev_level != MasteryLevel::Mastered => Some("mastered"),
            MasteryLevel::Forgotten if prev_level != MasteryLevel::Forgotten => Some("forgotten"),
            _ => None,
        };
        let scheduled_at =
            now_ms.saturating_add(decision.next_review_interval_secs.saturating_mul(1000));
        let scheduled_recall =
            mdm::recall_probability(&state.mdm, scheduled_at, &config.memory_model);

        pending_algo.push((
            key.clone(),
            serde_json::to_value(&state).map_err(|e| AppError::internal(&e.to_string()))?,
        ));

        Ok(Some(MemoryFeedback {
            decision,
            scheduled_recall,
            desired_retention,
            mastery_transition,
        }))
    }

    /// 冷启动难度先验（Phase 1a）：从 word.text/word.difficulty/word_morphemes 预计算特征。
    /// word 不存在 → None（更新回落 bit-exact legacy）。权重全 0 时即便返回 Some 也 no-op。
    fn build_cold_start_priors(
        &self,
        word_id: &str,
        mm: &crate::amas::config::MemoryModelConfig,
    ) -> Option<mdm::ColdStartPriors> {
        let word = self.store.get_word(word_id).ok().flatten()?;
        let len = word.text.chars().count() as f64;
        let len_z =
            ((len - mm.cold_start_len_ref) / mm.cold_start_len_scale.max(1e-9)).clamp(-1.0, 1.0);
        // word.difficulty ∈ (0,1] → [1,10]；0/缺值 → None（与 word_selector::word_difficulty_logit_scale 同义）。
        let ext_difficulty = if word.difficulty > 0.0 {
            Some((1.0 + 9.0 * word.difficulty).clamp(1.0, 10.0))
        } else {
            None
        };
        Some(mdm::ColdStartPriors {
            len_z,
            morph_transparency: self.morph_transparency(word_id),
            ext_difficulty,
        })
    }

    /// 词素透明度 ∈ [0,1] = 已知词根/词缀（type∈{prefix,root,suffix} 且 meaning 非空）占比。
    /// 无词素 / 不可分解（<2 个词素）→ None（无信号，该项 no-op）。
    fn morph_transparency(&self, word_id: &str) -> Option<f64> {
        let arr = self
            .store
            .get_word_morphemes(word_id)
            .ok()
            .flatten()?
            .as_array()?
            .clone();
        if arr.len() < 2 {
            return None;
        }
        let total = arr.len() as f64;
        let known = arr
            .iter()
            .filter(|m| {
                let ty = m.get("type").and_then(|v| v.as_str()).unwrap_or("");
                let meaning = m.get("meaning").and_then(|v| v.as_str()).unwrap_or("");
                matches!(ty, "prefix" | "root" | "suffix") && !meaning.is_empty()
            })
            .count() as f64;
        Some((known / total).clamp(0.0, 1.0))
    }

    fn apply_constraints(
        &self,
        mut strategy: StrategyParams,
        state: &UserState,
        config: &AMASConfig,
    ) -> StrategyParams {
        let c = &config.constraints;

        if state.fatigue > c.high_fatigue_threshold {
            strategy.batch_size = strategy.batch_size.min(c.max_batch_size_when_fatigued);
            strategy.new_ratio = strategy.new_ratio.min(c.max_new_ratio_when_fatigued);
            strategy.difficulty = strategy.difficulty.min(c.max_difficulty_when_fatigued);
        }

        if state.attention < c.low_attention_threshold {
            strategy.review_mode = true;
            strategy.new_ratio = 0.0;
        }

        if state.motivation < c.low_motivation_threshold {
            strategy.difficulty =
                (strategy.difficulty - c.low_motivation_difficulty_drop).max(c.min_difficulty);
            strategy.new_ratio = (strategy.new_ratio - c.low_motivation_ratio_drop).max(0.0);
        }

        strategy.difficulty = strategy.difficulty.clamp(0.0, 1.0);
        strategy.new_ratio = strategy.new_ratio.clamp(0.0, 1.0);
        strategy.batch_size = strategy.batch_size.max(1);
        strategy.interval_scale = strategy.interval_scale.max(0.1);

        strategy
    }

    #[allow(clippy::too_many_arguments)]
    fn update_trust_scores(
        &self,
        algo_states: &mut AlgoStates,
        candidates: &[DecisionCandidate],
        reward: f64,
        objective_score: f64,
        user_state: &UserState,
        weights: &HashMap<AlgorithmId, f64>,
        config: &AMASConfig,
    ) {
        let blended = reward * 0.5 + objective_score * 0.5;
        let max_weight = weights.values().copied().fold(0.0_f64, f64::max).max(1e-9);

        for candidate in candidates {
            let weight = weights.get(&candidate.algorithm_id).copied().unwrap_or(0.0);
            if weight <= 0.0 {
                continue;
            }

            let lr = config.feature.trust_base_learning_rate * (weight / max_weight).max(0.1);
            ensemble::update_trust(
                &mut algo_states.trust_scores,
                candidate.algorithm_id,
                blended,
                lr,
            );

            if candidate.algorithm_id == AlgorithmId::Ige {
                ige::update(&mut algo_states.ige, &candidate.strategy, blended);
            }

            if candidate.algorithm_id == AlgorithmId::Swd {
                swd::update(
                    &mut algo_states.swd,
                    user_state,
                    &candidate.strategy,
                    blended,
                    config,
                );
            }
        }
    }

    /// 返回 `true`=本次状态已提交；`false`=幂等标记已被并发请求抢先写入、整笔回滚（详见
    /// [`Store::persist_engine_state_atomic`]）。`idempotency_key` 为 `None` 时恒 `true`。
    #[allow(clippy::too_many_arguments)]
    fn persist_state(
        &self,
        user_id: &str,
        user_state: &mut UserState,
        algo_states: &AlgoStates,
        pending_algo: Vec<(String, serde_json::Value)>,
        elo: Option<&crate::store::operations::engine::EloUpdateSpec>,
        idempotency_key: Option<&str>,
    ) -> Result<bool, AppError> {
        // 在保存前清理浮点字段，防止 NaN 传播
        user_state.attention = sanitize_float(user_state.attention, 0.5).clamp(0.0, 1.0);
        user_state.fatigue = sanitize_float(user_state.fatigue, 0.0).clamp(0.0, 1.0);
        user_state.motivation = sanitize_float(user_state.motivation, 0.0).clamp(-1.0, 1.0);
        user_state.confidence = sanitize_float(user_state.confidence, 0.5).clamp(0.0, 1.0);
        user_state.cognitive_profile.memory_capacity =
            sanitize_float(user_state.cognitive_profile.memory_capacity, 0.5).clamp(0.0, 1.0);
        user_state.cognitive_profile.processing_speed =
            sanitize_float(user_state.cognitive_profile.processing_speed, 0.5).clamp(0.0, 1.0);
        user_state.cognitive_profile.stability =
            sanitize_float(user_state.cognitive_profile.stability, 0.5).clamp(0.0, 1.0);

        let user_state_json =
            serde_json::to_value(&*user_state).map_err(|e| AppError::internal(&e.to_string()))?;

        // W1-1：ige/swd/trust 与 update_memory 累积的 per-word 状态（mastery/IAD/MTP/EVM）合并，
        // 连同幂等标记由 persist_engine_state_atomic 同一 tx 原子提交——保证"标记存在 ⟺ 全部 AMAS
        // 状态已应用"，崩溃重试不会对记忆模型二次累加。
        let mut algo_entries: Vec<(String, serde_json::Value)> = vec![
            (
                "ige".to_string(),
                serde_json::to_value(&algo_states.ige)
                    .map_err(|e| AppError::internal(&e.to_string()))?,
            ),
            (
                "swd".to_string(),
                serde_json::to_value(&algo_states.swd)
                    .map_err(|e| AppError::internal(&e.to_string()))?,
            ),
            (
                "trust".to_string(),
                serde_json::to_value(&algo_states.trust_scores)
                    .map_err(|e| AppError::internal(&e.to_string()))?,
            ),
        ];
        algo_entries.extend(pending_algo);

        self.store
            .persist_engine_state_atomic(
                user_id,
                &user_state_json,
                &algo_entries,
                elo,
                idempotency_key,
            )
            .map_err(|e| AppError::internal(&e.to_string()))
    }

    fn build_explanation(
        &self,
        strategy: &StrategyParams,
        user_state: &UserState,
        weights: &HashMap<AlgorithmId, f64>,
    ) -> Explanation {
        let mut factors = Vec::new();
        factors.push(ExplanationFactor {
            name: "difficulty".to_string(),
            value: strategy.difficulty,
            impact: if strategy.difficulty > 0.5 {
                "positive".to_string()
            } else {
                "neutral".to_string()
            },
        });
        factors.push(ExplanationFactor {
            name: "fatigue".to_string(),
            value: user_state.fatigue,
            impact: if user_state.fatigue > 0.7 {
                "negative".to_string()
            } else {
                "neutral".to_string()
            },
        });

        for (algo, weight) in weights {
            factors.push(ExplanationFactor {
                name: format!("weight_{}", algo.as_str()),
                value: *weight,
                impact: "neutral".to_string(),
            });
        }

        let primary_reason = Self::derive_primary_reason(strategy, user_state, weights);

        Explanation {
            primary_reason,
            factors,
        }
    }

    fn derive_primary_reason(
        strategy: &StrategyParams,
        user_state: &UserState,
        weights: &HashMap<AlgorithmId, f64>,
    ) -> String {
        if user_state.fatigue > 0.7 {
            return format!(
                "检测到疲劳度较高({:.0}%)，已降低难度并减少新词比例",
                user_state.fatigue * 100.0,
            );
        }

        if strategy.review_mode {
            return "当前为复习模式，优先巩固已学词汇".to_string();
        }

        if user_state.confidence < 0.3 {
            return format!(
                "学习信心偏低({:.0}%)，已适当降低难度以帮助建立信心",
                user_state.confidence * 100.0,
            );
        }

        if user_state.attention < 0.4 {
            return format!(
                "注意力下降({:.0}%)，已减小学习批次以保持效率",
                user_state.attention * 100.0,
            );
        }

        if strategy.difficulty > 0.7 && user_state.motivation > 0.6 {
            return "学习状态良好，适当提升难度以加速进步".to_string();
        }

        let dominant = weights
            .iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal));
        if let Some((algo, w)) = dominant {
            if *w > 0.5 {
                let algo_desc = match algo {
                    AlgorithmId::Heuristic => "经验规则",
                    AlgorithmId::Ige => "智能梯度",
                    AlgorithmId::Swd => "间隔加权",
                    _ => "综合评估",
                };
                return format!("基于{}策略(权重{:.0}%)生成学习方案", algo_desc, w * 100.0);
            }
        }

        "综合多维度指标生成个性化学习策略".to_string()
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    fn emit_monitoring(
        &self,
        user_id: &str,
        result: &ProcessResult,
        latency_ms: i64,
        config: &AMASConfig,
        pre_constraint_strategy: &StrategyParams,
        config_version: &str,
        routing_weights: &HashMap<AlgorithmId, f64>,
        is_correct: bool,
        experiment: Option<(&str, &str)>,
    ) {
        monitoring::record_event(
            &self.store,
            user_id,
            &result.session_id,
            result,
            latency_ms,
            config,
            pre_constraint_strategy,
            config_version,
            routing_weights,
            is_correct,
            experiment,
        );
    }
}

#[cfg(test)]
// 测试构造 AMASConfig / 各 sub-config 大量用 `let mut x = T::default(); x.field = v` 风格，
// 比 struct-update 语法（`T { field: v, ..Default::default() }`）更易看清「我故意改了什么」。
// clippy 视为习语警告，本 mod 整体豁免。
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    fn test_engine(config: AMASConfig) -> AMASEngine {
        let store = Arc::new(Store::open(":memory:", 5000, 1).unwrap());
        store.run_migrations().unwrap();
        AMASEngine::new(config, store)
    }

    fn sample_feature() -> FeatureVector {
        FeatureVector {
            accuracy: 1.0,
            response_speed: 0.9,
            quality: 0.9,
            engagement: 0.8,
            hint_penalty: 0.0,
            time_since_last_event_secs: 60.0,
            session_event_count: 1,
            is_quit: false,
        }
    }

    fn sample_event(word_id: &str) -> RawEvent {
        RawEvent {
            word_id: word_id.to_string(),
            is_correct: true,
            response_time_ms: 800,
            session_id: Some("session-1".to_string()),
            ..RawEvent::default()
        }
    }

    #[test]
    fn scheduled_recall_changes_reward_and_objective_with_interval_scale() {
        let config = AMASConfig::default();
        let engine = test_engine(config.clone());
        let feature = sample_feature();
        let user_state = UserState::default();
        let short_strategy = StrategyParams {
            interval_scale: 0.5,
            ..StrategyParams::default()
        };
        let long_strategy = StrategyParams {
            interval_scale: 2.0,
            ..StrategyParams::default()
        };

        let short_feedback = engine
            .update_memory(
                "user-short",
                &sample_event("word-short"),
                &feature,
                &short_strategy,
                &user_state,
                &config,
                None,
                &mut Vec::new(),
            )
            .unwrap()
            .expect("short feedback");
        let long_feedback = engine
            .update_memory(
                "user-long",
                &sample_event("word-long"),
                &feature,
                &long_strategy,
                &user_state,
                &config,
                None,
                &mut Vec::new(),
            )
            .unwrap()
            .expect("long feedback");

        assert!(short_feedback.decision.recall_probability > 0.99);
        assert!(long_feedback.decision.recall_probability > 0.99);
        assert!(short_feedback.scheduled_recall > long_feedback.scheduled_recall);

        let short_reward = engine.compute_reward(
            &feature,
            &user_state,
            short_feedback.scheduled_recall,
            &config,
        );
        let long_reward = engine.compute_reward(
            &feature,
            &user_state,
            long_feedback.scheduled_recall,
            &config,
        );
        let short_objective =
            engine.evaluate_objective(&short_reward, short_feedback.scheduled_recall, &config);
        let long_objective =
            engine.evaluate_objective(&long_reward, long_feedback.scheduled_recall, &config);

        assert!(
            short_reward.components.expected_forget_cost
                < long_reward.components.expected_forget_cost
        );
        assert!(short_objective.score > long_objective.score);
    }

    #[test]
    fn update_trust_scores_ignores_zero_weight_candidates() {
        let engine = test_engine(AMASConfig::default());
        let mut algo_states = AlgoStates::default();
        let original_ige_trust = algo_states.trust_scores.ige;
        let original_swd_trust = algo_states.trust_scores.swd;
        let candidates = vec![
            DecisionCandidate {
                algorithm_id: AlgorithmId::Ige,
                strategy: StrategyParams {
                    difficulty: 0.8,
                    new_ratio: 0.7,
                    ..StrategyParams::default()
                },
                confidence: 1.0,
                explanation: "ige".to_string(),
            },
            DecisionCandidate {
                algorithm_id: AlgorithmId::Swd,
                strategy: StrategyParams {
                    difficulty: 0.3,
                    new_ratio: 0.2,
                    review_mode: true,
                    ..StrategyParams::default()
                },
                confidence: 1.0,
                explanation: "swd".to_string(),
            },
        ];
        let mut weights = HashMap::new();
        weights.insert(AlgorithmId::Ige, 0.0);
        weights.insert(AlgorithmId::Swd, 1.0);

        engine.update_trust_scores(
            &mut algo_states,
            &candidates,
            0.6,
            0.6,
            &UserState::default(),
            &weights,
            &AMASConfig::default(),
        );

        assert_eq!(algo_states.trust_scores.ige, original_ige_trust);
        assert_ne!(algo_states.trust_scores.swd, original_swd_trust);
        assert_eq!(algo_states.ige.total_explorations, 0);
        assert_eq!(algo_states.swd.strategy_history.len(), 1);
    }

    // ---- 基础属性 ----

    #[test]
    fn get_config_clones_internal_config() {
        let cfg = AMASConfig::default();
        let engine = test_engine(cfg.clone());
        let got = engine.get_config();
        assert_eq!(
            got.feature_flags.ensemble_enabled,
            cfg.feature_flags.ensemble_enabled
        );
    }

    #[test]
    fn metrics_registry_is_initialized_with_zero_counts() {
        let engine = test_engine(AMASConfig::default());
        let snap = engine.metrics_registry().snapshot();
        for v in snap.values() {
            assert_eq!(v.call_count, 0);
        }
    }

    #[test]
    fn ssp_policy_present_when_ssp_enabled() {
        let mut cfg = AMASConfig::default();
        cfg.feature_flags.ssp_enabled = true;
        cfg.ssp.max_iterations = 5; // 加速测试
        let engine = test_engine(cfg);
        assert!(engine.ssp_policy().is_some());
    }

    #[test]
    fn ssp_policy_absent_when_ssp_disabled() {
        let mut cfg = AMASConfig::default();
        cfg.feature_flags.ssp_enabled = false;
        let engine = test_engine(cfg);
        assert!(engine.ssp_policy().is_none());
    }

    #[test]
    fn ssp_policy_dual_grid_path_constructs_policy() {
        let mut cfg = AMASConfig::default();
        cfg.feature_flags.ssp_enabled = true;
        cfg.ssp.max_iterations = 5;
        cfg.ssp.dual_grid_enabled = true;
        let engine = test_engine(cfg);
        assert!(engine.ssp_policy().is_some());
    }

    #[test]
    fn is_healthy_true_for_default_config() {
        let engine = test_engine(AMASConfig::default());
        assert!(engine.is_healthy());
    }

    #[test]
    fn is_healthy_false_when_ssp_enabled_but_policy_missing() {
        // 构造时 ssp_enabled=false，跳过 precompute；再 reload 把 flag 翻为 true
        // 但 reload 会重建 policy，故为了模拟 missing 我们用直接构造方式：
        // 用 default 配置构造，然后手工把内部 config 切到 ssp_enabled=true 而不重建 policy。
        let engine = test_engine(AMASConfig::default());
        let mut new_cfg = AMASConfig::default();
        new_cfg.feature_flags.ssp_enabled = true;
        // 直接写入 config 而不调用 reload_config —— 模拟 policy 缺失
        *engine.config.write() = Arc::new(new_cfg);
        // 不重建 ssp_policy
        assert!(!engine.is_healthy());
    }

    // ---- reload_config ----

    #[test]
    fn reload_config_with_invalid_config_returns_err() {
        let engine = test_engine(AMASConfig::default());
        let mut bad = AMASConfig::default();
        bad.modeling.attention_smoothing = 5.0; // 越界
        assert!(engine.reload_config(bad).is_err());
    }

    #[test]
    fn reload_config_with_valid_config_swaps_state() {
        let engine = test_engine(AMASConfig::default());
        let mut new_cfg = AMASConfig::default();
        new_cfg.feature_flags.ssp_enabled = true;
        new_cfg.ssp.max_iterations = 3;
        engine.reload_config(new_cfg).expect("reload ok");
        assert!(engine.ssp_policy().is_some());
    }

    #[test]
    fn reload_config_with_ssp_disabled_clears_policy() {
        let mut cfg = AMASConfig::default();
        cfg.feature_flags.ssp_enabled = true;
        cfg.ssp.max_iterations = 3;
        let engine = test_engine(cfg);
        assert!(engine.ssp_policy().is_some());

        let mut new_cfg = AMASConfig::default();
        new_cfg.feature_flags.ssp_enabled = false;
        engine.reload_config(new_cfg).expect("reload ok");
        assert!(engine.ssp_policy().is_none());
    }

    // ---- compute_strategy_from_state ----

    #[test]
    fn compute_strategy_boosts_difficulty_when_confidence_high() {
        let engine = test_engine(AMASConfig::default());
        let mut state = UserState::default();
        state.confidence = 0.95;
        let s = engine.compute_strategy_from_state(&state);
        assert!(s.difficulty >= StrategyParams::default().difficulty);
    }

    #[test]
    fn compute_strategy_increases_new_ratio_when_motivation_high() {
        let engine = test_engine(AMASConfig::default());
        let mut state = UserState::default();
        state.motivation = 0.9;
        let s = engine.compute_strategy_from_state(&state);
        assert!(s.new_ratio >= StrategyParams::default().new_ratio);
    }

    #[test]
    fn compute_strategy_reduces_difficulty_and_batch_when_fatigued() {
        let engine = test_engine(AMASConfig::default());
        let mut state = UserState::default();
        state.fatigue = 0.95;
        let s = engine.compute_strategy_from_state(&state);
        let baseline = StrategyParams::default();
        assert!(s.batch_size <= baseline.batch_size);
        assert!(s.difficulty <= baseline.difficulty);
    }

    // ---- reset_user_state ----

    #[test]
    fn reset_user_state_persists_default_state() {
        let engine = test_engine(AMASConfig::default());
        // 先随便存一个非默认 state
        let mut state = UserState::default();
        state.fatigue = 0.7;
        let state_json = serde_json::to_value(&state).unwrap();
        engine
            .store
            .set_engine_user_state("u-reset", &state_json)
            .unwrap();

        engine.reset_user_state("u-reset").expect("reset");
        let loaded = engine.get_user_state("u-reset").unwrap();
        assert!((loaded.fatigue - 0.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn reset_user_state_async_works() {
        let engine = test_engine(AMASConfig::default());
        engine
            .reset_user_state_async("u-async-reset")
            .await
            .expect("reset async");
        let state = engine
            .get_user_state_async("u-async-reset")
            .await
            .expect("get state");
        assert!((state.fatigue - 0.0).abs() < 1e-9);
    }

    // ---- process_event ----

    #[tokio::test]
    async fn process_event_full_pipeline_succeeds() {
        let engine = test_engine(AMASConfig::default());
        let result = engine
            .process_event(
                "u-process",
                RawEvent {
                    word_id: "w-1".to_string(),
                    is_correct: true,
                    response_time_ms: 500,
                    session_id: Some("session-A".to_string()),
                    ..RawEvent::default()
                },
            )
            .await
            .expect("process_event");
        assert_eq!(result.session_id, "session-A");
        assert!(result.state.total_event_count >= 1);
    }

    #[tokio::test]
    async fn experiment_running_enrolls_and_tags_monitoring() {
        // T1.3 A/A 基础设施自检：running 实验下，process_event 按桶冻结入组，且监控事件强制全采
        // 并带 experimentArm（即便配置采样率默认 5%）。
        let engine = test_engine(AMASConfig::default());
        let exp = crate::store::operations::amas_experiment::NewExperiment {
            experiment_id: "aa".into(),
            suggestion_id: None,
            canary_version_hash: "v".into(),
            baseline_version_hash: "v".into(), // A/A：两臂同配置
            canary_cohort_lo: 0,
            canary_cohort_hi: 50,
            primary_metric: "day7_retention".into(),
            min_sample: 10,
            alpha: 0.05,
            power: 0.8,
            mde: 0.05,
            offline_delta: None,
            notes: None,
        };
        engine
            .store
            .register_experiment(&exp, "2026-05-01T00:00:00+00:00")
            .unwrap();
        engine.reload_active_experiment();

        for i in 0..30 {
            let uid = format!("u{i}");
            engine
                .process_event(
                    &uid,
                    RawEvent {
                        word_id: "w".to_string(),
                        is_correct: true,
                        response_time_ms: 500,
                        session_id: Some("s".to_string()),
                        ..RawEvent::default()
                    },
                )
                .await
                .expect("process_event");
        }

        // 入组：每个 distinct 用户冻结一条，两臂合计=30；按 cohort [0,50) 两臂均应有人。
        let m = engine.store.experiment_raw_metrics("aa").unwrap();
        assert_eq!(m.canary.enrolled + m.baseline.enrolled, 30);
        assert!(m.canary.enrolled > 0 && m.baseline.enrolled > 0, "两臂都应有用户");

        // 强制全采：监控事件应带 experimentArm（默认 5% 采样会丢绝大多数，全采则全部落）。
        let evts = engine.store.get_recent_monitoring_events(100).unwrap();
        let tagged = evts
            .iter()
            .filter(|e| e.get("experimentArm").and_then(|v| v.as_str()).is_some())
            .count();
        assert!(tagged >= 25, "实验桶事件应强制全采并带 arm，实际 tagged={tagged}");
    }

    #[tokio::test]
    async fn process_event_without_session_id_falls_back_to_user_session() {
        let engine = test_engine(AMASConfig::default());
        let result = engine
            .process_event(
                "u-nosession",
                RawEvent {
                    word_id: "w-1".to_string(),
                    is_correct: false,
                    response_time_ms: 2000,
                    ..RawEvent::default()
                },
            )
            .await
            .expect("process_event");
        assert!(result.session_id.contains("u-nosession"));
    }

    #[tokio::test]
    async fn process_event_session_switch_resets_session_count() {
        let engine = test_engine(AMASConfig::default());
        // 两次相同 session
        for i in 0..2 {
            engine
                .process_event(
                    "u-switch",
                    RawEvent {
                        word_id: format!("w-{i}"),
                        is_correct: true,
                        response_time_ms: 500,
                        session_id: Some("s-1".to_string()),
                        ..RawEvent::default()
                    },
                )
                .await
                .unwrap();
        }
        // 切换到新 session
        let r = engine
            .process_event(
                "u-switch",
                RawEvent {
                    word_id: "w-x".to_string(),
                    is_correct: true,
                    response_time_ms: 500,
                    session_id: Some("s-2".to_string()),
                    ..RawEvent::default()
                },
            )
            .await
            .unwrap();
        // 新 session 计数应被重置到 1
        assert_eq!(r.state.session_event_count, 1);
        assert_eq!(r.state.last_session_id.as_deref(), Some("s-2"));
    }

    // ---- update_visual_fatigue ----

    #[tokio::test]
    async fn update_visual_fatigue_clamps_and_persists() {
        let engine = test_engine(AMASConfig::default());
        let state = engine
            .update_visual_fatigue("u-vf", 50.0)
            .await
            .expect("vf");
        assert!(state.fatigue >= 0.0 && state.fatigue <= 1.0);
        // 再调用一次（已有 state）
        let state2 = engine
            .update_visual_fatigue("u-vf", 200.0) // 200 → 应被 clamp 到 1.0
            .await
            .expect("vf2");
        assert!(state2.fatigue >= 0.0 && state2.fatigue <= 1.0);
    }

    // ---- get_phase ----

    #[tokio::test]
    async fn get_phase_returns_classify_for_new_user() {
        let engine = test_engine(AMASConfig::default());
        let phase = engine.get_phase("u-phase").await.expect("phase");
        assert!(matches!(phase, Some(ColdStartPhase::Classify)));
    }

    #[tokio::test]
    async fn get_phase_returns_none_after_enough_events() {
        let mut cfg = AMASConfig::default();
        cfg.cold_start.classify_to_explore_events = 1;
        cfg.cold_start.explore_to_exploit_events = 2;
        let engine = test_engine(cfg.clone());

        // 写一个 state，让 total_event_count >= explore_to_exploit_events
        let mut state = UserState::default();
        state.total_event_count = 10;
        let json = serde_json::to_value(&state).unwrap();
        engine
            .store
            .set_engine_user_state("u-mature", &json)
            .unwrap();

        let phase = engine.get_phase("u-mature").await.expect("phase");
        assert!(phase.is_none());
    }

    #[tokio::test]
    async fn get_phase_returns_explore_in_mid_range() {
        let mut cfg = AMASConfig::default();
        cfg.cold_start.classify_to_explore_events = 2;
        cfg.cold_start.explore_to_exploit_events = 10;
        let engine = test_engine(cfg);

        let mut state = UserState::default();
        state.total_event_count = 5;
        let json = serde_json::to_value(&state).unwrap();
        engine
            .store
            .set_engine_user_state("u-explore", &json)
            .unwrap();

        let phase = engine.get_phase("u-explore").await.expect("phase");
        assert!(matches!(phase, Some(ColdStartPhase::Explore)));
    }

    // ---- update_temporal_profile & get_temporal_boost ----

    #[tokio::test]
    async fn update_temporal_profile_records_session() {
        let engine = test_engine(AMASConfig::default());
        engine
            .update_temporal_profile("u-tp", 10, 0.9, 800.0, 0.7)
            .await
            .expect("update profile");
        // 调第二次让 EMA 分支生效
        engine
            .update_temporal_profile("u-tp", 10, 0.5, 1200.0, 0.4)
            .await
            .expect("update profile 2");

        let state = engine.get_user_state_async("u-tp").await.unwrap();
        let h = &state.habit_profile.temporal_performance.hourly_stats[10];
        assert_eq!(h.session_count, 2);
        assert!(h.avg_accuracy > 0.0 && h.avg_accuracy < 1.0);
    }

    #[tokio::test]
    async fn update_temporal_profile_clamps_hour_to_23() {
        let engine = test_engine(AMASConfig::default());
        engine
            .update_temporal_profile("u-tp24", 250, 0.9, 800.0, 0.7)
            .await
            .expect("update profile");
        let state = engine.get_user_state_async("u-tp24").await.unwrap();
        let h = &state.habit_profile.temporal_performance.hourly_stats[23];
        assert_eq!(h.session_count, 1);
    }

    #[test]
    fn get_temporal_boost_returns_one_for_unseen_hour() {
        let engine = test_engine(AMASConfig::default());
        let boost = engine.get_temporal_boost("u-unseen", 3).expect("boost");
        assert!((boost - 1.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn get_temporal_boost_after_update_in_range() {
        let engine = test_engine(AMASConfig::default());
        engine
            .update_temporal_profile("u-tp-boost", 10, 0.9, 800.0, 0.8)
            .await
            .expect("update");
        let boost = engine
            .get_temporal_boost_async("u-tp-boost", 10)
            .await
            .expect("boost");
        let cfg = AMASConfig::default();
        assert!(boost >= cfg.feature.temporal_boost_min);
        assert!(boost <= cfg.feature.temporal_boost_max);
    }

    // ---- classify_learner_type ----

    #[test]
    fn classify_learner_type_returns_cautious_for_default_state() {
        let engine = test_engine(AMASConfig::default());
        let t = engine
            .classify_learner_type("u-classify")
            .expect("classify");
        // UserState::default 各项较低 → 走最低分支
        assert!(matches!(
            t,
            LearnerType::Fast | LearnerType::Stable | LearnerType::Cautious
        ));
    }

    #[tokio::test]
    async fn classify_learner_type_async_identifies_fast_when_profile_high() {
        let engine = test_engine(AMASConfig::default());
        let mut state = UserState::default();
        state.cognitive_profile.processing_speed = 1.0;
        state.cognitive_profile.memory_capacity = 1.0;
        state.cognitive_profile.stability = 1.0;
        engine
            .store
            .set_engine_user_state("u-fast", &serde_json::to_value(&state).unwrap())
            .unwrap();
        let t = engine.classify_learner_type_async("u-fast").await.unwrap();
        assert!(matches!(t, LearnerType::Fast));
    }

    #[test]
    fn classify_learner_type_returns_stable_for_mid_range_profile() {
        let engine = test_engine(AMASConfig::default());
        let cfg = AMASConfig::default();
        // 让 auc 略高于 stable_learner_threshold 但低于 fast
        let target_auc =
            (cfg.classifier.stable_learner_threshold + cfg.classifier.fast_learner_threshold) / 2.0;
        // 把所有 profile 设到统一权重位
        let total_weight = cfg.classifier.processing_speed_weight
            + cfg.classifier.memory_capacity_weight
            + cfg.classifier.stability_weight;
        let v = (target_auc / total_weight).clamp(0.0, 1.0);
        let mut state = UserState::default();
        state.cognitive_profile.processing_speed = v;
        state.cognitive_profile.memory_capacity = v;
        state.cognitive_profile.stability = v;
        engine
            .store
            .set_engine_user_state("u-stable", &serde_json::to_value(&state).unwrap())
            .unwrap();
        let t = engine.classify_learner_type("u-stable").unwrap();
        assert!(matches!(t, LearnerType::Stable | LearnerType::Fast));
    }

    // ---- internal: build_feature_vector / compute_engagement via process_event ----

    #[tokio::test]
    async fn process_event_with_hint_used_applies_penalty() {
        let engine = test_engine(AMASConfig::default());
        let r = engine
            .process_event(
                "u-hint",
                RawEvent {
                    word_id: "w-1".to_string(),
                    is_correct: true,
                    response_time_ms: 400,
                    hint_used: true,
                    session_id: Some("s-hint".to_string()),
                    ..RawEvent::default()
                },
            )
            .await
            .unwrap();
        // 结果对象应包含 reward
        assert!(r.reward.value >= -1.0 && r.reward.value <= 1.0);
    }

    #[tokio::test]
    async fn process_event_with_engagement_penalties_works() {
        let engine = test_engine(AMASConfig::default());
        let r = engine
            .process_event(
                "u-eng",
                RawEvent {
                    word_id: "w-1".to_string(),
                    is_correct: false,
                    response_time_ms: 5000,
                    pause_count: Some(3),
                    switch_count: Some(2),
                    focus_loss_duration_ms: Some(8000),
                    is_quit: true,
                    session_id: Some("s-eng".to_string()),
                    ..RawEvent::default()
                },
            )
            .await
            .unwrap();
        // is_quit=true 应增加疲劳
        assert!(r.state.fatigue > 0.0);
    }

    #[tokio::test]
    async fn process_event_after_long_idle_applies_full_reset() {
        let mut cfg = AMASConfig::default();
        cfg.fatigue_decay.decay_start_threshold_secs = 1.0;
        cfg.fatigue_decay.full_reset_threshold_secs = 10.0;
        let engine = test_engine(cfg);

        // 设置 last_active_at 远在过去
        let mut state = UserState::default();
        state.fatigue = 0.9;
        state.last_active_at = Some(chrono::Utc::now() - chrono::Duration::hours(1));
        engine
            .store
            .set_engine_user_state("u-idle", &serde_json::to_value(&state).unwrap())
            .unwrap();

        let r = engine
            .process_event(
                "u-idle",
                RawEvent {
                    word_id: "w-1".to_string(),
                    is_correct: true,
                    response_time_ms: 500,
                    session_id: Some("s-idle".to_string()),
                    ..RawEvent::default()
                },
            )
            .await
            .unwrap();
        // 经过完全衰减后再 +fatigue_increase_rate
        assert!(r.state.fatigue < 0.5);
    }

    // ---- ensemble_or_fallback ----

    #[test]
    fn ensemble_or_fallback_returns_default_for_empty_candidates() {
        let engine = test_engine(AMASConfig::default());
        let (strategy, weights) = engine.ensemble_or_fallback(
            &[],
            &UserState::default(),
            &AlgoStates::default(),
            &AMASConfig::default(),
        );
        assert_eq!(weights.len(), 0);
        assert_eq!(strategy.difficulty, StrategyParams::default().difficulty);
    }

    #[test]
    fn ensemble_or_fallback_picks_highest_confidence_when_ensemble_disabled() {
        let mut cfg = AMASConfig::default();
        cfg.feature_flags.ensemble_enabled = false;
        let engine = test_engine(cfg.clone());
        let candidates = vec![
            DecisionCandidate {
                algorithm_id: AlgorithmId::Heuristic,
                strategy: StrategyParams {
                    difficulty: 0.2,
                    ..StrategyParams::default()
                },
                confidence: 0.3,
                explanation: "h".to_string(),
            },
            DecisionCandidate {
                algorithm_id: AlgorithmId::Ige,
                strategy: StrategyParams {
                    difficulty: 0.9,
                    ..StrategyParams::default()
                },
                confidence: 0.95,
                explanation: "i".to_string(),
            },
        ];
        let (strategy, weights) = engine.ensemble_or_fallback(
            &candidates,
            &UserState::default(),
            &AlgoStates::default(),
            &cfg,
        );
        assert_eq!(weights.get(&AlgorithmId::Ige), Some(&1.0));
        assert!((strategy.difficulty - 0.9).abs() < 1e-9);
    }

    // ---- compute_reward & evaluate_objective ----

    #[test]
    fn compute_reward_applies_fatigue_and_frustration_penalties() {
        let engine = test_engine(AMASConfig::default());
        let feature = FeatureVector {
            accuracy: 0.0,
            response_speed: 0.0,
            quality: 0.0,
            engagement: 0.0,
            hint_penalty: 0.0,
            time_since_last_event_secs: 0.0,
            session_event_count: 0,
            is_quit: false,
        };
        let mut state = UserState::default();
        state.fatigue = 0.95;
        state.motivation = -0.8;
        let reward = engine.compute_reward(&feature, &state, 0.5, &AMASConfig::default());
        assert!(reward.components.fatigue_penalty > 0.0);
        assert!(reward.components.frustration_penalty > 0.0);
        // value clamp
        assert!(reward.value >= -1.0 && reward.value <= 1.0);
    }

    #[test]
    fn evaluate_objective_score_uses_components() {
        let engine = test_engine(AMASConfig::default());
        let reward = Reward {
            value: 0.5,
            components: RewardComponents {
                accuracy_reward: 1.0,
                speed_reward: 0.5,
                fatigue_penalty: 0.1,
                frustration_penalty: 0.0,
                expected_forget_cost: 0.0,
            },
        };
        let evaluation = engine.evaluate_objective(&reward, 0.9, &AMASConfig::default());
        assert_eq!(evaluation.retention_gain, 0.9);
        assert_eq!(evaluation.accuracy_gain, 1.0);
    }

    // ---- apply_constraints ----

    #[test]
    fn apply_constraints_fatigue_caps_strategy() {
        let engine = test_engine(AMASConfig::default());
        let mut state = UserState::default();
        state.fatigue = 0.95;
        let cfg = AMASConfig::default();
        let s = engine.apply_constraints(
            StrategyParams {
                batch_size: 50,
                new_ratio: 0.8,
                difficulty: 0.9,
                ..StrategyParams::default()
            },
            &state,
            &cfg,
        );
        assert!(s.batch_size <= cfg.constraints.max_batch_size_when_fatigued);
        assert!(s.new_ratio <= cfg.constraints.max_new_ratio_when_fatigued);
        assert!(s.difficulty <= cfg.constraints.max_difficulty_when_fatigued);
    }

    #[test]
    fn apply_constraints_low_attention_forces_review_mode() {
        let engine = test_engine(AMASConfig::default());
        let mut state = UserState::default();
        state.attention = 0.05;
        let s = engine.apply_constraints(StrategyParams::default(), &state, &AMASConfig::default());
        assert!(s.review_mode);
        assert_eq!(s.new_ratio, 0.0);
    }

    #[test]
    fn apply_constraints_low_motivation_reduces_difficulty() {
        let engine = test_engine(AMASConfig::default());
        let mut state = UserState::default();
        state.motivation = -0.9;
        let cfg = AMASConfig::default();
        let s = engine.apply_constraints(
            StrategyParams {
                difficulty: 0.8,
                new_ratio: 0.5,
                ..StrategyParams::default()
            },
            &state,
            &cfg,
        );
        assert!(s.difficulty <= 0.8);
        assert!(s.new_ratio <= 0.5);
    }

    #[test]
    fn apply_constraints_enforces_minimums() {
        let engine = test_engine(AMASConfig::default());
        let s = engine.apply_constraints(
            StrategyParams {
                batch_size: 0,
                interval_scale: -1.0,
                difficulty: -0.5,
                new_ratio: 5.0,
                ..StrategyParams::default()
            },
            &UserState::default(),
            &AMASConfig::default(),
        );
        assert!(s.batch_size >= 1);
        assert!(s.interval_scale >= 0.1);
        assert!(s.difficulty >= 0.0 && s.difficulty <= 1.0);
        assert!(s.new_ratio >= 0.0 && s.new_ratio <= 1.0);
    }

    // ---- build_explanation primary reason branches ----

    #[test]
    fn build_explanation_reports_fatigue_when_high() {
        let engine = test_engine(AMASConfig::default());
        let mut state = UserState::default();
        state.fatigue = 0.95;
        let exp = engine.build_explanation(&StrategyParams::default(), &state, &HashMap::new());
        assert!(exp.primary_reason.contains("疲劳"));
    }

    #[test]
    fn build_explanation_reports_review_mode() {
        let engine = test_engine(AMASConfig::default());
        let strategy = StrategyParams {
            review_mode: true,
            ..StrategyParams::default()
        };
        let exp = engine.build_explanation(&strategy, &UserState::default(), &HashMap::new());
        assert!(exp.primary_reason.contains("复习"));
    }

    #[test]
    fn build_explanation_reports_low_confidence() {
        let engine = test_engine(AMASConfig::default());
        let mut state = UserState::default();
        state.confidence = 0.1;
        let exp = engine.build_explanation(&StrategyParams::default(), &state, &HashMap::new());
        assert!(exp.primary_reason.contains("信心"));
    }

    #[test]
    fn build_explanation_reports_low_attention() {
        let engine = test_engine(AMASConfig::default());
        let mut state = UserState::default();
        state.attention = 0.2;
        state.confidence = 0.8;
        let exp = engine.build_explanation(&StrategyParams::default(), &state, &HashMap::new());
        assert!(exp.primary_reason.contains("注意力"));
    }

    #[test]
    fn build_explanation_reports_dominant_algorithm() {
        let engine = test_engine(AMASConfig::default());
        let mut state = UserState::default();
        state.attention = 0.9;
        state.confidence = 0.9;
        state.motivation = 0.0;
        let mut weights = HashMap::new();
        weights.insert(AlgorithmId::Heuristic, 0.9);
        weights.insert(AlgorithmId::Ige, 0.1);
        let exp = engine.build_explanation(&StrategyParams::default(), &state, &weights);
        // 应包含算法描述（经验规则）
        assert!(exp.primary_reason.contains("经验规则") || exp.primary_reason.contains("综合"));
    }

    #[test]
    fn build_explanation_default_when_no_dominant_algorithm() {
        let engine = test_engine(AMASConfig::default());
        let mut state = UserState::default();
        state.attention = 0.9;
        state.confidence = 0.9;
        state.motivation = 0.0;
        // 全部权重低
        let mut weights = HashMap::new();
        weights.insert(AlgorithmId::Heuristic, 0.2);
        weights.insert(AlgorithmId::Ige, 0.2);
        weights.insert(AlgorithmId::Swd, 0.2);
        let exp = engine.build_explanation(&StrategyParams::default(), &state, &weights);
        assert!(exp.primary_reason.contains("综合"));
    }

    #[test]
    fn build_explanation_reports_progress_when_high_difficulty_and_motivation() {
        let engine = test_engine(AMASConfig::default());
        let mut state = UserState::default();
        state.attention = 0.9;
        state.confidence = 0.9;
        state.motivation = 0.8;
        let strategy = StrategyParams {
            difficulty: 0.8,
            ..StrategyParams::default()
        };
        let exp = engine.build_explanation(&strategy, &state, &HashMap::new());
        assert!(exp.primary_reason.contains("加速") || exp.primary_reason.contains("综合"));
    }

    // ---- load_or_init / acquire_user_lock_blocking 通过 process_event 间接覆盖（多次） ----

    #[tokio::test]
    async fn many_users_exercise_user_lock_cleanup() {
        let engine = test_engine(AMASConfig::default());
        for i in 0..30 {
            engine
                .process_event(
                    &format!("u-{i}"),
                    RawEvent {
                        word_id: "w".to_string(),
                        is_correct: true,
                        response_time_ms: 200,
                        ..RawEvent::default()
                    },
                )
                .await
                .unwrap();
        }
        // 验证不 panic 即可
    }

    // ---- update_memory with feature flags on ----

    #[tokio::test]
    async fn process_event_with_evm_and_ignored_confused_with() {
        // IAD/MTP 算法已移除（2026-06-13）。EVM 无 flag 恒运行（session_id 驱动）；
        // confused_with 字段保留为「接受但忽略」（不再被任何算法消费，不得致错）。
        let engine = test_engine(AMASConfig::default());
        let r = engine
            .process_event(
                "u-flags",
                RawEvent {
                    word_id: "w-evm".to_string(),
                    is_correct: true,
                    response_time_ms: 400,
                    confused_with: Some("w-confused".to_string()),
                    session_id: Some("s-flags".to_string()),
                    ..RawEvent::default()
                },
            )
            .await
            .unwrap();
        assert!(r.state.total_event_count >= 1);
    }

    #[test]
    fn sanitize_float_replaces_non_finite_with_default() {
        assert_eq!(sanitize_float(1.5, 0.0), 1.5);
        assert_eq!(sanitize_float(f64::NAN, 0.5), 0.5);
        assert_eq!(sanitize_float(f64::INFINITY, 0.5), 0.5);
        assert_eq!(sanitize_float(f64::NEG_INFINITY, 0.5), 0.5);
    }

    #[tokio::test]
    async fn load_algo_states_uses_fallback_on_corrupted_json() {
        let engine = test_engine(AMASConfig::default());
        // 写入故意损坏的 algo state（不符合 Schema 的 JSON）
        let bad = serde_json::json!({"unexpected_field": "x", "ige_state_should_have": null});
        engine
            .store
            .set_engine_algo_state("u-bad", "ige", &bad)
            .unwrap();
        engine
            .store
            .set_engine_algo_state("u-bad", "swd", &bad)
            .unwrap();
        engine
            .store
            .set_engine_algo_state("u-bad", "trust", &bad)
            .unwrap();

        // 触发 load_algo_states + warning fallback —— 通过 process_event 间接
        let r = engine
            .process_event(
                "u-bad",
                RawEvent {
                    word_id: "w-bad".to_string(),
                    is_correct: true,
                    response_time_ms: 500,
                    session_id: Some("s-bad".to_string()),
                    ..RawEvent::default()
                },
            )
            .await
            .unwrap();
        assert!(r.state.total_event_count >= 1);
    }

    #[test]
    fn derive_primary_reason_reports_ige_dominant() {
        let mut state = UserState::default();
        state.attention = 0.9;
        state.confidence = 0.9;
        state.motivation = 0.0;
        let mut weights = HashMap::new();
        weights.insert(AlgorithmId::Ige, 0.8);
        let reason =
            AMASEngine::derive_primary_reason(&StrategyParams::default(), &state, &weights);
        assert!(reason.contains("智能梯度"));
    }

    #[test]
    fn derive_primary_reason_reports_swd_dominant() {
        let mut state = UserState::default();
        state.attention = 0.9;
        state.confidence = 0.9;
        state.motivation = 0.0;
        let mut weights = HashMap::new();
        weights.insert(AlgorithmId::Swd, 0.85);
        let reason =
            AMASEngine::derive_primary_reason(&StrategyParams::default(), &state, &weights);
        assert!(reason.contains("间隔加权"));
    }

    #[test]
    fn derive_primary_reason_reports_mdm_fallback_label() {
        let mut state = UserState::default();
        state.attention = 0.9;
        state.confidence = 0.9;
        state.motivation = 0.0;
        let mut weights = HashMap::new();
        weights.insert(AlgorithmId::Mdm, 0.9); // 不在 Heuristic/Ige/Swd 中，走 "_ => 综合评估"
        let reason =
            AMASEngine::derive_primary_reason(&StrategyParams::default(), &state, &weights);
        assert!(reason.contains("综合评估"));
    }

    #[tokio::test]
    async fn process_event_with_empty_word_id_skips_memory() {
        let engine = test_engine(AMASConfig::default());
        let r = engine
            .process_event(
                "u-empty",
                RawEvent {
                    word_id: "".to_string(),
                    is_correct: true,
                    response_time_ms: 400,
                    session_id: Some("s-empty".to_string()),
                    ..RawEvent::default()
                },
            )
            .await
            .unwrap();
        assert!(r.word_mastery.is_none());
    }

    // ---- D3:per-patch canary 多路由 ----

    #[test]
    fn effective_config_routes_by_patch_canary_cohort() {
        use crate::store::operations::amas_versions::ConfigVersionSource;
        let engine = test_engine(AMASConfig::default());
        // 落一个 canary version snapshot(改一个可观测字段,与 stable 不同)
        let mut canary_cfg = engine.get_config();
        canary_cfg.memory_model.base_desired_retention = 0.99;
        let snap = serde_json::to_string(&canary_cfg).unwrap();
        let (_id, vhash) = engine
            .store
            .insert_amas_config_version(&snap, "admin", ConfigVersionSource::Manual, None, None)
            .unwrap();
        // 灰度 [0,20):宽度=percent=20
        engine
            .store
            .insert_patch_canary(1, &vhash, 20, 0, 20, "{}")
            .unwrap();

        // 命中桶的用户拿 canary,未命中的拿 stable
        let mut hit = 0usize;
        for i in 0..200 {
            let uid = format!("user-{i}");
            let cfg = engine.effective_config_for_user(&uid);
            if (cfg.memory_model.base_desired_retention - 0.99).abs() < 1e-9 {
                hit += 1;
            }
        }
        // 20% 桶 → 200 用户里命中数应在合理区间(非 0、非全部)
        assert!(hit > 0 && hit < 200, "hit={hit} 应落在 (0,200)");
    }

    #[test]
    fn effective_config_falls_back_stable_on_missing_version() {
        let engine = test_engine(AMASConfig::default());
        // canary 指向不存在的 version_hash → 全员回退 stable
        engine
            .store
            .insert_patch_canary(1, "nonexistent-hash", 100, 0, 100, "{}")
            .unwrap();
        let stable = engine.get_config();
        let cfg = engine.effective_config_for_user("any-user");
        assert_eq!(
            cfg.memory_model.base_desired_retention,
            stable.memory_model.base_desired_retention
        );
    }

    #[test]
    fn effective_config_no_active_canary_returns_stable() {
        let engine = test_engine(AMASConfig::default());
        let stable = engine.get_config();
        let cfg = engine.effective_config_for_user("user-x");
        assert_eq!(
            cfg.memory_model.base_desired_retention,
            stable.memory_model.base_desired_retention
        );
    }

    /// #47：引擎级幂等回滚分支覆盖。预先写入 processed_events 标记，再调
    /// process_event_idempotent：persist_engine_state_atomic 命中 `INSERT OR IGNORE` 0 行 →
    /// 整笔回滚 → 引擎早退返回 Ok(None)，且 user_state / per-word mastery / user_elo 零变更。
    #[tokio::test]
    async fn process_event_idempotent_rolls_back_on_existing_marker() {
        let engine = test_engine(AMASConfig::default());
        let user = "u-idem";
        let key = "rec-dup-1";

        // 预置标记：模拟并发同 client_record_id 请求已先行处理。
        engine.store.mark_event_processed(user, key).unwrap();

        let result = engine
            .process_event_idempotent(
                user,
                RawEvent {
                    word_id: "w-idem".to_string(),
                    is_correct: true,
                    response_time_ms: 500,
                    session_id: Some("session-A".to_string()),
                    ..RawEvent::default()
                },
                key,
            )
            .await
            .expect("process_event_idempotent should not error");

        // 核心断言：标记已存在 → 整笔回滚 → Ok(None)。
        assert!(result.is_none(), "已存在标记时应返回 Ok(None)");

        // 零变更：user_state 未落库、mastery / EVM per-word 状态未写、user_elo 仍为默认。
        assert!(
            engine.store.get_engine_user_state(user).unwrap().is_none(),
            "回滚后不应写入 user_state"
        );
        assert!(engine
            .store
            .get_engine_algo_state(user, "mastery:w-idem")
            .unwrap()
            .is_none());
        assert!(engine
            .store
            .get_engine_algo_state(user, "evm:w-idem")
            .unwrap()
            .is_none());
        let elo = engine.store.get_user_elo(user).unwrap();
        assert_eq!(elo.games, 0, "ELO 不应被回滚的事件累加");
    }

    /// #47：首个（胜出）请求应正常落库并写入标记，返回 Ok(Some)。与上面的回滚分支配对。
    #[tokio::test]
    async fn process_event_idempotent_first_call_applies_and_marks() {
        let engine = test_engine(AMASConfig::default());
        let user = "u-idem-win";
        let key = "rec-win-1";

        let result = engine
            .process_event_idempotent(
                user,
                RawEvent {
                    word_id: "w-win".to_string(),
                    is_correct: true,
                    response_time_ms: 500,
                    session_id: Some("session-A".to_string()),
                    ..RawEvent::default()
                },
                key,
            )
            .await
            .expect("process_event_idempotent");

        assert!(result.is_some(), "首次调用应返回 Ok(Some)");
        assert!(
            engine.store.is_event_processed(user, key).unwrap(),
            "首次调用应写入幂等标记"
        );
        assert!(engine.store.get_engine_user_state(user).unwrap().is_some());
        // ELO 已在同一原子 tx 内累加（#11/#39）。
        let elo = engine.store.get_user_elo(user).unwrap();
        assert_eq!(elo.games, 1, "胜出请求应在同 tx 内累加一局 ELO");
    }
}
