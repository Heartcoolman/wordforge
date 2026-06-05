use std::path::{Path, PathBuf};

use crate::store::{Store, StoreError};

/// 执行一次每日备份 + prune（保留最近 keep 个 daily）。返回新备份路径。
pub async fn run_once(
    store: &Store,
    backups_dir: &Path,
    keep: usize,
) -> Result<PathBuf, StoreError> {
    let store = store.clone();
    let dir = backups_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&dir)?;
        let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
        let target = dir.join(format!("backup-daily-{ts}.db"));
        store.backup_to(&target)?;
        prune_daily(&dir, keep);
        Ok(target)
    })
    .await
    .map_err(|e| StoreError::Io(std::io::Error::other(e.to_string())))?
}

/// 删除 `backup-daily-*.db` 中超出 keep 的最旧者（按 mtime）。失败仅 warn。
fn prune_daily(dir: &Path, keep: usize) {
    let mut entries: Vec<(PathBuf, std::time::SystemTime)> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .map(|n| n.starts_with("backup-daily-") && n.ends_with(".db"))
                    .unwrap_or(false)
            })
            .filter_map(|e| {
                let mtime = e.metadata().and_then(|m| m.modified()).ok()?;
                Some((e.path(), mtime))
            })
            .collect(),
        Err(e) => {
            tracing::warn!(error = %e, "每日备份 prune 读取目录失败");
            return;
        }
    };
    if entries.len() <= keep {
        return;
    }
    entries.sort_by_key(|(_, mtime)| *mtime);
    for (path, _) in entries.iter().take(entries.len() - keep) {
        if let Err(e) = std::fs::remove_file(path) {
            tracing::warn!(error = %e, path = ?path, "删除旧每日备份失败");
        }
    }
}
