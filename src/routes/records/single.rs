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
struct EngineStateSnapshot {
    user_state: Option<serde_json::Value>,
    ige: Option<serde_json::Value>,
    /// 写放大重构：swd 历史已落追加式行表，回滚不再覆写整块 blob，而是删除本事件 append 的那一行。
    /// 捕获时未知（None）；process_event 返回后填入 appended_seq；tx2 失败回滚时删该 seq。
    swd_appended_seq: Option<i64>,
    trust: Option<serde_json::Value>,
    /// ③ 多痕迹（Phase 2）：快照 mastery 键的 (key, 前态) 列表。单痕迹时仅 `mastery:{word}`；
    /// question_mode 已知时额外含 `mastery:{word}:{mode}`，覆盖引擎按 arm flag 可能写入的两种键，
    /// 保证回滚必命中（route 取快照早于 process_event 内部 per-user config 路由，不预知 arm）。
    mastery_states: Vec<(String, Option<serde_json::Value>)>,
    user_elo: crate::amas::elo::EloRating,
    word_elo: crate::amas::elo::EloRating,
    /// #14：抗投毒账本（user,word）净位移快照，与 word_elo 同步捕获/回滚。
    word_elo_contrib: f64,
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
    })
}

fn capture_engine_state_snapshot(
    store: &crate::store::Store,
    user_id: &str,
    word_id: &str,
    question_mode: Option<&str>,
) -> Result<EngineStateSnapshot, AppError> {
    // legacy 键恒快照；question_mode 已知则额外快照 per-mode 键（用 multi_trace=true 派生 suffix）。
    // 不预知本用户落 canary(flag on) 还是 baseline，两键全捕获 → 引擎无论写哪个，回滚都命中。
    let legacy_key = format!("mastery:{word_id}");
    let mut mastery_states = vec![(
        legacy_key.clone(),
        store.get_engine_algo_state(user_id, &legacy_key)?,
    )];
    if let Some(m) = question_mode {
        let mode_key = crate::amas::memory::mastery::mastery_state_key(word_id, Some(m), true);
        if mode_key != legacy_key {
            let prev = store.get_engine_algo_state(user_id, &mode_key)?;
            mastery_states.push((mode_key, prev));
        }
    }

    Ok(EngineStateSnapshot {
        user_state: store.get_engine_user_state(user_id)?,
        ige: store.get_engine_algo_state(user_id, "ige")?,
        // swd 不再在此读整块 blob（写放大重构省一次大读）；回滚句柄在 process_event 后填入。
        swd_appended_seq: None,
        trust: store.get_engine_algo_state(user_id, "trust")?,
        mastery_states,
        user_elo: store.get_user_elo(user_id)?,
        word_elo: store.get_word_elo(word_id)?,
        word_elo_contrib: store.get_word_elo_user_contrib(user_id, word_id)?,
    })
}

