//! B1:DB 备份离站外迁。每日本地备份成功后，把产物推送到 `backup-policy` 配置的
//! 各 [`BackupTarget`]（file:// / rsync / s3://），并按 target 的 retention_days 做远端 prune。
//!
//! 凭据注入：**S3 走环境变量**（无前端表单，避免凭据落库）。`object_store` 的
//! `AmazonS3Builder::from_env()` 读取 `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` /
//! `AWS_REGION` / `AWS_ENDPOINT` / `AWS_ALLOW_HTTP` 等标准变量；bucket 从 uri 解析。
//! rsync 走 SSH key（运维侧 `~/.ssh` 免密），file:// 无需凭据。详见
//! `docs/runbook/backup-restore.md` 离站章节。
//!
//! 失败处理：任一 target 上传失败 → `record_system_alert("backup_offsite", ...)`（dedup
//! 合并计数），不中断其余 target；本地备份本身不受影响。

use std::path::Path;

use crate::routes::admin::settings_sections::{BackupPolicy, BackupTarget};
use crate::store::Store;

/// 解析后的离站目标类型。
enum Scheme<'a> {
    /// file:///abs/dir —— 拷贝到本地/挂载目录。
    File(&'a str),
    /// rsync:host:/path 或 rsync://host/path —— 外部 rsync 命令推送。
    Rsync(&'a str),
    /// s3://bucket/prefix —— object_store aws。
    S3 { bucket: String, prefix: String },
}

/// 把本地备份文件推送到所有配置的离站目标。无 backup-policy / 无 targets 时静默跳过。
/// 单 target 失败仅告警，不影响其余 target 与本地备份。
pub async fn run(store: &Store, local_backup: &Path) {
    let started = std::time::Instant::now();
    let targets = match load_targets(store) {
        Some(t) if !t.is_empty() => t,
        _ => return, // 未配置离站策略，跳过（无事可做，不发心跳）
    };

    let file_name = match local_backup.file_name().and_then(|n| n.to_str()) {
        Some(n) => n.to_string(),
        None => {
            alert(
                store,
                "本地备份文件名无效",
                &local_backup.display().to_string(),
            );
            return;
        }
    };

    // 备份文件大小，成功上传时记入每 target 状态。
    let bytes = std::fs::metadata(local_backup)
        .map(|m| m.len() as i64)
        .unwrap_or(0);

    let mut failures = 0u32;
    let mut first_error: Option<String> = None;
    for target in &targets {
        if let Err(e) = upload_one(target, local_backup, &file_name).await {
            failures += 1;
            if first_error.is_none() {
                first_error = Some(format!("{}: {e}", target.name));
            }
            tracing::warn!(target = %target.name, uri = %target.uri, error = %e, "离站备份上传失败");
            alert(
                store,
                &format!("离站备份目标 `{}` 上传失败", target.name),
                &format!("uri={} error={e}", target.uri),
            );
            // W2-4:记 target 失败状态(保留上次成功痕迹)。
            let _ = store.mark_backup_target_failed(&target.name, &target.uri, &e);
        } else {
            tracing::info!(target = %target.name, uri = %target.uri, "离站备份上传成功");
            // W2-4:记 target 成功状态(last_ok_at + bytes)，灾备可验证。
            let _ = store.mark_backup_target_ok(&target.name, &target.uri, bytes);
        }
    }

    // W2-4:心跳。离站备份在 main.rs 独立 interval loop 内运行（非 WorkerManager），故必须在
    // run 内部直接 upsert_worker_last_run，否则 admin worker 列表 / Prometheus gauge 看不见它。
    // 注意：worker_last_run.last_outcome 有 CHECK('success','failure','skipped')，用 "failure"。
    let outcome = if failures == 0 { "success" } else { "failure" };
    let _ = store.upsert_worker_last_run(
        "backup_offsite",
        chrono::Utc::now().timestamp(),
        started.elapsed().as_millis() as i64,
        outcome,
        first_error.as_deref(),
    );
}

/// 从 settings_config 的 `backup-policy` section 解析 targets。
fn load_targets(store: &Store) -> Option<Vec<BackupTarget>> {
    let raw = store.get_settings_config("backup-policy").ok().flatten()?;
    let policy: BackupPolicy = serde_json::from_value(raw).ok()?;
    Some(policy.targets)
}

/// 解析 target.uri 的 scheme。无法识别返回错误串。
fn parse_scheme(uri: &str) -> Result<Scheme<'_>, String> {
    if let Some(rest) = uri.strip_prefix("file://") {
        return Ok(Scheme::File(rest));
    }
    if let Some(rest) = uri.strip_prefix("rsync://") {
        return Ok(Scheme::Rsync(rest));
    }
    if let Some(rest) = uri.strip_prefix("rsync:") {
        // rsync:user@host:/path 形态
        return Ok(Scheme::Rsync(rest));
    }
    if let Some(rest) = uri.strip_prefix("s3://") {
        let (bucket, prefix) = match rest.split_once('/') {
            Some((b, p)) => (b.to_string(), p.trim_end_matches('/').to_string()),
            None => (rest.to_string(), String::new()),
        };
        if bucket.is_empty() {
            return Err("s3:// uri 缺少 bucket".into());
        }
        return Ok(Scheme::S3 { bucket, prefix });
    }
    Err(format!("不支持的备份目标 scheme: {uri}"))
}

