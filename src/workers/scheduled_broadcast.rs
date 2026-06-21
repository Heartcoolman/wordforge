//! 定时广播下发 worker（m042，D2）。
//!
//! 每 60s 扫 scheduled_broadcasts 表中 status='pending' 且 scheduled_at <= now 的记录,
//! 复用 broadcast.rs 即时 fan-out 逻辑（fan_out_broadcast）做投递,写广播历史,并把
//! 队列行置为 'sent'（含 sent_count）或 'failed'（含 error）。受众过滤后零命中
//! （EMPTY_AUDIENCE）记 failed + system_alert 告警,绝不静默丢弃。
//!
//! 用独立 tokio interval loop 而非 cron worker:fan-out 需 AppState(run_store_task),
//! 与 main.rs 每日备份 / probe_confirm_sweeper 范式一致。

use std::time::Duration;

use tokio::sync::broadcast;

use crate::routes::admin::broadcast::{fan_out_broadcast, record_broadcast_history};
use crate::state::AppState;

/// 扫描间隔:60s（分钟级精度足够,定时广播非秒级实时）。
const SCAN_INTERVAL_SECS: u64 = 60;

/// 单次扫描最多处理的到期记录数（防积压一次性打满)。
const BATCH_LIMIT: i64 = 50;

pub async fn run(state: AppState, mut shutdown_rx: broadcast::Receiver<()>) {
    let mut interval = tokio::time::interval(Duration::from_secs(SCAN_INTERVAL_SECS));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                scan_once(&state).await;
            }
            _ = shutdown_rx.recv() => break,
        }
    }
}

/// 扫一批到期记录并逐条 fan-out。单条失败不影响其余（容错沿用 worker 惯例）。
/// pub 供集成测试单 tick 驱动（避免等 60s interval）。
pub async fn scan_once(state: &AppState) {
    // 步骤0:复活上次扫描崩溃/重启遗留的 'sending' 行（抢占后未及 mark）→ 重置 pending 重发。
    // worker 严格顺序 + leader 单实例,扫描起点的 'sending' 必为陈旧,重置安全(at-least-once)。
    match state
        .run_store_task("scheduled_broadcast.reset_stale", move |store| {
            store.reset_stale_sending_broadcasts()
        })
        .await
    {
        Ok(Ok(n)) if n > 0 => {
            tracing::warn!(reset = n, "scheduled_broadcast: 复活陈旧 sending 行重新下发")
        }
        Ok(Ok(_)) => {}
        Ok(Err(e)) => tracing::warn!(error = %e, "scheduled_broadcast: 重置陈旧 sending 失败"),
        Err(e) => tracing::warn!(error = %e, "scheduled_broadcast: 重置陈旧 sending 任务失败"),
    }

    let now = chrono::Utc::now().to_rfc3339();
    let due = match state
        .run_store_task("scheduled_broadcast.list_due", move |store| {
            store.list_due_scheduled_broadcasts(&now, BATCH_LIMIT)
        })
        .await
    {
        Ok(Ok(rows)) => rows,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "scheduled_broadcast: 查到期记录失败");
            return;
        }
        Err(e) => {
            tracing::warn!(error = %e, "scheduled_broadcast: 查到期记录任务失败");
            return;
        }
    };

    for row in due {
        // 受众:四维全空 → None(全员);否则 Some。
        let audience = if row.platforms.is_empty()
            && row.version_min.is_none()
            && row.last_active_days.is_none()
            && row.user_ids.is_empty()
        {
            None
        } else {
            Some((
                row.platforms.clone(),
                row.version_min.clone(),
                row.last_active_days,
                row.user_ids.clone(),
            ))
        };

        // fan-out 前原子抢占 pending → sending：抢占失败（已被 cancel 或不存在）则跳过、不下发，
        // 闭合「cancel 返回成功但已群发、最终被覆盖回 sent」的 TOCTOU。
        let claim_id = row.id.clone();
        let claimed = match state
            .run_store_task("scheduled_broadcast.claim", move |store| {
                store.claim_scheduled_broadcast_for_send(&claim_id)
            })
            .await
        {
            Ok(Ok(c)) => c,
            Ok(Err(e)) => {
                tracing::warn!(broadcast_id = %row.id, error = %e, "scheduled_broadcast: 抢占失败");
                continue;
            }
            Err(e) => {
                tracing::warn!(broadcast_id = %row.id, error = %e, "scheduled_broadcast: 抢占任务失败");
                continue;
            }
        };
        if !claimed {
            tracing::info!(broadcast_id = %row.id, "scheduled_broadcast: 已被取消，跳过下发");
            continue;
        }

        match fan_out_broadcast(state, &row.id, &row.title, &row.message, audience).await {
            Ok(sent) => {
                record_broadcast_history(
                    state,
                    &row.id,
                    &row.title,
                    &row.message,
                    &row.admin_id,
                    sent,
                )
                .await;
                let id = row.id.clone();
                let sent_at = chrono::Utc::now().to_rfc3339();
                let _ = state
                    .run_store_task("scheduled_broadcast.mark_sent", move |store| {
                        store.mark_scheduled_broadcast_sent(&id, sent as i64, &sent_at)
                    })
                    .await;
                tracing::info!(
                    broadcast_id = %row.id,
                    sent = sent,
                    "scheduled_broadcast: 定时广播已下发"
                );
            }
            Err(e) => {
                // 受众零命中或其它失败:置 failed + 告警,让 admin 在收件箱可见。
                let id = row.id.clone();
                let err_code = e.code.clone();
                let _ = state
                    .run_store_task("scheduled_broadcast.mark_failed", move |store| {
                        store.mark_scheduled_broadcast_failed(&id, &err_code)
                    })
                    .await;
                let store = state.store().clone();
                let title = row.title.clone();
                let code = e.code.clone();
                let msg = e.message.clone();
                // 收件箱告警写入失败也要记日志,避免静默丢失(与本文件其它 store 调用一致)。
                let alert_id = row.id.clone();
                match tokio::task::spawn_blocking(move || {
                    store.record_system_alert(
                        "scheduled_broadcast",
                        "delivery_failed",
                        "warning",
                        &format!("定时广播下发失败: {title}"),
                        &format!("{code}: {msg}"),
                    )
                })
                .await
                {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        tracing::warn!(broadcast_id = %alert_id, error = %e, "scheduled_broadcast: 写入失败告警(收件箱)失败(store)");
                    }
                    Err(e) => {
                        tracing::warn!(broadcast_id = %alert_id, error = %e, "scheduled_broadcast: 写入失败告警(收件箱)任务 panic");
                    }
                }
                tracing::warn!(
                    broadcast_id = %row.id,
                    code = %e.code,
                    error = %e.message,
                    "scheduled_broadcast: 定时广播下发失败"
                );
            }
        }
    }
}
