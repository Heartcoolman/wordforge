//! 自更新核心服务：拉 GitHub Releases latest → sha256 校验 → 原子替换 → fork-exec 自重启。
//!
//! 设计要点：
//! - **缓存**：ETag + cache_ttl_secs 双层；304 命中则只刷新时间戳。
//! - **平台**：仅 Linux x86_64 / aarch64，asset 命名 `wordforge-linux-{arch}.tar.gz` + `.sha256`。
//! - **安全网**：DB backup（VACUUM INTO）→ 旧二进制保留 N=2 → 文件锁防并发 → zip-slip 守门。
//! - **重启**：fork 一个后台启动器，等旧进程退出后再 exec 新二进制，不依赖 systemd。
//!   `env::current_exe()` 在 rename 后仍指向旧 inode，故必须用显式 install_dir.join("wordforge") 路径。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use minisign_verify::{PublicKey, Signature};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use tokio_stream::StreamExt;

use crate::config::UpdateCheckConfig;

const USER_AGENT: &str = "wordforge-updater";
const ASSET_PREFIX: &str = "wordforge-linux-";
const ASSET_SUFFIX_TAR: &str = ".tar.gz";
const ASSET_SUFFIX_SHA: &str = ".tar.gz.sha256";
const ASSET_SUFFIX_SIG: &str = ".tar.gz.minisig";
/// M0-R3：fork-exec 后子进程健康自检超时（秒）
const HEALTH_CHECK_TIMEOUT_SECS: u64 = 60;
/// M0-R4：maintenance 模式持久化 flag 文件名（install_dir 下）。
/// 新进程启动时若发现此文件，说明上次自更新途中崩溃，应立即清理 maintenance。
pub const MAINTENANCE_FLAG: &str = ".maintenance.flag";
const STAGING_DIR: &str = ".update-staging";
const TMP_DIR: &str = ".update-tmp";
const LOCK_FILE: &str = ".update.lock";
const ETAG_FILE: &str = ".update_etag";
const KEEP_OLD_VERSIONS: usize = 2;

fn blocking_io<T, F>(f: F) -> Result<T, UpdaterError>
where
    F: FnOnce() -> Result<T, UpdaterError>,
{
    // 同步执行：updater 的 file IO 全是 KB 级毫秒操作（etag 写盘、tar 解包、
    // 文件锁、清理旧版本），对 axum 调度影响可忽略；而 `block_in_place` 在
    // current_thread runtime（含 `#[tokio::test]` 默认）下直接 panic，
    // 反而把测试搞坏。保留 wrapper 名留作未来批量切换的占位。
    f()
}

/// 整个 service 的错误层级。
#[derive(Debug, thiserror::Error)]
pub enum UpdaterError {
    #[error("network: {0}")]
    Network(#[from] reqwest::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("github api {status}: {body}")]
    Api { status: u16, body: String },
    #[error("rate limited (try again later)")]
    RateLimited,
    #[error("no compatible asset for {arch} in tag {tag}")]
    NoAsset { tag: String, arch: String },
    #[error("sha256 mismatch: expected {expected}, got {actual}")]
    Sha256Mismatch { expected: String, actual: String },
    #[error("downgrade refused: latest {latest} ≤ current {current}")]
    DowngradeRefused { current: String, latest: String },
    #[error("tarball too large: {size} > {max}")]
    TarballTooLarge { size: u64, max: u64 },
    #[error("unsafe path inside tarball: {0}")]
    UnsafePath(String),
    #[error("another update is in progress")]
    Locked,
    #[error("apply rolled back: {0}")]
    RolledBack(String),
    #[error("storage: {0}")]
    Store(#[from] crate::store::StoreError),
    #[error("invalid target version {0}")]
    InvalidTarget(String),
    #[error("config: {0}")]
    Config(String),
    /// M0-R2：minisign 签名校验失败（tarball 可能已被篡改）
    #[error("minisign signature invalid: {0}")]
    SignatureInvalid(String),
}

/// 更新通道：stable 排除 prerelease，beta 包含所有（含 stable 自身）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Beta,
}

impl Channel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Beta => "beta",
        }
    }
}

/// 平台架构 → asset 命名后缀。返回 `Some("x86_64")` / `Some("aarch64")`，其它返回 `None`。
fn current_arch_token() -> Option<&'static str> {
    #[cfg(target_os = "linux")]
    {
        match std::env::consts::ARCH {
            "x86_64" => Some("x86_64"),
            "aarch64" => Some("aarch64"),
            _ => None,
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// 单通道视图：每个 channel 的最新 release 元数据 + 升级判定。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatus {
    pub latest_version: String,
    pub latest_published_at: Option<DateTime<Utc>>,
    pub release_notes: String,
    pub release_url: String,
    pub has_update: bool,
    /// 当前进程是否能用这条 release 自更新：架构匹配 + 找到 tar.gz / sha256 资产对。
    pub can_apply: bool,
}

/// 暴露给前端的版本视图，三个 admin updates API 都返回它。
///
/// v0.6.0-beta.3 起 stable / beta 双通道；后端单次 `/releases?per_page=10`
/// 调用分流出两个 latest，前端 admin 后台同时展示 + 可分别一键升级。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current_version: String,
    pub stable: Option<ChannelStatus>,
    pub beta: Option<ChannelStatus>,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub auto_check_enabled: bool,
    pub allow_downgrade: bool,
}

/// 已解析的最新 release，包含 apply 需要的全部 url 与元数据。
#[derive(Debug, Clone)]
struct CachedRelease {
    tag: String,
    body: String,
    published_at: Option<DateTime<Utc>>,
    html_url: String,
    tarball_url: String,
    sha256_url: String,
    /// M0-R2：minisign 签名文件 URL（.tar.gz.minisig）；旧 release 无此 asset 时为空。
    sig_url: String,
    tarball_size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "phase")]
pub enum UpdatePhase {
    Downloading { downloaded: u64, total: u64 },
    Verifying,
    Extracting,
    BackingUpDb,
    Swapping,
    /// M0-R3：子进程健康自检（等待 /health 返回 200）
    HealthChecking,
    Restarting,
}

/// 进度回调；apply 阶段调用，供 SSE 转发。
pub type ProgressSink = Arc<dyn Fn(UpdatePhase) + Send + Sync>;

