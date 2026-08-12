use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;

use crate::extractors::JsonBody;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::amas::types::{MasteryLevel, ProcessResult, RawEvent};
use crate::auth::AuthUser;
use crate::constants::{DEFAULT_HALF_LIFE_HOURS, DEFAULT_PAGE_SIZE_RECORDS, MAX_PAGE_SIZE};
use crate::response::{created, ok, paginated, AppError};
use crate::services::event_bus::DomainEvent;
use crate::state::AppState;
use crate::store::operations::learning_sessions::LearningSession;
use crate::store::operations::records::{LearningRecord, RecordType};
use crate::store::operations::word_states::{WordLearningState, WordState};

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list_records).post(create_record))
}

/// 路由层 per-user 流水线互斥：把「process_event → tx2 落库」整段纳入同一临界区，
/// 串行化同用户事件的应用与落库顺序。引擎内部的 user_locks 只覆盖 process_event 本身。
///（S2 收尾删除快照回滚后仍保留：保持 AMAS 应用序 = 落库序，避免会话/词态计数交错。）
///
/// 独立建 map、不复用引擎 user_locks：引擎锁是 parking_lot 同步锁、在 spawn_blocking 线程内
/// 短持有；这里临界区跨多个 await（store task + 引擎调用），须用 tokio 异步锁，且复用同一把
/// 会在「路由持锁 → 引擎再锁」时自我死锁。锁序恒为 路由锁 → 引擎锁，无逆序获取。
/// 容量治理复制引擎 user_locks 模式：超阈值时清掉无人持有的空闲项（Arc 强计数=1）。
static USER_PIPELINE_LOCKS: once_cell::sync::Lazy<
    parking_lot::Mutex<std::collections::HashMap<String, std::sync::Arc<tokio::sync::Mutex<()>>>>,
> = once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(std::collections::HashMap::new()));

