//! 自更新检查 worker：每小时调一次 GitHub Releases，缓存预热 + SSE 广播。
//!
//! 与 LLM advisor 的区别在于不需要存表，只是把"有新版本"这个事实推到前端。

use std::sync::Arc;

use crate::services::updater::Updater;
use crate::state::{AppState, SseEvent};

pub async fn run(updater: Arc<Updater>, state: AppState) {
    let prev_tag = updater.snapshot().await.latest_version;
    let status = match updater.check_latest().await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("update_checker: {e}");
            return;
        }
    };
    let new_tag = match status.latest_version.as_ref() {
        Some(t) => t.clone(),
        None => return,
    };
    if !status.has_update {
        return;
    }
    // 首次拉到新版本，或缓存中的 latest 改变了 → 广播
    if prev_tag.as_deref() != Some(new_tag.as_str()) {
        tracing::info!(latest = %new_tag, "update_checker: announcing new release");
        state.broadcast_to_all_sse(SseEvent::ReleaseAvailable {
            latest_tag: new_tag,
        });
    }
}