#[derive(Default)]
struct UpdaterCache {
    last_checked_at: Option<DateTime<Utc>>,
    last_checked_instant: Option<Instant>,
    /// stable_latest = max semver where prerelease=false
    stable: Option<CachedRelease>,
    /// beta_latest = max semver overall（含 prerelease，即"任何 release 里能拿到的最高"）
    beta: Option<CachedRelease>,
    etag: Option<String>,
}

pub struct Updater {
    api_url: String,
    cache_ttl: std::time::Duration,
    cache: RwLock<UpdaterCache>,
    /// API / metadata 调用：30s 总超时，适合短请求
    client: reqwest::Client,
    /// 下载 release tarball / sha256：仅 per-chunk read timeout，不限总时长，
    /// 适应国内服务器到 GitHub release CDN 的慢链路（实测 22 KB/s × 9MB ≈ 7 min）
    download_client: reqwest::Client,
    github_token: Option<String>,
    allow_downgrade: bool,
    install_dir: PathBuf,
    max_tarball_bytes: u64,
    auto_check_enabled: bool,
    current_tag: String,
    /// v0.5.4：可选 GitHub release 下载镜像前缀（如 `https://gh-proxy.com/`）
    download_mirror_prefix: Option<String>,
}

impl Updater {
    /// 根据 `UpdateCheckConfig` 实例化。`current_tag` 一般传 `env!("GIT_VERSION")`。
    pub fn new(cfg: &UpdateCheckConfig, current_tag: &str) -> Result<Arc<Self>, UpdaterError> {
        let install_dir = match cfg.install_dir.clone() {
            Some(p) => p,
            None => std::env::current_exe()
                .map_err(UpdaterError::Io)?
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| {
                    UpdaterError::Config("current_exe has no parent directory".into())
                })?,
        };
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        // v0.5.4：下载用独立 client，不设总 timeout，只用 connect + per-chunk read
        // timeout，适应国内服务器到 GitHub release CDN 的慢链路。
        let download_client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(std::time::Duration::from_secs(15))
            .read_timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let updater = Self {
            api_url: cfg.api_url.clone(),
            cache_ttl: std::time::Duration::from_secs(cfg.cache_ttl_secs),
            cache: RwLock::new(UpdaterCache::default()),
            client,
            download_client,
            github_token: cfg.github_token.clone(),
            allow_downgrade: cfg.allow_downgrade,
            install_dir,
            max_tarball_bytes: cfg.max_tarball_bytes,
            auto_check_enabled: cfg.worker_enabled,
            current_tag: current_tag.to_string(),
            download_mirror_prefix: cfg.download_mirror_prefix.clone(),
        };

        // 持久化 ETag（断言写盘失败不阻塞构造）
        if let Ok(etag) = std::fs::read_to_string(updater.etag_path()) {
            let trimmed = etag.trim().to_string();
            if !trimmed.is_empty() {
                if let Ok(mut cache) = updater.cache.try_write() {
                    cache.etag = Some(trimmed);
                }
            }
        }

