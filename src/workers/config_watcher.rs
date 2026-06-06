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
    // #36:监听父目录而非文件本身。inotify 把 watch 绑在 inode 上,而 write_to_toml 用
    // 临时文件 + atomic rename 替换目标,会换 inode 让文件级 watch 失效(首次保存后静默失聪)。
    // 目录 inode 稳定,rename-replace 始终可观测;再按事件路径的文件名过滤回目标文件。
    let canonical_target = std::fs::canonicalize(&watch_path).ok();
    let target_file_name = watch_path.file_name().map(|n| n.to_os_string());
    let watch_dir = watch_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    let (tx, mut rx) = mpsc::channel::<notify::Result<notify::Event>>(8);

    let mut watcher = match RecommendedWatcher::new(
        move |res| {
            // try_send：channel 满时丢弃事件而非阻塞 watcher 线程；
            // 防抖逻辑会在接收端清空积压，多余事件可安全丢弃。
            let _ = tx.try_send(res);
        },
        notify::Config::default(),
    ) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!(error = %e, "创建文件监听器失败，热重载不可用");
            return;
        }
    };

    if let Err(e) = watcher.watch(&watch_dir, RecursiveMode::NonRecursive) {
        tracing::warn!(dir = %watch_dir.display(), error = %e, "监听配置目录失败，热重载不可用");
        return;
    }

    tracing::info!(path = %path, dir = %watch_dir.display(), "AMAS 配置文件监听已启动");

    while let Some(event_result) = rx.recv().await {
        let event = match event_result {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(error = %e, "文件监听事件错误");
                continue;
            }
        };

        // 目录级监听会收到目录内所有文件事件,只放行命中目标文件的:优先比对 canonical 全路径,
        // 回退比对文件名(rename 来源临时文件/目标尚不可 canonicalize 时)。
        let hits_target = event.paths.iter().any(|p| {
            if let (Some(ct), Ok(cp)) = (&canonical_target, std::fs::canonicalize(p)) {
                if &cp == ct {
                    return true;
                }
            }
            match (&target_file_name, p.file_name()) {
                (Some(want), Some(got)) => got == want.as_os_str(),
                _ => false,
            }
        });
        if !hits_target {
            continue;
        }

        let is_modify = matches!(
            event.kind,
            EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
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
                Ok(()) => match amas.reload_config(cfg) {
                    Ok(()) => tracing::info!(path = %path, "AMAS 配置热重载成功"),
                    Err(e) => tracing::warn!(path = %path, error = %e, "AMAS 配置热重载失败"),
                },
                Err(e) => {
                    tracing::warn!(path = %path, error = %e, "AMAS 配置校验失败，已跳过本次重载")
                }
            },
            Err(e) => {
                tracing::warn!(path = %path, error = %e, "读取/解析 TOML 失败，已跳过本次重载")
            }
        }
    }
}
