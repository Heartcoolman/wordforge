//! S2-1：outbox 异步消费 worker。
//!
//! 轮询 `outbox` 表领取到期领域事件，按 event_type 分发处理；失败按指数退避重排，超
//! `MAX_ATTEMPTS` 移入 `events_dead_letter` 并告警 admin。`record_created` 事件复用
//! [`process_single_record`]，与同步路径**零逻辑分叉**（含 best-effort rollback）。
//!
//! 默认 `RECORDS_OUTBOX_ASYNC=false` 时 outbox 为空，每 tick 仅一次空查询、零影响；
//! 即便后续关闭异步模式，本 worker 仍会排空此前 opt-in 期间残留的事件。

use crate::routes::records::single::{process_single_record, OutboxRecordPayload};
use crate::state::AppState;
use crate::store::operations::outbox::OutboxEvent;

/// 单 tick 最多领取处理的事件数（限制单次时长）。
const BATCH: i64 = 50;
/// 重试上限；达到后移入死信。
const MAX_ATTEMPTS: i64 = 5;

/// 指数退避秒数：2^attempts，上限 300s。
fn backoff_secs(attempts: i64) -> i64 {
    let exp = attempts.clamp(0, 30) as u32;
    2_i64.saturating_pow(exp).min(300)
}

/// 处理一批到期 outbox 事件。供 main.rs 的 interval loop 每 tick 调用。
pub async fn run(state: &AppState) {
    let now = chrono::Utc::now();
    let now_str = now.to_rfc3339();

    let due = match state
        .run_store_task("outbox.claim", {
            let now_str = now_str.clone();
            move |store| store.claim_due_outbox_events(&now_str, BATCH)
        })
        .await
    {
        Ok(Ok(d)) => d,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "outbox claim 失败");
            return;
        }
        Err(e) => {
            tracing::warn!(error = %e, "outbox claim 任务失败");
            return;
        }
    };

    if due.is_empty() {
        return;
    }
    tracing::debug!(count = due.len(), "outbox_processor 处理一批事件");

    for ev in due {
        // BA3b：Outbox 阶段 = 单条 outbox 事件处理耗时（含其内部 process_single_record 的
        // AMAS/Sqlite 子调用，故与那两阶段时长重叠，属预期）。
        let outbox_start = std::time::Instant::now();
        let processed = process_one(&ev, state).await;
        crate::stage_metrics::stage_observe(
            crate::stage_metrics::Stage::Outbox,
            outbox_start.elapsed().as_secs_f64() * 1000.0,
            processed.is_err(),
        );
        match processed {
            Ok(()) => {
                let id = ev.id;
                if let Err(e) = state
                    .run_store_task("outbox.done", move |store| store.delete_outbox_event(id))
                    .await
                {
                    tracing::warn!(error = %e, id, "outbox 完成事件删除任务失败");
                }
            }
            Err(perr) => {
                let permanent = perr.permanent;
                let err = perr.message;
                let attempts = ev.attempts + 1;
                // 永久错误（毒丸）跳过退避直接进死信；暂时性错误重试到上限才进死信。
                if permanent || attempts >= MAX_ATTEMPTS {
                    let ev2 = ev.clone();
                    let err2 = err.clone();
                    let _ = state
                        .run_store_task("outbox.dead_letter", move |store| {
                            store.move_outbox_to_dead_letter(&ev2, attempts, &err2)
                        })
                        .await;
                    let reason = if permanent {
                        "永久错误,跳过退避"
                    } else {
                        "重试耗尽"
                    };
                    tracing::error!(id = ev.id, attempts, permanent, error = %err, "outbox 事件进入死信({reason})");
                    crate::services::alerting::raise_data_alert(
                        state,
                        "amas.outbox",
                        "dead_letter",
                        "error",
                        "领域事件进入死信".to_string(),
                        format!(
                            "event#{} {}（{}）：{}",
                            ev.id,
                            if permanent { "永久失败" } else { "重试耗尽" },
                            attempts,
                            err
                        ),
                        None,
                    )
                    .await;
                } else {
                    let next =
                        (now + chrono::Duration::seconds(backoff_secs(attempts))).to_rfc3339();
                    let id = ev.id;
                    let err2 = err.clone();
                    let _ = state
                        .run_store_task("outbox.retry", move |store| {
                            store.reschedule_outbox_event(id, attempts, &next, &err2)
                        })
                        .await;
                    tracing::warn!(id, attempts, error = %err, "outbox 事件处理失败，已退避重排");
                }
            }
        }
    }
}

/// process_one 的失败分类。`permanent=true` 表示重试无意义——payload 反序列化失败 / 未知
/// event_type 这类毒丸消息，直接进死信，不空跑 5 次指数退避（上限 300s）。`permanent=false`
/// 为暂时性失败（DB 锁 / AMAS 临时失败），仍按退避重试。
struct ProcessError {
    message: String,
    permanent: bool,
}

/// 按 event_type 分发处理一条事件。错误带永久/暂时分类供 run 决定是否退避。
async fn process_one(ev: &OutboxEvent, state: &AppState) -> Result<(), ProcessError> {
    match ev.event_type.as_str() {
        "record_created" => {
            let payload: OutboxRecordPayload =
                serde_json::from_str(&ev.payload).map_err(|e| ProcessError {
                    message: format!("payload 解析失败: {e}"),
                    permanent: true,
                })?;
            let mut req = payload.request;
            // 固化客户端提交时刻，避免落库时间漂移到 worker 处理时刻。
            req.created_at_override = Some(payload.created_at);
            process_single_record(&payload.user_id, &req, state)
                .await
                .map(|_| ())
                .map_err(|e| ProcessError {
                    message: format!("{}: {}", e.code, e.message),
                    permanent: false,
                })
        }
        other => Err(ProcessError {
            message: format!("未知事件类型: {other}"),
            permanent: true,
        }),
    }
}