        Ok(Arc::new(updater))
    }

    pub fn install_dir(&self) -> &Path {
        &self.install_dir
    }

    pub fn current_tag(&self) -> &str {
        &self.current_tag
    }

    pub fn auto_check_enabled(&self) -> bool {
        self.auto_check_enabled
    }

    /// 从缓存读出当前状态视图，**不**发起网络请求。
    pub async fn snapshot(&self) -> UpdateStatus {
        let cache = self.cache.read().await;
        self.status_from(&cache)
    }

    /// 普通检查：走 TTL 短路。worker 用这条。
    pub async fn check_latest(&self) -> Result<UpdateStatus, UpdaterError> {
        self.check_inner(false).await
    }

    /// 强制刷新：跳过 TTL，但仍带 ETag 走 304 节省 GitHub 额度。admin "立即检查" 用这条。
    pub async fn force_check_latest(&self) -> Result<UpdateStatus, UpdaterError> {
        self.check_inner(true).await
    }

    async fn check_inner(&self, force: bool) -> Result<UpdateStatus, UpdaterError> {
        // 非 force：TTL 命中 → 直接复用缓存
        if !force {
            let cache = self.cache.read().await;
            if let Some(instant) = cache.last_checked_instant {
                if instant.elapsed() < self.cache_ttl {
                    return Ok(self.status_from(&cache));
                }
            }
        }

        if self.api_url.trim().is_empty() {
            // 显式禁用远端 → 仅返回 current
            let mut cache = self.cache.write().await;
            cache.last_checked_at = Some(Utc::now());
            cache.last_checked_instant = Some(Instant::now());
            return Ok(self.status_from(&cache));
        }

        // Codex P1 (2nd pass): etag 仅在内存 latest 也在时才发；进程重启后
        // .update_etag 被还原但 latest=None，此时若发条件请求拿到 304 就永远
        // 看不到任何版本元数据。v0.6.0-beta.3：cache 拆 stable + beta，
        // 任一有缓存即可带 If-None-Match。
        let etag_now = {
            let cache = self.cache.read().await;
            if cache.stable.is_some() || cache.beta.is_some() {
                cache.etag.clone()
            } else {
                None
            }
        };
        let mut req = self
            .client
            .get(&self.api_url)
            .header("Accept", "application/vnd.github+json");
        if let Some(ref tag) = etag_now {
            req = req.header("If-None-Match", tag);
        }
        if let Some(ref token) = self.github_token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }

        let resp = req.send().await?;
        let status = resp.status();

        if status.as_u16() == 304 {
            // 304 时必须保证 cache 双通道至少一项非空（上面的守门已经保证）。
            // 加一道防御性检查让逻辑更稳：若都空则丢 etag 让下次重新拉。
            let mut cache = self.cache.write().await;
            cache.last_checked_at = Some(Utc::now());
            cache.last_checked_instant = Some(Instant::now());
            if cache.stable.is_none() && cache.beta.is_none() {
                cache.etag = None;
                tracing::warn!(
                    "304 received but both channel caches empty; dropping etag for next call"
                );
            }
            return Ok(self.status_from(&cache));
        }
        if status.as_u16() == 403 || status.as_u16() == 429 {
            return Err(UpdaterError::RateLimited);
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(UpdaterError::Api {
                status: status.as_u16(),
                body,
            });
        }

        let new_etag = resp
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let body: serde_json::Value = resp.json().await?;
        let parsed = parse_release_list_payload(&body);

        let mut cache = self.cache.write().await;
        cache.last_checked_at = Some(Utc::now());
        cache.last_checked_instant = Some(Instant::now());
        if let Some(tag) = new_etag {
            cache.etag = Some(tag.clone());
            // 持久化（写盘失败仅 warn）
            if let Err(e) = blocking_io(|| {
                std::fs::write(self.etag_path(), tag)?;
                Ok(())
            }) {
                tracing::warn!("persist etag: {e}");
            }
        }
        cache.stable = parsed.stable;
        cache.beta = parsed.beta;
        Ok(self.status_from(&cache))
    }

    /// 跑一次完整的自更新流程。`channel` 决定从 stable / beta 哪条缓存读 latest；
    /// `target_tag` 必须等于该通道缓存的 latest_tag，否则 `InvalidTarget`。
    /// 成功后 fork-exec 启动新进程 + exit(0)，**调用方不再返回**。
    ///
    /// M0-R3：`health_url` 是新进程的 `/health` 端点（`http://127.0.0.1:{port}/health`），
    /// fork-exec 后父进程轮询 60 秒；成功则 exit(0)，失败则回滚二进制并调 `on_rollback` 告警。
    ///
    /// M0-R4：`on_maintenance(true/false)` 在 Swapping 时被调用为 true（开维护模式），
    /// failed/rollback 时调 false；成功路径 exit(0) 前不调（新进程启动时靠 flag 文件清理）。
    pub async fn apply<F>(
        &self,
        channel: Channel,
        target_tag: &str,
        backup_callback: F,
        progress: ProgressSink,
        health_url: &str,
        on_rollback: impl Fn(String) + Send + 'static,
        on_maintenance: impl Fn(bool) + Send + 'static,
    ) -> Result<(), UpdaterError>
    where
        F: FnOnce(&Path) -> Result<(), UpdaterError> + Send,
    {
        let latest = {
            let cache = self.cache.read().await;
            let opt = match channel {
                Channel::Stable => cache.stable.clone(),
                Channel::Beta => cache.beta.clone(),
            };
            opt.ok_or_else(|| {
                UpdaterError::InvalidTarget(format!(
                    "no cached release for channel {}; check first",
                    channel.as_str()
                ))
            })?
        };
        if latest.tag != target_tag {
            return Err(UpdaterError::InvalidTarget(format!(
                "channel {} latest cached tag is {}, not {}",
                channel.as_str(),
                latest.tag,
                target_tag
            )));
        }
        // Codex P3: 资产缺失要在 apply 入口就明确报错，否则会在 fetch_sha256 阶段
        // 退化成 generic network error，运维难以定位 release 打包失误
        if latest.tarball_url.is_empty() || latest.sha256_url.is_empty() {
            return Err(UpdaterError::NoAsset {
                tag: latest.tag.clone(),
                arch: current_arch_token().unwrap_or("unknown").to_string(),
            });
        }
        if !self.allow_downgrade && !is_strictly_newer(&latest.tag, &self.current_tag) {
            return Err(UpdaterError::DowngradeRefused {
                current: self.current_tag.clone(),
                latest: latest.tag.clone(),
            });
        }
        if latest.tarball_size > self.max_tarball_bytes {
            return Err(UpdaterError::TarballTooLarge {
                size: latest.tarball_size,
                max: self.max_tarball_bytes,
            });
        }

        let _lock_file = blocking_io(|| {
            std::fs::create_dir_all(&self.install_dir)?;
            let lock_path = self.install_dir.join(LOCK_FILE);
            let lock_file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(false)
                .open(&lock_path)?;
            lock_file
                .try_lock_exclusive()
                .map_err(|_| UpdaterError::Locked)?;
            Ok(lock_file)
        })?;

        let outcome = self
            .apply_locked(
                &latest,
                backup_callback,
                progress.clone(),
                health_url,
                on_rollback,
                on_maintenance,
            )
            .await;
        // 失败时锁随 file drop 自动释放；成功时进程 exit 也会自动释放
        if let Err(ref e) = outcome {
            tracing::error!("auto-update failed: {e}");
        }
        outcome
    }

    async fn apply_locked<F>(
        &self,
        latest: &CachedRelease,
        backup_callback: F,
        progress: ProgressSink,
        health_url: &str,
        on_rollback: impl Fn(String) + Send + 'static,
        on_maintenance: impl Fn(bool) + Send + 'static,
    ) -> Result<(), UpdaterError>
    where
        F: FnOnce(&Path) -> Result<(), UpdaterError> + Send,
    {
        let tmp_dir = self.install_dir.join(TMP_DIR);
        blocking_io(|| {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            std::fs::create_dir_all(&tmp_dir)?;
            Ok(())
        })?;
        let tarball_path = tmp_dir.join(format!(
            "{}{}.tar.gz",
            ASSET_PREFIX,
            current_arch_token().unwrap_or("x86_64")
        ));

        // 1) 下载 sha256
        let sha_expected = self.fetch_sha256(&latest.sha256_url).await?;

        // 2) 流式下载 + 计算 sha256
        progress(UpdatePhase::Downloading {
            downloaded: 0,
            total: latest.tarball_size,
        });
        let actual_sha = self
            .stream_download(
                &latest.tarball_url,
                &tarball_path,
                &progress,
                latest.tarball_size,
            )
            .await?;
        progress(UpdatePhase::Verifying);
        if !actual_sha.eq_ignore_ascii_case(&sha_expected) {
            let _ = blocking_io(|| {
                let _ = std::fs::remove_file(&tarball_path);
                Ok(())
            });
            return Err(UpdaterError::Sha256Mismatch {
                expected: sha_expected,
                actual: actual_sha,
            });
        }

        // 2b) minisign 验签（M0-R2）
        self.verify_minisign_tarball(&latest.sig_url, &tarball_path).await?;

        // 3) 解压到 staging
        progress(UpdatePhase::Extracting);
        let staging_dir = self.install_dir.join(STAGING_DIR);
        let extracted = blocking_io(|| {
            let _ = std::fs::remove_dir_all(&staging_dir);
            std::fs::create_dir_all(&staging_dir)?;
            extract_tar_gz_safe(&tarball_path, &staging_dir)?;
            locate_extracted_root(&staging_dir)
        })?;

        // 4) DB 备份
        progress(UpdatePhase::BackingUpDb);
        let backup_path = self
            .install_dir
            .join("data")
            .join(format!("learning-{}.backup.db", self.current_tag));
        blocking_io(|| {
            if let Some(parent) = backup_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            backup_callback(&backup_path)
        })?;

        // 5) 原子替换二进制和 static/
        // M0-R4：进入 Swapping 前开启 maintenance 模式，写 flag 文件（新进程启动时清理）。
        progress(UpdatePhase::Swapping);
        on_maintenance(true);
        let flag_path = self.install_dir.join(MAINTENANCE_FLAG);
        let _ = blocking_io(|| {
            std::fs::write(&flag_path, b"").map_err(|e| {
                tracing::warn!("写 maintenance flag 失败: {e}");
                UpdaterError::Io(e)
            })
        });
        let bin_path = self.install_dir.join("wordforge");
        let bin_backup = self
            .install_dir
            .join(format!("wordforge.{}", self.current_tag));
        let static_path = self.install_dir.join("static");
        let static_backup = self
            .install_dir
            .join(format!("static.{}", self.current_tag));

        let swap_result = blocking_io(|| {
            let mut steps_done: Vec<UndoStep> = Vec::new();
            if bin_path.exists() {
                std::fs::rename(&bin_path, &bin_backup)?;
                steps_done.push(UndoStep::Rename(bin_backup.clone(), bin_path.clone()));
            }
            if static_path.exists() {
                if let Err(e) = std::fs::rename(&static_path, &static_backup) {
                    rollback(steps_done);
                    return Err(UpdaterError::RolledBack(format!(
                        "rename static failed: {e}"
                    )));
                }
                steps_done.push(UndoStep::Rename(static_backup.clone(), static_path.clone()));
            }

            if let Err(e) = std::fs::rename(extracted.join("wordforge"), &bin_path) {
                rollback(steps_done);
                return Err(UpdaterError::RolledBack(format!(
                    "install new binary failed: {e}"
                )));
            }
            steps_done.push(UndoStep::Remove(bin_path.clone()));
            if let Err(e) = std::fs::rename(extracted.join("static"), &static_path) {
                rollback(steps_done);
                return Err(UpdaterError::RolledBack(format!(
                    "install new static failed: {e}"
                )));
            }

            let _ = std::fs::remove_dir_all(&staging_dir);
            let _ = std::fs::remove_dir_all(&tmp_dir);
            Ok(())
        });
        // M0-R4：swap 失败 → 关闭 maintenance 模式（flag 删除 + 回调）再传播错误
        if let Err(e) = swap_result {
            let _ = std::fs::remove_file(&flag_path);
            on_maintenance(false);
            return Err(e);
        }

        // 旧版本保留数控制
        if let Err(e) = blocking_io(|| self.prune_old_backups("wordforge.").map_err(Into::into)) {
            tracing::warn!("prune binary backups: {e}");
        }
        if let Err(e) = blocking_io(|| self.prune_old_backups("static.").map_err(Into::into)) {
            tracing::warn!("prune static backups: {e}");
        }
        if let Err(e) = blocking_io(|| self.prune_old_db_backups().map_err(Into::into)) {
            tracing::warn!("prune db backups: {e}");
        }

        // 6) fork-exec 自重启
        progress(UpdatePhase::Restarting);
        blocking_io(|| spawn_replacement(&bin_path))?;

        // M0-R3：健康自检。父进程保持存活最多 60s，轮询子进程 /health。
        // 子进程成功响应 200 → 父进程退出；超时未响应 → 回滚二进制并报警。
        progress(UpdatePhase::HealthChecking);
        let health_deadline = tokio::time::Instant::now()
            + std::time::Duration::from_secs(HEALTH_CHECK_TIMEOUT_SECS);
        // 初始等待：给子进程 1s 抢端口 + 初始化完成
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let child_healthy = loop {
            if tokio::time::Instant::now() >= health_deadline {
                break false;
            }
            let ok = self.client
                .get(health_url)
                .timeout(std::time::Duration::from_secs(3))
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false);
            if ok {
                break true;
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        };

        if child_healthy {
            tracing::info!("新进程健康自检通过，父进程退出");
            std::process::exit(0);
        }

        // 子进程启动失败 → 回滚二进制（static 不回滚，因为变更通常向后兼容）
        let alert_msg = format!(
            "自更新回滚：新进程 {} 启动 {}s 内未通过 /health 检查，已还原 {}",
            latest.tag, HEALTH_CHECK_TIMEOUT_SECS, self.current_tag
        );
        tracing::error!("{}", alert_msg);
        blocking_io(|| {
            if bin_backup.exists() {
                std::fs::rename(&bin_backup, &bin_path)?;
                tracing::info!("回滚：已还原 {:?} → {:?}", bin_backup, bin_path);
            }
            // M0-R4：回滚后关闭 maintenance 并删除 flag
            let _ = std::fs::remove_file(&flag_path);
            Ok(())
        })?;
        on_maintenance(false);
        on_rollback(alert_msg.clone());
        Err(UpdaterError::RolledBack(alert_msg))
    }

    /// v0.5.4：对 release.githubusercontent.com / github.com/.../releases/download/
    /// 类 URL 拼接镜像 prefix（如 `https://gh-proxy.com/`）；前缀为空时返回原 URL。
    /// 规则：`<prefix>/<原 url>`，与 gh-proxy.com / ghproxy.net 等镜像约定一致。
    fn mirror(&self, url: &str) -> String {
        match self.download_mirror_prefix.as_deref() {
            Some(p) if !p.is_empty() => format!("{}/{}", p.trim_end_matches('/'), url),
            _ => url.to_string(),
        }
    }

    async fn fetch_sha256(&self, url: &str) -> Result<String, UpdaterError> {
        let mirrored = self.mirror(url);
        let mut req = self.download_client.get(&mirrored);
        if let Some(ref token) = self.github_token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }
        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(UpdaterError::Api {
                status: status.as_u16(),
                body,
            });
        }
        let text = resp.text().await?;
        // 兼容 `sha256  file` 或单 hex 行两种格式
        let hex = text
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string();
        if hex.len() != 64 {
            return Err(UpdaterError::Api {
                status: 422,
                body: format!("malformed sha256 file: {text:?}"),
            });
        }
        Ok(hex)
    }

    /// M0-R2：从 sig_url 下载 .minisig 文件并验签 tarball。
    ///
    /// 决策 O5-a：公钥编译期嵌入（`env!("MINISIGN_PUBKEY")`）。
    /// - 公钥为空（本地开发）→ warn 跳过，不阻断。
    /// - sig_url 为空（旧 release 无签名 asset）→ warn 跳过，不阻断。
    /// - 公钥/签名格式错误或验签失败 → `SignatureInvalid` 错误，阻断 apply。
    async fn verify_minisign_tarball(
        &self,
        sig_url: &str,
        tarball_path: &Path,
    ) -> Result<(), UpdaterError> {
        const PUBKEY_STR: &str = env!("MINISIGN_PUBKEY");
        if PUBKEY_STR.is_empty() {
            tracing::warn!("MINISIGN_PUBKEY 未设置，跳过签名校验（非生产构建）");
            return Ok(());
        }
        if sig_url.is_empty() {
            tracing::warn!("release 无 .minisig asset，跳过签名校验（旧版 release）");
            return Ok(());
        }

        let pk = PublicKey::from_base64(PUBKEY_STR)
            .map_err(|e| UpdaterError::SignatureInvalid(format!("公钥格式非法: {e}")))?;

        let sig_text = {
            let mirrored = self.mirror(sig_url);
            let mut req = self.download_client.get(&mirrored);
            if let Some(ref token) = self.github_token {
                req = req.header("Authorization", format!("Bearer {token}"));
            }
            let resp = req.send().await?;
            let status = resp.status();
            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                return Err(UpdaterError::Api { status: status.as_u16(), body });
            }
            resp.text().await?
        };

        let sig = Signature::decode(&sig_text)
            .map_err(|e| UpdaterError::SignatureInvalid(format!("签名文件解析失败: {e}")))?;

        let tarball_bytes = blocking_io(|| std::fs::read(tarball_path).map_err(Into::into))?;
        pk.verify(&tarball_bytes, &sig, false)
            .map_err(|e| UpdaterError::SignatureInvalid(format!("验签失败: {e}")))?;

        tracing::info!(tag = %sig_url, "minisign 签名验证通过");
        Ok(())
    }

    async fn stream_download(
        &self,
        url: &str,
        dst: &Path,
        progress: &ProgressSink,
        expected_size: u64,
    ) -> Result<String, UpdaterError> {
        let mirrored = self.mirror(url);
        let mut req = self.download_client.get(&mirrored);
        if let Some(ref token) = self.github_token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }
        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(UpdaterError::Api {
                status: status.as_u16(),
                body,
            });
        }

        let mut file = tokio::fs::File::create(dst).await?;
        let mut hasher = Sha256::new();
        let mut stream = resp.bytes_stream();
        let mut downloaded: u64 = 0;
        while let Some(chunk) = stream.next().await {
            let bytes = chunk?;
            downloaded += bytes.len() as u64;
            if downloaded > self.max_tarball_bytes {
                return Err(UpdaterError::TarballTooLarge {
                    size: downloaded,
                    max: self.max_tarball_bytes,
                });
            }
            hasher.update(&bytes);
            file.write_all(&bytes).await?;
            progress(UpdatePhase::Downloading {
                downloaded,
                total: expected_size.max(downloaded),
            });
        }
        file.flush().await?;
        let digest = hasher.finalize();
        Ok(hex::encode(digest))
    }

    fn etag_path(&self) -> PathBuf {
        self.install_dir.join(ETAG_FILE)
    }

    fn prune_old_backups(&self, prefix: &str) -> std::io::Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(&self.install_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with(prefix))
            .collect();
        entries.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
        while entries.len() > KEEP_OLD_VERSIONS {
            let victim = entries.remove(0);
            let p = victim.path();
            let res = if p.is_dir() {
                std::fs::remove_dir_all(&p)
            } else {
                std::fs::remove_file(&p)
            };
            if let Err(e) = res {
                tracing::warn!("remove backup {:?}: {e}", p);
            }
        }
        Ok(())
    }

    fn prune_old_db_backups(&self) -> std::io::Result<()> {
        let data_dir = self.install_dir.join("data");
        if !data_dir.exists() {
            return Ok(());
        }
        let mut entries: Vec<_> = std::fs::read_dir(&data_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                let n = e.file_name();
                let s = n.to_string_lossy();
                s.starts_with("learning-") && s.ends_with(".backup.db")
            })
            .collect();
        entries.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
        while entries.len() > KEEP_OLD_VERSIONS {
            let victim = entries.remove(0);
            if let Err(e) = std::fs::remove_file(victim.path()) {
                tracing::warn!("remove db backup: {e}");
            }
        }
        Ok(())
    }

    fn status_from(&self, cache: &UpdaterCache) -> UpdateStatus {
        let current = &self.current_tag;
        let to_channel = |opt: &Option<CachedRelease>| -> Option<ChannelStatus> {
            opt.as_ref().map(|r| {
                let has_update = is_strictly_newer(&r.tag, current);
                let assets_ok = !r.tarball_url.is_empty() && !r.sha256_url.is_empty();
                ChannelStatus {
                    latest_version: r.tag.clone(),
                    latest_published_at: r.published_at,
                    release_notes: r.body.clone(),
                    release_url: r.html_url.clone(),
                    has_update,
                    can_apply: has_update && assets_ok,
                }
            })
        };
        UpdateStatus {
            current_version: self.current_tag.clone(),
            stable: to_channel(&cache.stable),
            beta: to_channel(&cache.beta),
            last_checked_at: cache.last_checked_at,
            auto_check_enabled: self.auto_check_enabled,
            allow_downgrade: self.allow_downgrade,
        }
    }
}