/// 回滚单条记录处理对引擎状态的全部改动。W1-1：改为**单 tx 原子**回滚（user_state + algo +
/// ELO），并可选在同一 tx 内清除幂等标记（`clear_marker`）——保证回滚后「标记存在 ⟺ AMAS 已应用」
/// 不变式在崩溃窗口仍成立（详见 `Store::restore_engine_state_atomic`）。手动 rollback 机制保留，
/// 仅由多次独立 store 调用收敛为一次原子调用。
fn restore_engine_state_snapshot(
    store: &crate::store::Store,
    user_id: &str,
    word_id: &str,
    snapshot: &EngineStateSnapshot,
    clear_marker: Option<&str>,
) {
    // ③ 多痕迹：固定算法态(ige/trust) + 1~2 个 mastery 键（legacy + 可选 per-mode）一并原子还原。
    // swd 不再走 algo_states blob，改由 swd_delete_seq 删本事件 append 的那一行（同 tx）。
    let mut algo_states: Vec<(&str, &Option<serde_json::Value>)> = vec![
        ("ige", &snapshot.ige),
        ("trust", &snapshot.trust),
    ];
    for (key, prev) in &snapshot.mastery_states {
        algo_states.push((key.as_str(), prev));
    }
    let restore = crate::store::operations::engine::EngineStateRestore {
        user_id,
        user_state: Some(&snapshot.user_state),
        algo_states: &algo_states,
        user_elo: Some(&snapshot.user_elo),
        word_elo: Some((word_id, &snapshot.word_elo)),
        word_elo_contrib: Some((word_id, snapshot.word_elo_contrib)),
        swd_delete_seq: snapshot.swd_appended_seq,
        clear_marker_record_id: clear_marker,
    };
    if let Err(error) = store.restore_engine_state_atomic(&restore) {
        tracing::warn!(user_id, word_id, error = %error, "原子回滚 AMAS 状态+清标记失败");
    }
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
/// 处理逻辑（含 best-effort rollback），零逻辑分叉。`created_at` 固化客户端提交时刻。
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
                    .create_record_with_updates(&record_for_replay, None, None)
                    .map_err(|e| AppError::internal(&e.to_string()))
            })
            .await??;
        return Ok(CreateRecordResponse {
            record,
            amas_result: None,
            duplicate: true,
        });
    }

    let mut engine_snapshot = state
        .run_store_task("records.single.snapshot", {
            let user_id = user_id_owned.clone();
            let word_id = word_id.clone();
            let question_mode = req.question_mode.clone();
            move |store| {
                capture_engine_state_snapshot(&store, &user_id, &word_id, question_mode.as_deref())
            }
        })
        .await??;

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
                    .create_record_with_updates(&record_for_replay, None, None)
                    .map_err(|e| AppError::internal(&e.to_string()))
            })
            .await??;
        return Ok(CreateRecordResponse {
            record,
            amas_result: None,
            duplicate: true,
        });
    };
    // 回填 swd 回滚句柄：tx2 失败时按此 seq 删本事件 append 的那一行 swd 历史。
    engine_snapshot.swd_appended_seq = swd_appended_seq;
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
                    wls.total_attempts += 1;
                    if req_for_store.is_correct {
                        wls.correct_streak += 1;
                    } else {
                        wls.correct_streak = 0;
                    }
                    if wm.next_review_interval_secs > 0 {
                        wls.next_review_date = Some(
                            Utc::now() + chrono::Duration::seconds(wm.next_review_interval_secs),
                        );
                    }
                    wls.updated_at = Utc::now();
                    next_word_state = Some(wls);
                }

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
                    )
                    .map_err(|error| {
                        // W1-1：原子回滚 AMAS 状态 + 清幂等标记（同一 tx）。崩溃要么全回滚（标记
                        // 已清，重试重新应用 AMAS），要么全不动（标记在、AMAS 仍在，重试走裸记录
                        // 路径），两种结局都守「标记存在 ⟺ AMAS 已应用」不变式，消除原两步非原子
                        // 写之间的崩溃窗口（重试丢 AMAS）。
                        restore_engine_state_snapshot(
                            &store,
                            &user_id_owned,
                            &word_id,
                            &engine_snapshot,
                            Some(&record_for_store.id),
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
    JsonBody(req): JsonBody<CreateRecordRequest>,
) -> Result<axum::response::Response, AppError> {
    // S2-1：opt-in 异步路径（RECORDS_OUTBOX_ASYNC=true）。把创建请求持久化到 outbox 并立即
    // 202 返回，AMAS 处理交 outbox_processor 异步消费；默认 false 走下方同步老路不变。
    // 注意：异步模式响应不含 amas_result（客户端拿不到即时 mastery/复习结果），切换前须与学习端协同。
    // 范围：当前异步路径仅覆盖单条上报；批量 /records/batch 仍恒走同步路径（异步化待后续）。
    // 已知限制(RFC R06)：进程在 AMAS 已持久化、记录未落库之间崩溃时，重启重放会二次累加 AMAS 状态
    //（outbox 保证不丢事件，但未保证不重复）；单事务原子化属后续 cutover，启用本开关前须评估。
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
    let result = match process_single_record(&auth.user_id, &req, &state).await {
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