/// 上传单个 target + 远端 prune。
async fn upload_one(
    target: &BackupTarget,
    local_backup: &Path,
    file_name: &str,
) -> Result<(), String> {
    match parse_scheme(&target.uri)? {
        Scheme::File(dir) => upload_file(dir, local_backup, file_name, target.retention_days),
        Scheme::Rsync(dest) => upload_rsync(dest, local_backup).await,
        Scheme::S3 { bucket, prefix } => {
            upload_s3(
                &bucket,
                &prefix,
                local_backup,
                file_name,
                target.retention_days,
            )
            .await
        }
    }
}

/// file://：拷贝到目标目录 + 按 retention 删旧。
fn upload_file(
    dir: &str,
    local_backup: &Path,
    file_name: &str,
    retention_days: u32,
) -> Result<(), String> {
    let dir = Path::new(dir);
    std::fs::create_dir_all(dir).map_err(|e| format!("创建目标目录失败: {e}"))?;
    let dst = dir.join(file_name);
    std::fs::copy(local_backup, &dst).map_err(|e| format!("拷贝失败: {e}"))?;
    prune_local_dir(dir, retention_days);
    Ok(())
}

/// 删除目标目录内 mtime 早于 retention 窗口的 `backup-daily-*.db`。失败仅 warn。
fn prune_local_dir(dir: &Path, retention_days: u32) {
    let cutoff = std::time::SystemTime::now().checked_sub(std::time::Duration::from_secs(
        retention_days as u64 * 86400,
    ));
    let cutoff = match cutoff {
        Some(c) => c,
        None => return,
    };
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    for entry in rd.filter_map(|e| e.ok()) {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !(name.starts_with("backup-daily-") && name.ends_with(".db")) {
            continue;
        }
        let mtime = entry.metadata().and_then(|m| m.modified()).ok();
        if let Some(mtime) = mtime {
            if mtime < cutoff {
                if let Err(e) = std::fs::remove_file(entry.path()) {
                    tracing::warn!(error = %e, path = ?entry.path(), "离站 file 目标 prune 删除失败");
                }
            }
        }
    }
}