fn parse_release_payload(body: &serde_json::Value) -> Option<CachedRelease> {
    let tag = body.get("tag_name")?.as_str()?.to_string();
    let html_url = body
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let notes = body
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let published_at = body
        .get("published_at")
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    let arch = current_arch_token()?;
    let want_tar = format!("{ASSET_PREFIX}{arch}{ASSET_SUFFIX_TAR}");
    let want_sha = format!("{ASSET_PREFIX}{arch}{ASSET_SUFFIX_SHA}");
    let want_sig = format!("{ASSET_PREFIX}{arch}{ASSET_SUFFIX_SIG}");

    let mut tar_url = String::new();
    let mut sha_url = String::new();
    let mut sig_url = String::new();
    let mut tar_size: u64 = 0;
    if let Some(assets) = body.get("assets").and_then(|v| v.as_array()) {
        for a in assets {
            let name = a.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            let url = a
                .get("browser_download_url")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if name == want_tar {
                tar_url = url.to_string();
                tar_size = a.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
            } else if name == want_sha {
                sha_url = url.to_string();
            } else if name == want_sig {
                sig_url = url.to_string();
            }
        }
    }

    if tar_url.is_empty() || sha_url.is_empty() {
        // Codex P3: 不阻止缓存（前端仍能看到"有新版本但不可应用"），但留醒目日志
        tracing::warn!(
            tag = %tag,
            arch = %arch,
            has_tarball = !tar_url.is_empty(),
            has_sha256 = !sha_url.is_empty(),
            "release is mispackaged: missing wordforge-linux-{arch}.tar.gz or its .sha256 sibling"
        );
    }

    Some(CachedRelease {
        tag,
        body: notes,
        published_at,
        html_url,
        tarball_url: tar_url,
        sha256_url: sha_url,
        sig_url,
        tarball_size: tar_size,
    })
}

