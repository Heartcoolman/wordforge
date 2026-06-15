//! m037 AMAS 数据软拦截告警 helper(handler 场景,有 AppState)。
//! 双通道:admin 落 system_alerts(运维监控时间线透出)+ 若有受影响 user 再发应用内通知。
//! 告警是 best-effort——失败仅 warn,绝不反向阻断业务流程(软拦截语义)。
//! worker 场景(仅 &Store)直接调 store.record_system_alert,不走此 helper。

use serde_json::json;

use crate::state::AppState;

/// 发一条数据告警。`affected_user` 有值时额外给该 user 发应用内通知,
/// 用确定性 id(`amas_sync_fail:{user}:{severity}:{小时桶}`)+ INSERT OR IGNORE 做小时级 dedup,
/// 防客户端批量重试把通知刷爆。severity 纳入 dedup 键,使同小时内不同严重度各保留一条——
/// 否则较早的低严重度告警会把后续更严重告警静默吞掉。
pub async fn raise_data_alert(
    state: &AppState,
    source: &'static str,
    kind: &'static str,
    severity: &'static str,
    title: String,
    message: String,
    affected_user: Option<String>,
) {
    // admin 告警:落 system_alerts(dedup by source+kind)。
    {
        let title = title.clone();
        let message = message.clone();
        let res = state
            .run_store_task("alerting.record_system_alert", move |store| {
                store.record_system_alert(source, kind, severity, &title, &message)
            })
            .await;
        match res {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!(error = %e, "record_system_alert failed"),
            Err(e) => tracing::warn!(error = %e, "record_system_alert task failed"),
        }
    }

    // user 通知(仅有 user 上下文):小时桶 dedup。
    if let Some(uid) = affected_user {
        let hour = chrono::Utc::now().format("%Y-%m-%d-%H").to_string();
        let notification = json!({
            "id": format!("amas_sync_fail:{uid}:{severity}:{hour}"),
            "userId": uid,
            "type": "system",
            "title": title,
            "message": message,
            "read": false,
            "createdAt": chrono::Utc::now().to_rfc3339(),
        });
        let res = state
            .run_store_task("alerting.create_notification", move |store| {
                store.create_notification(&notification)
            })
            .await;
        match res {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!(error = %e, "alert notification failed"),
            Err(e) => tracing::warn!(error = %e, "alert notification task failed"),
        }
    }
}
