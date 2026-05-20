//! 自更新检查 worker：每小时调一次 GitHub Releases，缓存预热 + SSE 广播。
//!
//! 与 LLM advisor 的区别在于不需要存表，只是把"有新版本"这个事实推到前端。
//! v0.6.0-beta.3 起 stable / beta 双通道分别比对 prev / new，各自有更新各自广播。

use std::sync::Arc;

use crate::services::updater::{Channel, ChannelStatus, Updater};
use crate::state::{AppState, SseEvent};

pub async fn run(updater: Arc<Updater>, state: AppState) {
    let prev = updater.snapshot().await;
    let status = match updater.check_latest().await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("update_checker: {e}");
            return;
        }
    };
    broadcast_channel_change(&state, Channel::Stable, &prev.stable, &status.stable);
    broadcast_channel_change(&state, Channel::Beta, &prev.beta, &status.beta);
}

fn broadcast_channel_change(
    state: &AppState,
    channel: Channel,
    prev: &Option<ChannelStatus>,
    new: &Option<ChannelStatus>,
) {
    let Some(new) = new else {
        return;
    };
    if !new.has_update {
        return;
    }
    // 首次拉到 / cached latest 变化 → 广播
    if prev.as_ref().map(|p| p.latest_version.as_str()) != Some(new.latest_version.as_str()) {
        tracing::info!(
            channel = channel.as_str(),
            latest = %new.latest_version,
            "update_checker: announcing new release"
        );
        state.broadcast_to_all_sse(SseEvent::ReleaseAvailable {
            latest_tag: new.latest_version.clone(),
            channel,
        });
    }
}