/// 单次 `/releases?per_page=N` 解析结果。
/// - `stable` = max semver where `prerelease=false`
/// - `beta`   = max semver overall（含 prerelease；即"任何 release 里能拿到的最高"，因此 `beta_latest_semver >= stable_latest_semver`）
struct ParsedReleaseList {
    stable: Option<CachedRelease>,
    beta: Option<CachedRelease>,
}

/// 解析 GitHub `/releases?per_page=N` 数组，分别取出 stable / beta 通道的 latest。
///
/// 设计要点：
/// - 输入是 JSON array；若意外传入单 release object 走 fallback 走老逻辑（视作 stable=beta=同一项），
///   保留对早期 `/releases/latest` 端点的兼容
/// - `parse_release_payload` 完成单 release 的字段提取与 asset 匹配（架构 / sha256 / size）
/// - tag 不可 semver 解析的 release 被跳过（不影响其它项）
fn parse_release_list_payload(body: &serde_json::Value) -> ParsedReleaseList {
    let items = match body.as_array() {
        Some(a) => a,
        None => {
            // fallback：单 object 当作单一 release，两通道指向同一项
            let single = parse_release_payload(body);
            return ParsedReleaseList {
                stable: single.clone(),
                beta: single,
            };
        }
    };
    let mut stable_best: Option<(semver::Version, CachedRelease)> = None;
    let mut beta_best: Option<(semver::Version, CachedRelease)> = None;
    for item in items {
        let Some(parsed) = parse_release_payload(item) else {
            continue;
        };
        let ver = match semver::Version::parse(parsed.tag.trim_start_matches('v')) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let is_prerelease = item
            .get("prerelease")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        // beta_latest = max semver overall（含 stable）
        let beta_take = match &beta_best {
            None => true,
            Some((cur, _)) => ver > *cur,
        };
        if beta_take {
            beta_best = Some((ver.clone(), parsed.clone()));
        }
        // stable_latest = max semver where prerelease=false
        if !is_prerelease {
            let stable_take = match &stable_best {
                None => true,
                Some((cur, _)) => ver > *cur,
            };
            if stable_take {
                stable_best = Some((ver, parsed));
            }
        }
    }
    ParsedReleaseList {
        stable: stable_best.map(|(_, r)| r),
        beta: beta_best.map(|(_, r)| r),
    }
}

