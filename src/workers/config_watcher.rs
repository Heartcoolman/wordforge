use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::amas::config::AMASConfig;
use crate::amas::engine::AMASEngine;

/// 监听 AMAS 配置 TOML 文件变化，自动热重载。
/// 文件修改后 500ms 防抖，验证通过后调用 engine.reload_config()。
pub async fn run(path: String, amas: Arc<AMASEngine>) {
    let watch_path = PathBuf::from(&path);

    let (tx, mut rx) = mpsc::channel::<notify::Result<notify::Event>>(8);

    let mut watcher = match RecommendedWatcher::new(
        move |res| {
            let _ = tx.blocking_send(res);
        },
        notify::Config::default(),
    ) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!(error = %e, "创建文件监听器失败，热重载不可用");
            return;
        }
    };

    if let Err(e) = watcher.watch(&watch_path, RecursiveMode::NonRecursive) {
        tracing::warn!(path = %path, error = %e, "监听配置文件失败，热重载不可用");
        return;
    }

    tracing::info!(path = %path, "AMAS 配置文件监听已启动");

    while let Some(event_result) = rx.recv().await {
        let event = match event_result {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(error = %e, "文件监听事件错误");
                continue;
            }
        };

        let is_modify = matches!(
            event.kind,
            EventKind::Modify(_) | EventKind::Create(_)
        );
        if !is_modify {
            continue;
        }

        // 500ms 防抖：等待写入完成
        tokio::time::sleep(Duration::from_millis(500)).await;
        // 清空积压事件
        while rx.try_recv().is_ok() {}

        match AMASConfig::load_from_toml(&path) {
            Ok(cfg) => match cfg.validate() {
                Ok(()) => match amas.reload_config(cfg).await {
                    Ok(()) => tracing::info!(path = %path, "AMAS 配置热重载成功"),
                    Err(e) => tracing::warn!(path = %path, error = %e, "AMAS 配置热重载失败"),
                },
                Err(e) => tracing::warn!(path = %path, error = %e, "AMAS 配置校验失败，已跳过本次重载"),
            },
            Err(e) => tracing::warn!(path = %path, error = %e, "读取/解析 TOML 失败，已跳过本次重载"),
        }
    }
}
