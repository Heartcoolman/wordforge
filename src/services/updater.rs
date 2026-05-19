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
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use tokio_stream::StreamExt;

use crate::config::UpdateCheckConfig;

const USER_AGENT: &str = "wordforge-updater";
const ASSET_PREFIX: &str = "wordforge-linux-";
const ASSET_SUFFIX_TAR: &str = ".tar.gz";
const ASSET_SUFFIX_SHA: &str = ".tar.gz.sha256";
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

/// 暴露给前端的版本视图，三个 API 都返回它。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub latest_published_at: Option<DateTime<Utc>>,
    pub release_notes: Option<String>,
    pub release_url: Option<String>,
    pub has_update: bool,
    /// 当前进程是否能用这条 release 自更新：架构匹配 + 找到 tar.gz / sha256 资产对。
    pub can_apply: bool,
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
    Restarting,
}

/// 进度回调；apply 阶段调用，供 SSE 转发。
pub type ProgressSink = Arc<dyn Fn(UpdatePhase) + Send + Sync>;

#[derive(Default)]
struct UpdaterCache {
    last_checked_at: Option<DateTime<Utc>>,
    last_checked_instant: Option<Instant>,
    latest: Option<CachedRelease>,
    etag: Option<String>,
}

pub struct Updater {
    api_url: String,
    cache_ttl: std::time::Duration,
    cache: RwLock<UpdaterCache>,
    client: reqwest::Client,
    github_token: Option<String>,
    allow_downgrade: bool,
    install_dir: PathBuf,
    max_tarball_bytes: u64,
    auto_check_enabled: bool,
    current_tag: String,
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

        let updater = Self {
            api_url: cfg.api_url.clone(),
            cache_ttl: std::time::Duration::from_secs(cfg.cache_ttl_secs),
            cache: RwLock::new(UpdaterCache::default()),
            client,
            github_token: cfg.github_token.clone(),
            allow_downgrade: cfg.allow_downgrade,
            install_dir,
            max_tarball_bytes: cfg.max_tarball_bytes,
            auto_check_enabled: cfg.worker_enabled,
            current_tag: current_tag.to_string(),
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
        // 看不到任何版本元数据。
        let etag_now = {
            let cache = self.cache.read().await;
            if cache.latest.is_some() {
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
            // 304 时必须保证 cache.latest 非空（上面的守门已经保证了 etag
            // 仅在 latest 存在时才发），但加一道防御性 assert 让逻辑更稳。
            let mut cache = self.cache.write().await;
            cache.last_checked_at = Some(Utc::now());
            cache.last_checked_instant = Some(Instant::now());
            if cache.latest.is_none() {
                // 理论不可达；万一被外部清空了 cache.latest，丢 etag 让下次重新拉
                cache.etag = None;
                tracing::warn!(
                    "304 received but cache.latest is empty; dropping etag for next call"
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
        let parsed = parse_release_payload(&body);

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
        cache.latest = parsed;
        Ok(self.status_from(&cache))
    }

    /// 跑一次完整的自更新流程。target_tag 必须等于当前缓存的 latest_tag，否则 `InvalidTarget`。
    /// 成功后 fork-exec 启动新进程 + exit(0)，**调用方不再返回**。
    pub async fn apply<F>(
        &self,
        target_tag: &str,
        backup_callback: F,
        progress: ProgressSink,
    ) -> Result<(), UpdaterError>
    where
        F: FnOnce(&Path) -> Result<(), UpdaterError> + Send,
    {
        let latest = {
            let cache = self.cache.read().await;
            cache.latest.clone().ok_or_else(|| {
                UpdaterError::InvalidTarget("no cached release; check first".into())
            })?
        };
        if latest.tag != target_tag {
            return Err(UpdaterError::InvalidTarget(format!(
                "latest cached tag is {}, not {}",
                latest.tag, target_tag
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
            .apply_locked(&latest, backup_callback, progress.clone())
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
        progress(UpdatePhase::Swapping);
        let bin_path = self.install_dir.join("wordforge");
        let bin_backup = self
            .install_dir
            .join(format!("wordforge.{}", self.current_tag));
        let static_path = self.install_dir.join("static");
        let static_backup = self
            .install_dir
            .join(format!("static.{}", self.current_tag));

        blocking_io(|| {
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
        })?;

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
        // 给子进程 500ms 抢端口，让父释放 SocketAddr
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        std::process::exit(0);
    }

    async fn fetch_sha256(&self, url: &str) -> Result<String, UpdaterError> {
        let mut req = self.client.get(url);
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

    async fn stream_download(
        &self,
        url: &str,
        dst: &Path,
        progress: &ProgressSink,
        expected_size: u64,
    ) -> Result<String, UpdaterError> {
        let mut req = self.client.get(url);
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
        let (latest_version, published_at, notes, url, can_apply) = match &cache.latest {
            Some(r) => (
                Some(r.tag.clone()),
                r.published_at,
                Some(r.body.clone()),
                Some(r.html_url.clone()),
                !r.tarball_url.is_empty() && !r.sha256_url.is_empty(),
            ),
            None => (None, None, None, None, false),
        };
        let has_update = latest_version
            .as_ref()
            .map(|tag| is_strictly_newer(tag, &self.current_tag))
            .unwrap_or(false);
        UpdateStatus {
            current_version: self.current_tag.clone(),
            latest_version,
            latest_published_at: published_at,
            release_notes: notes,
            release_url: url,
            has_update,
            can_apply,
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

    let mut tar_url = String::new();
    let mut sha_url = String::new();
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
        tarball_size: tar_size,
    })
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
}