/// 严格 newer：semver 解析失败时回退到字符串比较，仍要求 latest != current。
fn is_strictly_newer(latest: &str, current: &str) -> bool {
    let l = latest.trim_start_matches('v');
    let c = current.trim_start_matches('v');
    if l == c {
        return false;
    }
    match (semver::Version::parse(l), semver::Version::parse(c)) {
        (Ok(lv), Ok(cv)) => lv > cv,
        _ => l > c,
    }
}

fn extract_tar_gz_safe(tarball: &Path, dst: &Path) -> Result<(), UpdaterError> {
    let f = std::fs::File::open(tarball)?;
    let gz = flate2::read::GzDecoder::new(f);
    let mut archive = tar::Archive::new(gz);
    archive.set_preserve_permissions(true);
    archive.set_overwrite(true);
    let dst_canon = std::fs::canonicalize(dst).unwrap_or_else(|_| dst.to_path_buf());
    for entry_res in archive.entries()? {
        let mut entry = entry_res?;
        let rel = entry.path()?.into_owned();
        // 拒绝绝对路径或包含 .. 的条目（zip-slip 基本款）
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(UpdaterError::UnsafePath(rel.to_string_lossy().into()));
        }
        // 拒绝任何 link / symlink 条目：本工程发布产物只含 regular file + directory；
        // 允许 symlink 会让后续 file 条目通过预置的 symlink 写出 dst 外（Codex P1）。
        match entry.header().entry_type() {
            tar::EntryType::Symlink | tar::EntryType::Link => {
                return Err(UpdaterError::UnsafePath(format!(
                    "{} (link/symlink not allowed)",
                    rel.to_string_lossy()
                )));
            }
            _ => {}
        }
        let out = dst_canon.join(&rel);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        entry.unpack(&out)?;
    }
    Ok(())
}

/// 解压后定位顶层目录（应该唯一）。约定 tar.gz 内根目录名形如 `wordforge-linux-x86_64/`。
fn locate_extracted_root(dst: &Path) -> Result<PathBuf, UpdaterError> {
    for entry in std::fs::read_dir(dst)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            // 必须同时存在 wordforge 二进制和 static 目录才认作合法 root
            if p.join("wordforge").is_file() && p.join("static").is_dir() {
                return Ok(p);
            }
        }
    }
    Err(UpdaterError::RolledBack(
        "extracted tarball missing wordforge+static".into(),
    ))
}