pub(crate) async fn acquire_user_pipeline_lock(user_id: &str) -> tokio::sync::OwnedMutexGuard<()> {
    let lock = {
        let mut locks = USER_PIPELINE_LOCKS.lock();
        if locks.len() > crate::amas::constants::USER_LOCK_CLEANUP_THRESHOLD {
            locks.retain(|_, v| std::sync::Arc::strong_count(v) > 1);
        }
        locks
            .entry(user_id.to_string())
            .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    };
    lock.lock_owned().await
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListRecordsQuery {
    page: Option<u64>,
    per_page: Option<u64>,
}

impl ListRecordsQuery {
    fn page(&self) -> u64 {
        self.page.unwrap_or(1).clamp(1, u64::MAX)
    }
    fn per_page(&self) -> u64 {
        self.per_page
            .unwrap_or(DEFAULT_PAGE_SIZE_RECORDS)
            .clamp(1, MAX_PAGE_SIZE)
    }
}

async fn list_records(
    auth: AuthUser,
    Query(q): Query<ListRecordsQuery>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let page = q.page();
    let per_page = q.per_page();
    let limit = per_page as usize;
    let offset = ((page - 1) * per_page) as usize;
    let (records, total) = state
        .run_store_task("records.list", move |store| {
            Ok::<_, crate::store::StoreError>((
                store.get_user_records_with_offset(&auth.user_id, limit, offset)?,
                store.count_user_records(&auth.user_id)? as u64,
            ))
        })
        .await??;
    Ok(paginated(records, total, page, per_page))
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreateRecordRequest {
    pub(crate) client_record_id: Option<String>,
    pub(crate) word_id: String,
    pub(crate) is_correct: bool,
    pub(crate) response_time_ms: i64,
    pub(crate) session_id: Option<String>,
    pub(crate) is_quit: Option<bool>,
    pub(crate) dwell_time_ms: Option<i64>,
    pub(crate) pause_count: Option<i32>,
    pub(crate) switch_count: Option<i32>,
    pub(crate) retry_count: Option<i32>,
    pub(crate) focus_loss_duration_ms: Option<i64>,
    pub(crate) interaction_density: Option<f64>,
    pub(crate) paused_time_ms: Option<i64>,
    pub(crate) hint_used: Option<bool>,
    #[serde(default)]
    pub(crate) confused_with: Option<String>,
    #[serde(default)]
    pub(crate) record_type: Option<RecordType>,
    /// SRS 自评粒度（0=Again / 1=Hard / 2=Good / 3=Easy）；
    /// 客户端选填，落库供 AMAS half-life 模型分级回退使用。
    #[serde(default)]
    pub(crate) self_rating: Option<u8>,
    /// 出题模式（word-to-meaning / meaning-to-word / audio-to-meaning / meaning-to-spelling）；
    /// 客户端选填，落库供数据分析"答题分布·题型"使用。非法值原样存，NULL 视为"未标注"。
    #[serde(default)]
    pub(crate) question_mode: Option<String>,
    /// 内部派生路径专用：覆盖 created_at（不参与 JSON 反序列化）。
    #[serde(skip)]
    pub(crate) created_at_override: Option<DateTime<Utc>>,
}

impl CreateRecordRequest {
    pub(crate) fn record_type_or_default(&self) -> RecordType {
        self.record_type.unwrap_or_default()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreateRecordResponse {
    pub(crate) record: LearningRecord,
    pub(crate) amas_result: Option<ProcessResult>,
    pub(crate) duplicate: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct UserStateSnapshot {
    pub(crate) user_state: Option<serde_json::Value>,
    pub(crate) ige: Option<serde_json::Value>,
    /// 写放大重构：批量首条前的 swd 历史最大 seq。全批失败回滚时删 `seq > swd_max_seq` 的行，
    /// 等价于把 swd 还原到批前快照（旧实现是覆写整块 swd blob）。
    pub(crate) swd_max_seq: i64,
    pub(crate) trust: Option<serde_json::Value>,
    pub(crate) user_elo: crate::amas::elo::EloRating,
    /// user_elo.trend 快照（EloRating 不含此列），回滚时一并复位，防动态 K 污染残留。
    pub(crate) user_elo_trend: f64,
}

pub(crate) fn capture_user_state_snapshot(
    store: &crate::store::Store,
    user_id: &str,
) -> Result<UserStateSnapshot, AppError> {
    Ok(UserStateSnapshot {
        user_state: store.get_engine_user_state(user_id)?,
        ige: store.get_engine_algo_state(user_id, "ige")?,
        swd_max_seq: store.swd_max_seq(user_id)?,
        trust: store.get_engine_algo_state(user_id, "trust")?,
        user_elo: store.get_user_elo(user_id)?,
        user_elo_trend: store.get_user_elo_trend(user_id)?,
    })
}

pub(crate) fn restore_user_state_snapshot(
    store: &crate::store::Store,
    user_id: &str,
    snapshot: &UserStateSnapshot,
) {
    match &snapshot.user_state {
        Some(previous) => {
            if let Err(error) = store.set_engine_user_state(user_id, previous) {
                tracing::warn!(user_id, error = %error, "Failed to rollback AMAS user state");
            }
        }
        None => {
            if let Err(error) = store.delete_engine_user_state(user_id) {
                tracing::warn!(user_id, error = %error, "Failed to delete AMAS user state during rollback");
            }
        }
    }

    restore_engine_algo_state(store, user_id, "ige", &snapshot.ige);
    restore_engine_algo_state(store, user_id, "trust", &snapshot.trust);
    // 写放大重构：删批内 append 的所有 swd 历史行（seq > 批前 max），等价还原 swd 到批前快照。
    if let Err(error) = store.delete_swd_history_after(user_id, snapshot.swd_max_seq) {
        tracing::warn!(user_id, error = %error, "Failed to rollback swd history");
    }

    if let Err(error) = store.set_user_elo(user_id, &snapshot.user_elo) {
        tracing::warn!(user_id, error = %error, "Failed to rollback user ELO");
    }
    // set_user_elo 只写 rating/games，trend（动态 K 残差 EWMA）须单独复位。
    if let Err(error) = store.set_user_elo_trend(user_id, snapshot.user_elo_trend) {
        tracing::warn!(user_id, error = %error, "Failed to rollback user ELO trend");
    }
}

pub(crate) fn restore_engine_algo_state(
    store: &crate::store::Store,
    user_id: &str,
    algo_id: &str,
    previous: &Option<serde_json::Value>,
) {
    let result = match previous {
        Some(value) => store.set_engine_algo_state(user_id, algo_id, value),
        None => store.delete_engine_algo_state(user_id, algo_id),
    };

    if let Err(error) = result {
        tracing::warn!(user_id, algo_id, error = %error, "Failed to rollback AMAS algorithm state");
    }
}

/// S2-1：outbox 异步路径的事件载荷。异步模式下 handler 把整条创建请求持久化到 outbox，
/// outbox_processor worker 反序列化后调用 [`process_single_record`] 走与同步路径完全一致的
/// 处理逻辑（零逻辑分叉）。`created_at` 固化客户端提交时刻。
/// S2 收尾后落库失败不再手动回滚引擎状态，恢复统一走幂等账本短路。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct OutboxRecordPayload {
    pub(crate) user_id: String,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) request: CreateRecordRequest,
}

pub(crate) async fn process_single_record(
    user_id: &str,
    req: &CreateRecordRequest,
    state: &AppState,
    request_id: Option<String>,
) -> Result<CreateRecordResponse, AppError> {
    let user_id_owned = user_id.to_string();
    let record_id = req
        .client_record_id
        .as_ref()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    if let Some(existing) = state
        .run_store_task("records.single.check_duplicate", {
            let user_id = user_id_owned.clone();
            let record_id = record_id.clone();
            move |store| store.get_user_record_by_id(&user_id, &record_id)
        })
        .await??
    {
        return Ok(CreateRecordResponse {
            record: existing,
            amas_result: None,
            duplicate: true,
        });
    }

    let record = LearningRecord {
        id: record_id,
        user_id: user_id.to_string(),
        word_id: req.word_id.clone(),
        is_correct: req.is_correct,
        response_time_ms: req.response_time_ms,
        session_id: req.session_id.clone(),
        created_at: req.created_at_override.unwrap_or_else(Utc::now),
        record_type: req.record_type_or_default(),
        self_rating: req.self_rating,
        question_mode: req.question_mode.clone(),
    };
    let word_id = req.word_id.clone();
    let record_for_store = record.clone();
    let req_for_store = req.clone();

    // W1-1：幂等账本预检。check_duplicate 只挡记录行重复;此处专挡"AMAS 状态已在先前(崩溃)
    // 尝试中应用、但记录行未落库"的窗口——命中则跳过 process_event(避免二次累加
    // ELO/mastery/trust),仅补落裸记录行(不带 AMAS 派生的 word_state/session 增量,那些已在
    // 原尝试中处理或随该次崩溃丢失,属单事件有界损失,优于无界的 AMAS 状态漂移)。
    // 注:marker 在 AMAS tx 内提交、记录行在其后单独提交,二者之间存在"标记在、记录行未落"窗口。
    // 并发同 client_record_id 请求可能在该窗口走到这里抢先裸插入,但 create_record_with_updates
    // 的记录行 INSERT 已是幂等(ON CONFLICT DO NOTHING)且 user_stats 仅按真实插入计数,故裸回放与
    // 在途全量持久化互不破坏——全量方不会因裸行主键冲突而误回滚 AMAS(PR #61 审查 P1)。
    let already_processed = state
        .run_store_task("records.single.check_processed", {
            let user_id = user_id_owned.clone();
            let record_id = record.id.clone();
            move |store| store.is_event_processed(&user_id, &record_id)
        })
        .await??;
    if already_processed {
        let record_for_replay = record.clone();
        state
            .run_store_task("records.single.persist_replayed", move |store| {
                store
                    .create_record_with_updates(&record_for_replay, None, None, false)
                    .map_err(|e| AppError::internal(&e.to_string()))
            })
            .await??;
        return Ok(CreateRecordResponse {
            record,
            amas_result: None,
            duplicate: true,
        });
    }

    // per-user 流水线互斥（见 USER_PIPELINE_LOCKS）：串行化同用户「AMAS → tx2」流水线，
    // 保持事件应用顺序与落库顺序一致（S2 收尾删除快照回滚后仍保留，行为不回归）。
    let _pipeline_guard = acquire_user_pipeline_lock(user_id).await;

    // BA3b：从 AMAS 阶段起锚定瀑布 trace（record.id 已就绪）。早退的重复/并发坍缩路径
    // 不调用 finish()，故不入环 —— trace 只记录真正完整处理的事件。
    let mut trace = crate::stage_metrics::TraceBuilder::start(&record.id);
    let t_amas = std::time::Instant::now();
    let amas_result = state
        .amas()
        .process_event_idempotent(
            user_id,
            RawEvent {
                word_id: req.word_id.clone(),
                is_correct: req.is_correct,
                response_time_ms: req.response_time_ms,
                session_id: req.session_id.clone(),
                is_quit: req.is_quit.unwrap_or(false),
                dwell_time_ms: req.dwell_time_ms,
                pause_count: req.pause_count,
                switch_count: req.switch_count,
                retry_count: req.retry_count,
                focus_loss_duration_ms: req.focus_loss_duration_ms,
                interaction_density: req.interaction_density,
                paused_time_ms: req.paused_time_ms,
                hint_used: req.hint_used.unwrap_or(false),
                confused_with: req.confused_with.clone(),
                question_mode: req.question_mode.clone(),
            },
            &record.id,
            request_id,
            // 正规学习流：计入全局 ELO 与 mastery 边沿流水（amas 诊断端点传 false）。
            true,
        )
        .await?;
    crate::stage_metrics::stage_observe(
        crate::stage_metrics::Stage::Amas,
        t_amas.elapsed().as_secs_f64() * 1000.0,
        false,
    );
    trace.mark("amas");
    // W1-1 并发收口：None 表示并发同 client_record_id 请求抢先写入幂等标记、本次 AMAS 已整笔回滚，
    // 走与 already_processed 一致的裸记录回放（不重复累加 ELO/mastery/trust）。
    let Some((amas_result, swd_appended_seq)) = amas_result else {
        let record_for_replay = record.clone();
        state
            .run_store_task("records.single.persist_replayed", move |store| {
                store
                    .create_record_with_updates(&record_for_replay, None, None, false)
                    .map_err(|e| AppError::internal(&e.to_string()))
            })
            .await??;
        return Ok(CreateRecordResponse {
            record,
            amas_result: None,
            duplicate: true,
        });
    };
    // S2 收尾：快照回滚已删除，swd append 句柄不再需要（重试恢复统一走幂等账本短路）。
    let _ = swd_appended_seq;
    let amas_result_for_store = amas_result.clone();

    let t_sql = std::time::Instant::now();
    state
        .run_store_task(
            "records.single.persist",
            move |store| -> Result<_, AppError> {
                // ELO 现由 AMAS 引擎在 process_event_blocking → persist_engine_state_atomic
                // 的锁定原子事务内应用，路由侧不再重复 RMW（否则 ELO 双重累加）。
                let mut next_word_state: Option<WordLearningState> = None;
                if let Some(ref wm) = amas_result_for_store.word_mastery {
                    let new_state = match wm.mastery_level {
                        MasteryLevel::New => WordState::New,
                        MasteryLevel::Learning => WordState::Learning,
                        MasteryLevel::Reviewing => WordState::Reviewing,
                        MasteryLevel::Mastered => WordState::Mastered,
                        MasteryLevel::Forgotten => WordState::Forgotten,
                    };

                    let mut wls = store
                        .get_word_learning_state(&user_id_owned, &word_id)?
                        .unwrap_or_else(|| WordLearningState {
                            user_id: user_id_owned.clone(),
                            word_id: word_id.clone(),
                            state: WordState::New,
                            mastery_level: 0.0,
                            next_review_date: None,
                            half_life: DEFAULT_HALF_LIFE_HOURS,
                            correct_streak: 0,
                            total_attempts: 0,
                            updated_at: Utc::now(),
                        });

                    wls.state = new_state;
                    wls.mastery_level = wm.memory_strength;
                    // total_attempts/correct_streak 不在此累加：store 层 create_record_with_updates
                    // 以 SQL 相对自增（基于行自身当前值 +1 / 归零）落库，struct 里这两个字段仅占位、
                    // 写入时被忽略——在此 Rust 端累加是死代码，徒增"绝对值被采信"的误读。
                    if wm.next_review_interval_secs > 0 {
                        wls.next_review_date = Some(
                            Utc::now() + chrono::Duration::seconds(wm.next_review_interval_secs),
                        );
                    }
                    wls.updated_at = Utc::now();
                    next_word_state = Some(wls);
                }
                // Passed to create_record_with_updates as a delta (not derived from the struct
                // above, which store.rs now writes as a relative +1 off the row's own value —
                // see create_record_with_updates's learning_sessions comment).
                // 注意语义：这是「本次答题后处于 Mastered 级」的水平判定，非进入边沿（无
                // prev!=Mastered 检测），已 Mastered 的词每答一次 actual_mastery_count 都 +1。
                let just_mastered = amas_result_for_store
                    .word_mastery
                    .as_ref()
                    .is_some_and(|wm| wm.mastery_level == MasteryLevel::Mastered);

                let mut next_session: Option<LearningSession> = None;
                if let Some(ref sid) = req_for_store.session_id {
                    // 归属校验：仅累加调用者本人的会话，防止跨用户篡改他人会话统计 (IDOR)。
                    if let Some(mut session) = store
                        .get_learning_session(sid)?
                        .filter(|s| s.user_id == user_id_owned)
                    {
                        session.total_questions += 1;
                        session.total_count += 1;
                        if req_for_store.is_correct {
                            session.correct_count += 1;
                        }
                        if let Some(ref wm) = amas_result_for_store.word_mastery {
                            if wm.mastery_level == MasteryLevel::Mastered {
                                session.actual_mastery_count += 1;
                            }
                        }
                        session.updated_at = Utc::now();
                        next_session = Some(session);
                    }
                }

                store
                    .create_record_with_updates(
                        &record_for_store,
                        next_word_state.as_ref(),
                        next_session.as_ref(),
                        just_mastered,
                    )
                    .map_err(|error| {
                        // S2 收尾：手动快照回滚已删除。tx2 失败时 AMAS 状态与幂等标记原样保留
                        //（「标记存在 ⟺ AMAS 已应用」不变式仍成立），重试（outbox worker 退避 /
                        // 客户端离线队列同 clientRecordId 重发）命中幂等账本短路，仅补落裸记录行
                        // ——与崩溃窗口重放同一恢复语义：单事件有界损失（word_state/session 增量），
                        // 无 AMAS 双重累加，无陈旧快照覆盖并发已提交状态的风险。
                        tracing::warn!(
                            user_id = %user_id_owned,
                            word_id = %word_id,
                            record_id = %record_for_store.id,
                            error = %error,
                            "记录落库失败（AMAS 已应用、标记保留），等待重试经幂等账本补落记录行"
                        );
                        AppError::internal(&error.to_string())
                    })?;
                Ok(())
            },
        )
        .await??;
    crate::stage_metrics::stage_observe(
        crate::stage_metrics::Stage::Sqlite,
        t_sql.elapsed().as_secs_f64() * 1000.0,
        false,
    );
    trace.mark("sqlite");

    // v1.1-P1 S2：写库成功后旁路 emit RecordCreated；
    // AMAS 已在前面同步通路执行过，这里只是事件总线基础设施落地，
    // 为未来 outbox 持久化 + 真异步消费铺路。fire-and-forget 不阻塞 HTTP 响应。
    let t_sse = std::time::Instant::now();
    state.event_bus().emit(DomainEvent::RecordCreated {
        user_id: record.user_id.clone(),
        word_id: record.word_id.clone(),
        record_id: record.id.clone(),
        is_correct: record.is_correct,
        response_time_ms: record.response_time_ms,
        session_id: record.session_id.clone(),
        record_type: record.record_type,
        created_at: record.created_at,
    });
    crate::stage_metrics::stage_observe(
        crate::stage_metrics::Stage::Sse,
        t_sse.elapsed().as_secs_f64() * 1000.0,
        false,
    );
    trace.mark("sse");
    trace.finish();

    Ok(CreateRecordResponse {
        record,
        amas_result: Some(amas_result),
        duplicate: false,
    })
}

async fn create_record(
    auth: AuthUser,
    State(state): State<AppState>,
    request_id: Option<axum::Extension<crate::middleware::request_id::RequestId>>,
    JsonBody(req): JsonBody<CreateRecordRequest>,
) -> Result<axum::response::Response, AppError> {
    // 任务C:从请求扩展取 request_id（中间件注入），透传到 AMAS 决策以关联监控事件与日志。
    let request_id = request_id.map(|axum::Extension(rid)| rid.0);
    // S2（v1.3.0 起默认路径，RECORDS_OUTBOX_ASYNC 默认 true）：把创建请求持久化到 outbox 并
    // 立即 202 返回，AMAS 处理交 outbox_processor 异步消费。三端 ≥1.6.0 已确认容忍无
    // amas_result 的 202 响应。设 RECORDS_OUTBOX_ASYNC=false 可回退下方同步老路。
    // 范围：异步路径仅覆盖单条上报；批量 /records/batch 仍恒走同步路径（异步化待后续）。
    // 崩溃窗口重放已由 processed_events 幂等账本短路（W1-1），不会二次累加 AMAS 状态。
    if state.config().records_outbox_async {
        let mut req = req.clone();
        // 确保 client_record_id 存在：worker 重试时凭它幂等去重，避免重复落库。
        if req
            .client_record_id
            .as_deref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
        {
            req.client_record_id = Some(uuid::Uuid::new_v4().to_string());
        }
        let client_record_id = req.client_record_id.clone();
        let payload = OutboxRecordPayload {
            user_id: auth.user_id.clone(),
            created_at: Utc::now(),
            request: req,
        };
        let json =
            serde_json::to_string(&payload).map_err(|e| AppError::internal(&e.to_string()))?;
        let t_outbox = std::time::Instant::now();
        state
            .run_store_task("records.outbox.enqueue", move |store| {
                store.enqueue_outbox_event("record_created", &json)
            })
            .await??;
        crate::stage_metrics::stage_observe(
            crate::stage_metrics::Stage::Outbox,
            t_outbox.elapsed().as_secs_f64() * 1000.0,
            false,
        );
        return Ok((
            axum::http::StatusCode::ACCEPTED,
            axum::Json(serde_json::json!({
                "accepted": true,
                "async": true,
                "clientRecordId": client_record_id,
            })),
        )
            .into_response());
    }

    // m037 软拦截:处理失败时告警 admin(单条失败客户端已收错误响应,不重复发 user 通知),
    // 仍返回原错误不吞。
    let result = match process_single_record(&auth.user_id, &req, &state, request_id).await {
        Ok(r) => r,
        Err(e) => {
            crate::services::alerting::raise_data_alert(
                &state,
                "amas.learning_record",
                "single_process_failed",
                "warning",
                "学习数据上报处理失败".to_string(),
                format!("单条上报失败: {}", e.code),
                None,
            )
            .await;
            return Err(e);
        }
    };
    if result.duplicate {
        Ok(ok(result).into_response())
    } else {
        Ok(created(result).into_response())
    }
}
