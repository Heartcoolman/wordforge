//! m073：app_events 埋点日聚合 worker（每日 01:10，错开 01:00 的 daily_aggregation）。
//!
//! 对「昨天起往前 7 天」逐日幂等重算（DELETE + 重算，见 Store::rollup_app_events_day）——
//! 重算窗口 = 摄取端 clientTsMs 回填钳制窗（7d），离线队列晚到补传的事件最终一致。
//! 当天不聚合，admin 读端点对当日走 raw 表索引扫补齐。失败逐日告警不中断后续天。

use crate::store::Store;

pub async fn run(store: &Store) {
    let store = store.clone();
    match crate::blocking::run_blocking("worker.app_event_rollup", move || {
        let today = chrono::Utc::now().date_naive();
        for offset in 1..=7i64 {
            let day = (today - chrono::Duration::days(offset))
                .format("%Y-%m-%d")
                .to_string();
            if let Err(e) = store.rollup_app_events_day(&day) {
                tracing::warn!(day = %day, error = %e, "app_event_rollup: 日聚合失败");
                let _ = store.record_system_alert(
                    "app_events.rollup",
                    "rollup_failed",
                    "warning",
                    "埋点事件日聚合失败",
                    &format!("day={day}: {e}"),
                );
            }
        }
    })
    .await
    {
        Ok(()) => {}
        Err(e) => tracing::warn!(error = %e, "app_event_rollup task failed"),
    }
}