enum UndoStep {
    /// `from → to` 已发生，回滚需要 `to → from`
    Rename(PathBuf, PathBuf),
    /// 新文件 `path` 已就位但后续失败，需要删除
    Remove(PathBuf),
}

fn rollback(mut steps: Vec<UndoStep>) {
    while let Some(step) = steps.pop() {
        match step {
            UndoStep::Rename(from, to) => {
                if let Err(e) = std::fs::rename(&from, &to) {
                    tracing::error!("rollback rename {:?}→{:?}: {e}", from, to);
                }
            }
            UndoStep::Remove(path) => {
                let res = if path.is_dir() {
                    std::fs::remove_dir_all(&path)
                } else {
                    std::fs::remove_file(&path)
                };
                if let Err(e) = res {
                    tracing::error!("rollback remove {:?}: {e}", path);
                }
            }
        }
    }
}

#[cfg(unix)]
fn spawn_replacement(bin_path: &Path) -> Result<(), UpdaterError> {
    use std::os::unix::process::CommandExt;
    let parent_pid = std::process::id().to_string();
    let mut cmd = std::process::Command::new("/bin/sh");
    cmd.arg("-c")
        .arg(
            r#"parent="$1"; shift
while kill -0 "$parent" 2>/dev/null; do
  sleep 0.2
done
exec "$@"
"#,
        )
        .arg("wordforge-restart")
        .arg(parent_pid)
        .arg(bin_path)
        .args(std::env::args().skip(1))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    unsafe {
        cmd.pre_exec(|| {
            // 新建 session，让子进程脱离父 tty 与进程组，父退出不影响子
            nix_setsid();
            Ok(())
        });
    }
    cmd.spawn()?;
    Ok(())
}

#[cfg(not(unix))]
fn spawn_replacement(bin_path: &Path) -> Result<(), UpdaterError> {
    let parent_pid = std::process::id().to_string();
    std::process::Command::new("cmd")
        .arg("/C")
        .arg(format!(
            "ping 127.0.0.1 -n 2 >NUL && start \"\" \"{}\"",
            bin_path.display()
        ))
        .arg(parent_pid)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(())
}