/// rsync：调用系统 rsync 命令推送单文件到 dest（host:/path 形态，目录尾随 `/`）。
/// 远端保留策略由运维侧 rsync server / cron 负责（rsync 单文件推送不做远端 prune）。
async fn upload_rsync(dest: &str, local_backup: &Path) -> Result<(), String> {
    let local = local_backup.to_string_lossy().to_string();
    // dest 末尾补 `/` 表示推入目录
    let dest = if dest.ends_with('/') {
        dest.to_string()
    } else {
        format!("{dest}/")
    };
    // tokio 未开 process feature，rsync 走 spawn_blocking + std::process。
    tokio::task::spawn_blocking(move || {
        let output = std::process::Command::new("rsync")
            .arg("-az")
            .arg("--timeout=120")
            .arg(&local)
            .arg(&dest)
            .output()
            .map_err(|e| format!("rsync 启动失败（请确认已安装 rsync）: {e}"))?;
        if !output.status.success() {
            return Err(format!(
                "rsync 退出码 {:?}: {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("rsync 任务 join 失败: {e}"))?
}

/// s3://：用 object_store 上传 + 按 retention 删除前缀下过期对象。凭据走 env。
async fn upload_s3(
    bucket: &str,
    prefix: &str,
    local_backup: &Path,
    file_name: &str,
    retention_days: u32,
) -> Result<(), String> {
    use futures::StreamExt;
    use object_store::aws::AmazonS3Builder;
    use object_store::{ObjectStore, PutPayload};

    let s3 = AmazonS3Builder::from_env()
        .with_bucket_name(bucket)
        .build()
        .map_err(|e| format!("S3 client 构建失败（检查 AWS_* 环境变量）: {e}"))?;

    let key = if prefix.is_empty() {
        file_name.to_string()
    } else {
        format!("{prefix}/{file_name}")
    };
    let object_path = object_store::path::Path::from(key.as_str());

    let bytes = tokio::fs::read(local_backup)
        .await
        .map_err(|e| format!("读取本地备份失败: {e}"))?;
    s3.put(&object_path, PutPayload::from(bytes))
        .await
        .map_err(|e| format!("S3 上传失败: {e}"))?;

    // 远端 prune：列前缀对象，删 last_modified 早于 retention 的 backup-daily-*.db。
    let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
    let list_prefix = (!prefix.is_empty()).then(|| object_store::path::Path::from(prefix));
    let mut stream = s3.list(list_prefix.as_ref());
    while let Some(meta) = stream.next().await {
        let meta = match meta {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(error = %e, "S3 离站 prune 列举失败");
                break;
            }
        };
        let name = meta.location.filename().unwrap_or("");
        if name.starts_with("backup-daily-") && name.ends_with(".db") && meta.last_modified < cutoff
        {
            if let Err(e) = s3.delete(&meta.location).await {
                tracing::warn!(error = %e, key = %meta.location, "S3 离站 prune 删除失败");
            }
        }
    }
    Ok(())
}

/// 记录离站备份失败告警（dedup by source+kind）。
fn alert(store: &Store, title: &str, message: &str) {
    let _ = store.record_system_alert("backup_offsite", "upload_failed", "error", title, message);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_scheme_file() {
        assert!(matches!(
            parse_scheme("file:///tmp/x"),
            Ok(Scheme::File("/tmp/x"))
        ));
    }

    #[test]
    fn parse_scheme_rsync_variants() {
        assert!(matches!(
            parse_scheme("rsync://host/path"),
            Ok(Scheme::Rsync("host/path"))
        ));
        assert!(matches!(
            parse_scheme("rsync:user@h:/p"),
            Ok(Scheme::Rsync("user@h:/p"))
        ));
    }

    #[test]
    fn parse_scheme_s3_with_prefix() {
        match parse_scheme("s3://my-bucket/backups/wf/") {
            Ok(Scheme::S3 { bucket, prefix }) => {
                assert_eq!(bucket, "my-bucket");
                assert_eq!(prefix, "backups/wf");
            }
            _ => panic!("expected S3 with prefix"),
        }
    }

    #[test]
    fn parse_scheme_s3_no_prefix() {
        match parse_scheme("s3://only-bucket") {
            Ok(Scheme::S3 { bucket, prefix }) => {
                assert_eq!(bucket, "only-bucket");
                assert!(prefix.is_empty());
            }
            _ => panic!("expected S3"),
        }
    }

    #[test]
    fn parse_scheme_rejects_unknown() {
        assert!(parse_scheme("glacier://x").is_err());
        assert!(parse_scheme("s3://").is_err());
    }
}
