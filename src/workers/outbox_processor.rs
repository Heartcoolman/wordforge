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
        match process_one(&ev, state).await {
            Ok(()) => {
                let id = ev.id;
                if let Err(e) = state
                    .run_store_task("outbox.done", move |store| store.delete_outbox_event(id))
                    .await
                {
                    tracing::warn!(error = %e, id, "outbox 完成事件删除任务失败");
                }
            }
            Err(err) => {
                let attempts = ev.attempts + 1;
                if attempts >= MAX_ATTEMPTS {
                    let ev2 = ev.clone();
                    let err2 = err.clone();
                    let _ = state
                        .run_store_task("outbox.dead_letter", move |store| {
                            store.move_outbox_to_dead_letter(&ev2, attempts, &err2)
                        })
                        .await;
                    tracing::error!(id = ev.id, attempts, error = %err, "outbox 事件进入死信");
                    crate::services::alerting::raise_data_alert(
                        state,
                        "amas.outbox",
                        "dead_letter",
                        "error",
                        "领域事件进入死信".to_string(),
                        format!("event#{} 重试 {} 次仍失败：{}", ev.id, attempts, err),
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

/// 按 event_type 分发处理一条事件。错误转 String 供死信 / last_error 记录。
async fn process_one(ev: &OutboxEvent, state: &AppState) -> Result<(), String> {
    match ev.event_type.as_str() {
        "record_created" => {
            let payload: OutboxRecordPayload =
                serde_json::from_str(&ev.payload).map_err(|e| format!("payload 解析失败: {e}"))?;
            let mut req = payload.request;
            // 固化客户端提交时刻，避免落库时间漂移到 worker 处理时刻。
            req.created_at_override = Some(payload.created_at);
            process_single_record(&payload.user_id, &req, state)
                .await
                .map(|_| ())
                .map_err(|e| format!("{}: {}", e.code, e.message))
        }
        other => Err(format!("未知事件类型: {other}")),
    }
}