#[cfg(unix)]
fn nix_setsid() {
    // 直接走 libc：避免引入 nix crate
    unsafe {
        libc::setsid();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_strictly_newer_basic() {
        assert!(is_strictly_newer("v0.4.3", "v0.4.2"));
        assert!(is_strictly_newer("v1.0.0", "v0.99.99"));
        assert!(!is_strictly_newer("v0.4.2", "v0.4.2"));
        assert!(!is_strictly_newer("v0.4.1", "v0.4.2"));
    }

    #[test]
    fn is_strictly_newer_no_prefix() {
        assert!(is_strictly_newer("0.4.3", "0.4.2"));
        assert!(is_strictly_newer("v0.4.3", "0.4.2"));
    }

    #[test]
    fn parse_release_payload_finds_assets() {
        // mock arch fallthrough: skip when not linux
        if current_arch_token().is_none() {
            return;
        }
        let arch = current_arch_token().unwrap();
        let body = serde_json::json!({
            "tag_name": "v0.5.0",
            "html_url": "https://example.com/r/v0.5.0",
            "body": "## notes",
            "published_at": "2026-05-17T16:00:00Z",
            "assets": [
                {"name": format!("wordforge-linux-{arch}.tar.gz"), "browser_download_url": "https://example.com/t.tar.gz", "size": 12345},
                {"name": format!("wordforge-linux-{arch}.tar.gz.sha256"), "browser_download_url": "https://example.com/t.sha256", "size": 64},
            ],
        });
        let parsed = parse_release_payload(&body).expect("parsed");
        assert_eq!(parsed.tag, "v0.5.0");
        assert_eq!(parsed.tarball_size, 12345);
        assert!(parsed.tarball_url.ends_with(".tar.gz"));
        assert!(parsed.sha256_url.ends_with(".sha256"));
    }

    /// M0-R2：parse_release_payload 应解析 .minisig asset URL。
    #[test]
    fn parse_release_payload_finds_minisig_asset() {
        if current_arch_token().is_none() {
            return;
        }
        let arch = current_arch_token().unwrap();
        let body = serde_json::json!({
            "tag_name": "v1.0.0",
            "html_url": "https://example.com/r/v1.0.0",
            "body": "",
            "published_at": "2026-05-21T00:00:00Z",
            "assets": [
                {"name": format!("wordforge-linux-{arch}.tar.gz"), "browser_download_url": "https://example.com/t.tar.gz", "size": 9000000},
                {"name": format!("wordforge-linux-{arch}.tar.gz.sha256"), "browser_download_url": "https://example.com/t.sha256", "size": 64},
                {"name": format!("wordforge-linux-{arch}.tar.gz.minisig"), "browser_download_url": "https://example.com/t.minisig", "size": 128},
            ],
        });
        let parsed = parse_release_payload(&body).expect("parsed");
        assert_eq!(parsed.sig_url, "https://example.com/t.minisig");
        // 旧 release 无 .minisig，sig_url 应为空（不阻断 can_apply）
        let body_no_sig = serde_json::json!({
            "tag_name": "v0.9.0",
            "html_url": "https://example.com/r/v0.9.0",
            "body": "",
            "published_at": "2026-05-21T00:00:00Z",
            "assets": [
                {"name": format!("wordforge-linux-{arch}.tar.gz"), "browser_download_url": "https://example.com/t.tar.gz", "size": 9000000},
                {"name": format!("wordforge-linux-{arch}.tar.gz.sha256"), "browser_download_url": "https://example.com/t.sha256", "size": 64},
            ],
        });
        let parsed_no_sig = parse_release_payload(&body_no_sig).expect("parsed");
        assert!(parsed_no_sig.sig_url.is_empty());
    }

    /// v0.5.4 镜像 prefix helper：无 prefix 走原 URL；有 prefix 时拼成
    /// `<prefix>/<原 url>` 形式（gh-proxy 系列镜像约定）。
    #[test]
    fn mirror_helper_handles_prefix_variants() {
        let cfg_no_mirror = UpdateCheckConfig {
            api_url: String::new(),
            cache_ttl_secs: 3600,
            worker_enabled: false,
            worker_interval_secs: 3600,
            github_token: None,
            allow_downgrade: false,
            install_dir: Some(std::env::temp_dir()),
            max_tarball_bytes: 1024,
            download_mirror_prefix: None,
        };
        let u = Updater::new(&cfg_no_mirror, "v0.5.4").expect("build updater");
        assert_eq!(
            u.mirror("https://github.com/o/r/releases/download/v1/foo.tar.gz"),
            "https://github.com/o/r/releases/download/v1/foo.tar.gz"
        );

        let cfg_with_mirror = UpdateCheckConfig {
            download_mirror_prefix: Some("https://gh-proxy.com/".into()),
            ..cfg_no_mirror.clone()
        };
        let u2 = Updater::new(&cfg_with_mirror, "v0.5.4").expect("build updater");
        assert_eq!(
            u2.mirror("https://github.com/o/r/releases/download/v1/foo.tar.gz"),
            "https://gh-proxy.com/https://github.com/o/r/releases/download/v1/foo.tar.gz"
        );

        // 末尾带 / 与不带 / 行为应一致
        let cfg_trailing = UpdateCheckConfig {
            download_mirror_prefix: Some("https://gh-proxy.com".into()),
            ..cfg_no_mirror.clone()
        };
        let u3 = Updater::new(&cfg_trailing, "v0.5.4").expect("build updater");
        assert_eq!(
            u3.mirror("https://github.com/o/r/foo"),
            "https://gh-proxy.com/https://github.com/o/r/foo"
        );

        // 空字符串视同未配置
        let cfg_empty = UpdateCheckConfig {
            download_mirror_prefix: Some(String::new()),
            ..cfg_no_mirror.clone()
        };
        let u4 = Updater::new(&cfg_empty, "v0.5.4").expect("build updater");
        assert_eq!(u4.mirror("https://github.com/o"), "https://github.com/o");
    }

    /// v0.6.0-beta.3 双通道核心：list 输入下分别取 stable / beta latest。
    #[test]
    fn channel_serde_lowercase_roundtrip() {
        let s: Channel = serde_json::from_str("\"stable\"").unwrap();
        assert!(matches!(s, Channel::Stable));
        let b: Channel = serde_json::from_str("\"beta\"").unwrap();
        assert!(matches!(b, Channel::Beta));
        assert_eq!(serde_json::to_string(&Channel::Stable).unwrap(), "\"stable\"");
        assert_eq!(serde_json::to_string(&Channel::Beta).unwrap(), "\"beta\"");
    }

    fn release_json(arch: &str, tag: &str, prerelease: bool) -> serde_json::Value {
        serde_json::json!({
            "tag_name": tag,
            "html_url": format!("https://example.com/r/{tag}"),
            "body": format!("notes {tag}"),
            "published_at": "2026-05-20T00:00:00Z",
            "prerelease": prerelease,
            "assets": [
                {"name": format!("wordforge-linux-{arch}.tar.gz"), "browser_download_url": format!("https://example.com/{tag}.tar.gz"), "size": 1000},
                {"name": format!("wordforge-linux-{arch}.tar.gz.sha256"), "browser_download_url": format!("https://example.com/{tag}.sha256"), "size": 64},
            ],
        })
    }

    #[test]
    fn parse_release_list_picks_max_semver_per_channel() {
        if current_arch_token().is_none() {
            return; // 非 Linux 平台 parse_release_payload 永远返 None；该测试只验 Linux 路径
        }
        let arch = current_arch_token().unwrap();
        let body = serde_json::Value::Array(vec![
            release_json(arch, "v0.6.0-beta.3", true),
            release_json(arch, "v0.6.0-beta.2", true),
            release_json(arch, "v0.5.6", false),
            release_json(arch, "v0.5.5", false),
        ]);
        let parsed = parse_release_list_payload(&body);
        assert_eq!(parsed.stable.as_ref().unwrap().tag, "v0.5.6");
        assert_eq!(parsed.beta.as_ref().unwrap().tag, "v0.6.0-beta.3");
    }

    #[test]
    fn parse_release_list_beta_superset_of_stable_when_only_stable() {
        if current_arch_token().is_none() {
            return;
        }
        let arch = current_arch_token().unwrap();
        let body = serde_json::Value::Array(vec![release_json(arch, "v1.0.0", false)]);
        let parsed = parse_release_list_payload(&body);
        // 只有 stable release 时 beta_latest 也等于 stable_latest（beta 是 overall max）
        assert_eq!(parsed.stable.as_ref().unwrap().tag, "v1.0.0");
        assert_eq!(parsed.beta.as_ref().unwrap().tag, "v1.0.0");
    }

    #[test]
    fn parse_release_list_handles_empty_array() {
        let parsed = parse_release_list_payload(&serde_json::Value::Array(vec![]));
        assert!(parsed.stable.is_none() && parsed.beta.is_none());
    }

    #[test]
    fn parse_release_list_skips_unparsable_tags() {
        if current_arch_token().is_none() {
            return;
        }
        let arch = current_arch_token().unwrap();
        let body = serde_json::Value::Array(vec![
            release_json(arch, "not-a-semver", false),
            release_json(arch, "v0.5.6", false),
        ]);
        let parsed = parse_release_list_payload(&body);
        assert_eq!(parsed.stable.as_ref().unwrap().tag, "v0.5.6");
        assert_eq!(parsed.beta.as_ref().unwrap().tag, "v0.5.6");
    }

    #[test]
    fn parse_release_list_fallback_for_single_object() {
        // 老 /releases/latest 端点返单 object 时仍能 work：stable=beta=same
        if current_arch_token().is_none() {
            return;
        }
        let arch = current_arch_token().unwrap();
        let body = release_json(arch, "v0.5.6", false);
        let parsed = parse_release_list_payload(&body);
        assert_eq!(parsed.stable.as_ref().unwrap().tag, "v0.5.6");
        assert_eq!(parsed.beta.as_ref().unwrap().tag, "v0.5.6");
    }
}
