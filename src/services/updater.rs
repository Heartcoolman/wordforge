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
use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
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
// v1.1.0-beta.3：HEALTH_CHECK_TIMEOUT_SECS 已废弃（M0-R3 死锁修复，watcher 内 hardcoded 60s loop）。
/// M0-P5：每个 apply phase 的独立 watchdog 超时（秒）。
/// 超过此值未推进到下一 phase 则强制 abort + 回滚。
const PHASE_TIMEOUT_SECS: u64 = 300; // 5 分钟
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
    /// M0-P5：phase 超过 watchdog 限制（5 分钟）未推进，强制 abort
    #[error("phase timeout: {phase} 超过 {timeout_secs}s 未完成")]
    PhaseTimeout {
        phase: &'static str,
        timeout_secs: u64,
    },
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
    /// tarball 字节大小（asset `size` 字段）。
    pub tarball_size: u64,
    /// tarball 的 sha256 hex；check 时 best-effort 填充，未拉取到则 None。
    pub sha256: Option<String>,
}

/// 暴露给前端的版本视图，三个 admin updates API 都返回它。
///
/// v0.6.0-beta.3 起 stable / beta 双通道；后端单次 `/releases?per_page=N`
/// 调用分流出两个 latest，前端 admin 后台同时展示 + 可分别一键升级。
/// per_page 需足够大（默认 30），使持续发布 beta 期间「上一个 stable」仍落在窗口内。
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

/// changelog 单条 commit（conventional-commit 分类后）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangelogCommit {
    pub category: String,
    pub scope: Option<String>,
    pub subject: String,
    pub sha: String,
    pub author: String,
}

/// 两个 tag 间的 changelog 汇总（GitHub compare API 结果）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangelogSummary {
    pub base: String,
    pub head: String,
    pub total_commits: u32,
    pub contributors: u32,
    pub category_counts: std::collections::BTreeMap<String, u32>,
    pub commits: Vec<ChangelogCommit>,
    pub compare_url: String,
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
    /// parse 时 None，check / fetch_release_by_tag 阶段 best-effort 拉 .sha256 填充。
    sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "phase")]
pub enum UpdatePhase {
    Downloading {
        downloaded: u64,
        total: u64,
    },
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

/// apply() 的上下文参数包：将 channel / target_tag / health_url / on_rollback /
/// on_maintenance / task_id 6 个参数合并为一个结构体，消除 clippy::too_many_arguments。
/// `task_id` 是 v1.1.0-beta.3 新增：传给 watcher 子进程用于 finalize audit log outcome。
pub struct ApplyContext {
    pub channel: Channel,
    pub target_tag: String,
    pub health_url: String,
    pub on_rollback: Box<dyn Fn(String) + Send + 'static>,
    pub on_maintenance: Box<dyn Fn(bool) + Send + 'static>,
    pub task_id: String,
    /// 真实审计 DB 路径（= 运行时 `config.database_url`）。watcher 子进程据此 finalize
    /// audit outcome（success / rolled_back）。**必须**用运行时实际 DB 路径，不能由
    /// install_dir 推断——否则 watcher 把终态写进错误/空库，升级记录永远停在 in_progress。
    pub audit_db_path: PathBuf,
    /// m022:rollback 专用旁路。设 true 时绕过 `is_strictly_newer` 校验,
    /// 允许把当前版本 swap 成更低的 target_tag。默认 false 即沿用 Updater struct
    /// 的 `allow_downgrade` 全局开关(env 配置)。
    pub allow_downgrade: bool,
}

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

    /// m022:rollback 专用。从 GitHub `/releases/tags/{tag}` 拉单条 release,parse 后
    /// 塞入指定 channel 的 cache slot,使后续 `apply()` 能用该 tag 作为 target。
    /// 注意:覆盖现有 cache 中的 channel latest;rollback 完成后下次 check 会重新拉真正的 latest。
    pub async fn fetch_release_by_tag(
        &self,
        channel: Channel,
        target_tag: &str,
    ) -> Result<(), UpdaterError> {
        if self.api_url.trim().is_empty() {
            return Err(UpdaterError::Config(
                "api_url is empty; cannot fetch release by tag".into(),
            ));
        }
        // 把 list URL("https://api.github.com/.../releases?per_page=N")
        // 改写为 single tag URL("https://api.github.com/.../releases/tags/{tag}")
        let base = self.api_url.split('?').next().unwrap_or(&self.api_url);
        let tag_url = format!("{}/tags/{}", base.trim_end_matches('/'), target_tag);

        let mut req = self
            .client
            .get(&tag_url)
            .header("Accept", "application/vnd.github+json");
        if let Some(ref token) = self.github_token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }

        let resp = req.send().await?;
        let status = resp.status();
        if status.as_u16() == 403 || status.as_u16() == 429 {
            return Err(UpdaterError::RateLimited);
        }
        if status.as_u16() == 404 {
            return Err(UpdaterError::InvalidTarget(format!(
                "GitHub release tag '{}' not found",
                target_tag
            )));
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(UpdaterError::Api {
                status: status.as_u16(),
                body,
            });
        }
        let body: serde_json::Value = resp.json().await?;
        let mut parsed = parse_release_payload(&body).ok_or_else(|| {
            UpdaterError::InvalidTarget(format!(
                "release tag '{}' has no compatible asset for current arch",
                target_tag
            ))
        })?;
        if parsed.tag != target_tag {
            return Err(UpdaterError::InvalidTarget(format!(
                "GitHub returned tag '{}' for query '{}'",
                parsed.tag, target_tag
            )));
        }

        let mut slot = Some(parsed);
        self.fill_release_sha256(&mut slot).await;
        parsed = slot.expect("slot was Some");

        let mut cache = self.cache.write().await;
        match channel {
            Channel::Stable => cache.stable = Some(parsed),
            Channel::Beta => cache.beta = Some(parsed),
        }
        Ok(())
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
        let mut parsed = parse_release_list_payload(&body);

        // best-effort 填充 sha256：仅对有 update 的通道拉 .sha256，失败仅 warn 不阻断。
        self.fill_release_sha256(&mut parsed.stable).await;
        self.fill_release_sha256(&mut parsed.beta).await;

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
        ctx: ApplyContext,
        backup_callback: F,
        progress: ProgressSink,
    ) -> Result<(), UpdaterError>
    where
        F: FnOnce(&Path) -> Result<(), UpdaterError> + Send,
    {
        let ApplyContext {
            channel,
            target_tag,
            health_url,
            on_rollback,
            on_maintenance,
            task_id,
            audit_db_path,
            allow_downgrade: ctx_allow_downgrade,
        } = ctx;
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
        // m022:ApplyContext.allow_downgrade 任一为 true 即放行;rollback handler 走此分支。
        let allow_downgrade = self.allow_downgrade || ctx_allow_downgrade;
        if !allow_downgrade && !is_strictly_newer(&latest.tag, &self.current_tag) {
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
                &health_url,
                on_rollback,
                on_maintenance,
                &task_id,
                &audit_db_path,
            )
            .await;
        // 失败时锁随 file drop 自动释放；成功时进程 exit 也会自动释放
        if let Err(ref e) = outcome {
            tracing::error!("auto-update failed: {e}");
        }
        outcome
    }

    #[allow(clippy::too_many_arguments)]
    async fn apply_locked<F>(
        &self,
        latest: &CachedRelease,
        backup_callback: F,
        progress: ProgressSink,
        health_url: &str,
        _on_rollback: impl Fn(String) + Send + 'static,
        on_maintenance: impl Fn(bool) + Send + 'static,
        task_id: &str,
        audit_db_path: &Path,
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

        // M0-P5：per-phase watchdog 辅助闭包——将 async future 包裹在 PHASE_TIMEOUT_SECS 超时内。
        // 超时返回 PhaseTimeout 而非 Elapsed，确保错误码在前端可读。
        let phase_timeout = std::time::Duration::from_secs(PHASE_TIMEOUT_SECS);

        // 1) 下载 sha256
        let sha_expected =
            tokio::time::timeout(phase_timeout, self.fetch_sha256(&latest.sha256_url))
                .await
                .map_err(|_| UpdaterError::PhaseTimeout {
                    phase: "downloading_sha256",
                    timeout_secs: PHASE_TIMEOUT_SECS,
                })??;

        // 2) 流式下载 + 计算 sha256（M0-P5 watchdog 覆盖整体下载，per-chunk 由 download_client.read_timeout 兜底）
        progress(UpdatePhase::Downloading {
            downloaded: 0,
            total: latest.tarball_size,
        });
        let actual_sha = tokio::time::timeout(
            phase_timeout,
            self.stream_download(
                &latest.tarball_url,
                &tarball_path,
                &progress,
                latest.tarball_size,
            ),
        )
        .await
        .map_err(|_| UpdaterError::PhaseTimeout {
            phase: "downloading",
            timeout_secs: PHASE_TIMEOUT_SECS,
        })??;
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
        tokio::time::timeout(
            phase_timeout,
            self.verify_minisign_tarball(&latest.sig_url, &tarball_path),
        )
        .await
        .map_err(|_| UpdaterError::PhaseTimeout {
            phase: "verifying_signature",
            timeout_secs: PHASE_TIMEOUT_SECS,
        })??;

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
            // v1.1.0-beta.3：rename binary/static 前先清 stale 目标。
            // 上次升级失败回滚后 backup 文件/目录会遗留，下次升级 rename(source, dest)
            // 在 dest 是非空目录时 ENOTEMPTY、在 dest 是 file 时虽然 Linux rename(2)
            // 允许覆盖但为统一两条路径都显式预清理。修 v1.0 → beta.{1,2} 升级反复踩
            // `learning-v1.0.0.backup.db` / `static.v1.0.0` stale 残留陷阱。
            if bin_path.exists() {
                if bin_backup.exists() {
                    let _ = std::fs::remove_file(&bin_backup);
                }
                std::fs::rename(&bin_path, &bin_backup)?;
                steps_done.push(UndoStep::Rename(bin_backup.clone(), bin_path.clone()));
            }
            if static_path.exists() {
                if static_backup.exists() {
                    let _ = std::fs::remove_dir_all(&static_backup);
                }
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

        // 6) v1.1.0-beta.3：fork watcher + parent exit 取代旧 spawn_replacement sh wrapper +
        //    M0-R3 60s 父进程监督的死锁设计。
        //
        // 旧死锁（v1.0 引入 M0-R3 起一直存在，admin 一键升级从未在 v1.0 之后真正成功过）：
        //   - spawn_replacement 的 sh wrapper：`while kill -0 parent; do sleep 0.2; done; exec new_binary`
        //   - parent 在 60s health loop 内一直活着 → sh wrapper 永远 sleep → 新 binary 永远没被 exec
        //   - parent 探针打到自己（旧 v1.0 进程仍 listen）→ maintenance flag 让 /health 返回 503
        //   - 60s 全失败 → parent 走 rollback 路径 return Err 但仍不 exit → sh wrapper 孤儿永生
        //
        // 新设计（hybrid）：
        //   1. parent swap 完成后 fork 一个 watcher 子进程（detached / setsid / stdio→/dev/null）
        //   2. parent std::process::exit(0) → systemd Restart=always 在 RestartSec=5 后启动新 binary
        //   3. watcher 独立 sleep 10s 给 systemd + binary 启动留时间
        //   4. watcher 60s loop 探 /health：
        //      - 通过 → watcher 用 rusqlite 直接 UPDATE audit outcome=success → watcher exit
        //      - 60s 超时 → watcher rename bin/static 回滚 v1.0 + kill 当前 main pid
        //                  → systemd Restart 起 rolled-back v1.0 → watcher UPDATE outcome=rolled_back → exit
        progress(UpdatePhase::Restarting);
        progress(UpdatePhase::HealthChecking);
        let watcher_args = WatcherArgs {
            task_id: task_id.to_string(),
            target_tag: latest.tag.clone(),
            install_dir: self.install_dir.clone(),
            bin_path,
            bin_backup,
            static_path,
            static_backup,
            flag_path,
            health_url: health_url.to_string(),
            audit_db_path: audit_db_path.to_path_buf(),
            #[cfg(unix)]
            parent_pid: unsafe { libc::getpid() },
            #[cfg(not(unix))]
            parent_pid: 0,
        };
        // 标 audit outcome='applied_pending_watcher' 让 admin UI 在 watcher 接管期间显示中间态。
        // watcher 60s 后会把 outcome update 为 success / rolled_back 终态。
        watcher_update_audit_outcome(
            &watcher_args.audit_db_path,
            &watcher_args.task_id,
            "applied_pending_watcher",
            None,
            false, // 不写 completed_at（仍未完成）
        );
        spawn_watcher_then_exit_parent(watcher_args)
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

    /// best-effort 给有 update 的 release 填 sha256：拉 `.sha256` 资产，失败仅 warn。
    async fn fill_release_sha256(&self, slot: &mut Option<CachedRelease>) {
        let Some(r) = slot.as_mut() else { return };
        if r.sha256.is_some()
            || r.sha256_url.is_empty()
            || !is_strictly_newer(&r.tag, &self.current_tag)
        {
            return;
        }
        match self.fetch_sha256(&r.sha256_url).await {
            Ok(hex) => r.sha256 = Some(hex),
            Err(e) => tracing::warn!(tag = %r.tag, "拉 sha256 失败（不阻断）: {e}"),
        }
    }

    /// 从 `api_url` 推导 GitHub repos base（截到 `/releases` 前）。
    /// `https://api.github.com/repos/OWNER/REPO/releases?...` → `https://api.github.com/repos/OWNER/REPO`。
    /// api_url 为空或不含 `/releases` 时返回 None。
    fn repos_api_base(&self) -> Option<String> {
        let url = self.api_url.trim();
        if url.is_empty() {
            return None;
        }
        url.split_once("/releases")
            .map(|(base, _)| base.trim_end_matches('/').to_string())
            .filter(|b| !b.is_empty())
    }

    /// GitHub compare API：`base...head` → 分类后的 commit 列表。
    /// 403/429 → RateLimited；404 / 不可解析 → InvalidTarget；其它非 2xx → Api。
    /// repos base 不可推导（api_url 空）→ Config。
    pub async fn fetch_changelog(
        &self,
        base_tag: &str,
        head_tag: &str,
    ) -> Result<ChangelogSummary, UpdaterError> {
        let repos_base = self.repos_api_base().ok_or_else(|| {
            UpdaterError::Config("api_url 无法推导 repos base；无法拉 changelog".into())
        })?;
        let compare_api = format!("{repos_base}/compare/{base_tag}...{head_tag}");

        let mut req = self
            .client
            .get(&compare_api)
            .header("Accept", "application/vnd.github+json");
        if let Some(ref token) = self.github_token {
            req = req.header("Authorization", format!("Bearer {token}"));
        }
        let resp = req.send().await?;
        let status = resp.status();
        if status.as_u16() == 403 || status.as_u16() == 429 {
            return Err(UpdaterError::RateLimited);
        }
        if status.as_u16() == 404 {
            return Err(UpdaterError::InvalidTarget(format!(
                "compare {base_tag}...{head_tag} not found"
            )));
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(UpdaterError::Api {
                status: status.as_u16(),
                body,
            });
        }
        let body: serde_json::Value = resp.json().await?;

        let raw_commits = body
            .get("commits")
            .and_then(|v| v.as_array())
            .ok_or_else(|| {
                UpdaterError::InvalidTarget(format!(
                    "compare {base_tag}...{head_tag} 响应无 commits 数组"
                ))
            })?;

        let mut commits: Vec<ChangelogCommit> = Vec::with_capacity(raw_commits.len());
        let mut category_counts: std::collections::BTreeMap<String, u32> =
            std::collections::BTreeMap::new();
        let mut authors: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for c in raw_commits {
            let sha = c
                .get("sha")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .chars()
                .take(7)
                .collect::<String>();
            let message = c
                .get("commit")
                .and_then(|cm| cm.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let first_line = message.lines().next().unwrap_or_default().trim();
            // author：优先 GitHub login，否则 commit.author.name。
            let author = c
                .get("author")
                .and_then(|a| a.get("login"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    c.get("commit")
                        .and_then(|cm| cm.get("author"))
                        .and_then(|a| a.get("name"))
                        .and_then(|v| v.as_str())
                })
                .unwrap_or("unknown")
                .to_string();
            if !author.is_empty() {
                authors.insert(author.clone());
            }
            let (category, scope, subject) = classify_commit(first_line);
            *category_counts.entry(category.clone()).or_insert(0) += 1;
            commits.push(ChangelogCommit {
                category,
                scope,
                subject,
                sha,
                author,
            });
        }

        let total_commits = body
            .get("total_commits")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32)
            .unwrap_or(commits.len() as u32);

        // html base：把 `api.github.com/repos` 换成 `github.com`。
        let html_base = repos_base.replacen("api.github.com/repos", "github.com", 1);
        let compare_url = format!("{html_base}/compare/{base_tag}...{head_tag}");

        Ok(ChangelogSummary {
            base: base_tag.to_string(),
            head: head_tag.to_string(),
            total_commits,
            contributors: authors.len() as u32,
            category_counts,
            commits,
            compare_url,
        })
    }

    /// M0-R2：从 sig_url 下载 .minisig 文件并验签 tarball。
    ///
    /// 决策 O5-a：公钥编译期嵌入（`env!("MINISIGN_PUBKEY")`）。
    /// - 公钥为空（本地开发）→ warn 跳过，不阻断。
    /// - 公钥非空（生产构建）+ sig_url 为空 → `SignatureInvalid` 阻断 apply。
    ///   攻击者可能通过控制 GitHub API 响应去掉 .minisig 资产 URL 来绕过验签；
    ///   生产构建必须拒绝无签名的 release（降级攻击防御）。
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
        // 公钥非空 = 生产构建，必须验签。
        // sig_url 空意味着 release assets 列表里没有 .minisig 文件——
        // 可能是攻击者控制 API 响应去掉了该字段，按降级攻击处理，直接阻断。
        if sig_url.is_empty() {
            return Err(UpdaterError::SignatureInvalid(
                "release 无 .minisig asset，疑似降级攻击".into(),
            ));
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
                return Err(UpdaterError::Api {
                    status: status.as_u16(),
                    body,
                });
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
                    tarball_size: r.tarball_size,
                    sha256: r.sha256.clone(),
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
        sha256: None,
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

/// 已知的 conventional-commit category 白名单；其它一律归 `other`。
const KNOWN_CATEGORIES: &[&str] = &[
    "feat", "fix", "perf", "docs", "refactor", "style", "test", "chore", "build", "ci",
];

/// 解析 conventional-commit 头：`type(scope)!: subject`。
/// 等价正则 `^(\w+)(\(([^)]+)\))?!?:\s*(.+)$`。
/// 返回 `(category, scope, subject)`；不匹配 → `("other", None, 原文)`。
fn classify_commit(subject_line: &str) -> (String, Option<String>, String) {
    let none = || ("other".to_string(), None, subject_line.to_string());

    // type = 起始连续 \w（字母/数字/下划线）。
    let type_len = subject_line
        .char_indices()
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '_')
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    if type_len == 0 {
        return none();
    }
    let lower = subject_line[..type_len].to_lowercase();
    // 白名单外 type 整行回退 other（不保留解析出的 scope/subject）。
    if !KNOWN_CATEGORIES.contains(&lower.as_str()) {
        return none();
    }
    let mut rest = &subject_line[type_len..];

    // 可选 scope：`(...)`，内层不含 `)`。
    let mut scope: Option<String> = None;
    if let Some(stripped) = rest.strip_prefix('(') {
        match stripped.find(')') {
            Some(end) if !stripped[..end].contains('(') => {
                scope = Some(stripped[..end].to_string());
                rest = &stripped[end + 1..];
            }
            _ => return none(),
        }
    }

    // 可选 breaking `!`，然后必须是 `:`。
    let rest = rest.strip_prefix('!').unwrap_or(rest);
    let Some(after_colon) = rest.strip_prefix(':') else {
        return none();
    };
    let subject = after_colon.trim_start();
    if subject.is_empty() {
        return none();
    }

    (lower, scope, subject.to_string())
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

// v1.1.0-beta.3：升级监督 watcher 设计 — 替换旧 spawn_replacement / M0-R3 死锁实现。
//
// 旧实现废弃说明：spawn_replacement 用 sh wrapper "等 parent 退出后 exec 新 binary"，
// 但 M0-R3 设计是父进程 60s 监督子进程 /health 后再决定是否 exit。两者死锁导致
// 新 binary 永远没被 exec、admin 一键升级从 v1.0 起从未真正成功过（参见 apply_locked
// 内 v1.1.0-beta.3 注释段的事故复盘）。
//
// 新设计：parent fork watcher 子进程做 60s 监督 + 失败回滚，parent 自己立即 exit，
// 让 systemd Restart=always 在 RestartSec=5 后起新 binary。watcher 与 parent 完全
// 解耦，不存在等待循环。

#[derive(Debug, Clone)]
struct WatcherArgs {
    task_id: String,
    target_tag: String,
    install_dir: PathBuf,
    bin_path: PathBuf,
    bin_backup: PathBuf,
    static_path: PathBuf,
    static_backup: PathBuf,
    flag_path: PathBuf,
    health_url: String,
    audit_db_path: PathBuf,
    /// 当前主进程 PID（fork 前捕获）。watcher 回滚时直接 kill 它让 systemd 拉起回滚后的 binary，
    /// 避免按命令行子串 pgrep 与 install_dir 隐式耦合。
    parent_pid: i32,
}

/// fork watcher 子进程后 parent 立即 exit 让 systemd 接管的工具方法。
/// 不返回。child 走 run_watcher 后 exit；parent 直接 exit。
/// fork 失败时 fallback 到 parent 直接 exit（失去自动回滚但 systemd 仍会重启起新 binary）。
#[cfg(unix)]
fn spawn_watcher_then_exit_parent(args: WatcherArgs) -> ! {
    use std::io::Write;
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();

    let pid = unsafe { libc::fork() };
    if pid < 0 {
        let errno = std::io::Error::last_os_error();
        tracing::error!(
            "fork watcher 失败 ({errno})；parent 直接 exit 让 systemd 接管，本次升级无 60s 自动回滚监督"
        );
        std::process::exit(0);
    } else if pid == 0 {
        // child = watcher
        unsafe {
            libc::setsid();
            // 关 stdio：避免 watcher 的输出污染 systemd journal 接管的新主进程
            let null = libc::open(c"/dev/null".as_ptr(), libc::O_RDWR);
            if null >= 0 {
                libc::dup2(null, 0);
                libc::dup2(null, 1);
                libc::dup2(null, 2);
                libc::close(null);
            }
        }
        run_watcher(&args);
        std::process::exit(0);
    } else {
        tracing::info!(
            watcher_pid = pid,
            "watcher 子进程已 fork，parent exit 让 systemd Restart=always 启动新 binary"
        );
        std::process::exit(0);
    }
}

#[cfg(not(unix))]
fn spawn_watcher_then_exit_parent(_args: WatcherArgs) -> ! {
    tracing::error!("非 unix 平台不支持 fork-based watcher，parent exit，无自动监督");
    std::process::exit(0);
}

/// watcher 子进程主循环：
/// 1. sleep 10s 给 systemd RestartSec=5 + 新 binary startup（migration/bind）共留 10s
/// 2. 60s loop 探 /health（每 2s 一次）
///    - 通过 → 更新 audit outcome=success
///    - 超时 → rollback binary/static + kill 当前主进程让 systemd 起 rolled-back binary
///            + 更新 audit outcome=rolled_back
#[allow(clippy::doc_overindented_list_items)]
fn run_watcher(args: &WatcherArgs) {
    std::thread::sleep(std::time::Duration::from_secs(10));

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while std::time::Instant::now() < deadline {
        if watcher_probe_health(&args.health_url) {
            watcher_update_audit_outcome(&args.audit_db_path, &args.task_id, "success", None, true);
            return;
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    let err_msg = format!(
        "apply rolled back by watcher: 新进程 {} 启动 60s 内 {} 未通过 /health 检查",
        args.target_tag, args.health_url
    );
    watcher_rollback(args);
    watcher_update_audit_outcome(
        &args.audit_db_path,
        &args.task_id,
        "rolled_back",
        Some(&err_msg),
        true,
    );
}

/// 用 curl 探 /health：避免 fork 后 tokio runtime 状态损坏的 reqwest 风险，也避免
/// 新增 ureq 依赖。Linux 标准发行版均有 curl。
fn watcher_probe_health(url: &str) -> bool {
    std::process::Command::new("curl")
        .args(["-fsS", "--max-time", "3", "-o", "/dev/null", url])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// 回滚 binary + static + 清 maintenance flag + kill 当前 wordforge 主进程
/// 让 systemd Restart=always 接管起 rolled-back binary。
fn watcher_rollback(args: &WatcherArgs) {
    // 1. binary：失败版本标 .failed 保留（forensics），备份 → 现役
    let failed_bin = args
        .install_dir
        .join(format!("wordforge.{}.failed", args.target_tag));
    let _ = std::fs::remove_file(&failed_bin);
    let _ = std::fs::rename(&args.bin_path, &failed_bin);
    if args.bin_backup.exists() {
        let _ = std::fs::rename(&args.bin_backup, &args.bin_path);
    }

    // 2. static：失败版本标 .failed 保留，备份 → 现役
    let failed_static = args
        .install_dir
        .join(format!("static.{}.failed", args.target_tag));
    let _ = std::fs::remove_dir_all(&failed_static);
    let _ = std::fs::rename(&args.static_path, &failed_static);
    if args.static_backup.exists() {
        let _ = std::fs::rename(&args.static_backup, &args.static_path);
    }

    // 3. 清 maintenance flag（M0-R4：新进程启动时也会清，双保险）
    let _ = std::fs::remove_file(&args.flag_path);

    // 4. kill fork 前捕获的主进程 PID，让 systemd Restart=always 起 rolled-back
    #[cfg(unix)]
    if args.parent_pid > 0 {
        unsafe {
            libc::kill(args.parent_pid, libc::SIGTERM);
        }
    }
}

/// rusqlite 直接 UPDATE audit_log。失败静默：watcher 不应因写 audit_log 失败导致回滚流程失败。
fn watcher_update_audit_outcome(
    db_path: &Path,
    task_id: &str,
    outcome: &str,
    error: Option<&str>,
    write_completed_at: bool,
) {
    let conn = match rusqlite::Connection::open(db_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                "watcher 打开 audit_log db 失败 path={:?} task_id={} outcome={}: {}",
                db_path,
                task_id,
                outcome,
                e
            );
            return;
        }
    };
    let now = chrono::Utc::now().to_rfc3339();
    let result = if write_completed_at {
        conn.execute(
            "UPDATE update_audit_log SET outcome = ?1, error = ?2, completed_at = ?3 WHERE id = ?4",
            rusqlite::params![outcome, error.unwrap_or(""), now, task_id],
        )
    } else {
        conn.execute(
            "UPDATE update_audit_log SET outcome = ?1, error = ?2 WHERE id = ?3",
            rusqlite::params![outcome, error.unwrap_or(""), task_id],
        )
    };
    if let Err(e) = result {
        tracing::warn!(
            "watcher 写 audit_log 失败 task_id={} outcome={}: {}",
            task_id,
            outcome,
            e
        );
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
        assert_eq!(
            serde_json::to_string(&Channel::Stable).unwrap(),
            "\"stable\""
        );
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
    fn watcher_update_audit_outcome_writes_to_given_db() {
        // 回归：watcher 必须写入「传入的」db_path（= 运行时 database_url），而非由 install_dir 推断；
        // 否则升级终态写进错误/空库，真实记录永远停在 in_progress（升级历史全显示「进行中」）。
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("audit.db");
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute_batch(
                "CREATE TABLE update_audit_log (id TEXT PRIMARY KEY, outcome TEXT, error TEXT, completed_at TEXT);\
                 INSERT INTO update_audit_log (id, outcome) VALUES ('t-1', 'in_progress');",
            )
            .unwrap();
        }
        watcher_update_audit_outcome(&db, "t-1", "success", None, true);
        let conn = rusqlite::Connection::open(&db).unwrap();
        let (outcome, has_completed): (String, bool) = conn
            .query_row(
                "SELECT outcome, completed_at IS NOT NULL FROM update_audit_log WHERE id='t-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(outcome, "success");
        assert!(has_completed);
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

    /// M0-P5：PhaseTimeout 错误消息格式符合前端预期（含 phase 名与秒数）。
    #[test]
    fn phase_timeout_error_message_format() {
        let e = UpdaterError::PhaseTimeout {
            phase: "downloading",
            timeout_secs: 300,
        };
        let msg = e.to_string();
        assert!(msg.contains("downloading"), "消息应包含 phase 名：{msg}");
        assert!(msg.contains("300"), "消息应包含超时秒数：{msg}");
    }

    /// M0-P5：tokio::time::timeout 超时后映射为 PhaseTimeout（使用即时超时模拟慢 future）。
    #[tokio::test]
    async fn phase_timeout_wraps_elapsed() {
        let result: Result<(), UpdaterError> =
            tokio::time::timeout(std::time::Duration::from_millis(1), async {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                Ok(())
            })
            .await
            .map_err(|_| UpdaterError::PhaseTimeout {
                phase: "test_phase",
                timeout_secs: 0,
            })
            .and_then(|r| r);
        assert!(
            matches!(
                result,
                Err(UpdaterError::PhaseTimeout {
                    phase: "test_phase",
                    ..
                })
            ),
            "超时应映射为 PhaseTimeout"
        );
    }

    /// M0-R2 降级攻击防御：生产构建（PUBKEY 非空）+ sig_url 空 → 必须返回 SignatureInvalid。
    /// 注意：此测试在 CI 生产构建（MINISIGN_PUBKEY 非空）下才触发阻断路径；
    /// 本地 dev 构建 PUBKEY 为空时两种分支都走"跳过"路径，验证略有不同。
    #[tokio::test]
    async fn verify_minisign_rejects_missing_signature_in_production() {
        let cfg = UpdateCheckConfig {
            api_url: String::new(),
            cache_ttl_secs: 3600,
            worker_enabled: false,
            worker_interval_secs: 3600,
            github_token: None,
            allow_downgrade: false,
            install_dir: Some(std::env::temp_dir()),
            max_tarball_bytes: 1024 * 1024 * 100,
            download_mirror_prefix: None,
        };
        let updater = Updater::new(&cfg, "v0.0.0").expect("build updater");

        // 用一个不存在的临时文件路径——只需走到 sig_url 判断，不会真正读文件
        let dummy_path = std::env::temp_dir().join("dummy_tarball_nonexistent.tar.gz");

        const PUBKEY_STR: &str = env!("MINISIGN_PUBKEY");
        let result = updater.verify_minisign_tarball("", &dummy_path).await;

        if PUBKEY_STR.is_empty() {
            // 本地开发：公钥为空，跳过验签，返回 Ok
            assert!(result.is_ok(), "本地开发构建应跳过验签");
        } else {
            // 生产构建：公钥非空 + sig_url 空 → 必须阻断
            assert!(
                matches!(result, Err(UpdaterError::SignatureInvalid(_))),
                "生产构建应拒绝无签名 release，实际结果：{result:?}"
            );
        }
    }

    /// M0-R2：验签层拒绝篡改 tarball。
    ///
    /// 使用 minisign-verify crate 文档中的标准测试密钥对（对 b"test" 签名），
    /// 直接调用底层 pk.verify()，验证签名与文件内容不匹配时返回错误。
    /// 这是 verify_minisign_tarball() 内部 pk.verify() 调用的单元测试。
    #[test]
    fn verify_minisign_rejects_tampered_tarball() {
        use minisign_verify::{PublicKey, Signature};

        // 来自 minisign-verify crate 文档的标准测试密钥对，对 b"test" 签名
        let pubkey_b64 = "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
        let sig_text = "untrusted comment: signature from minisign secret key\n\
RUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/\
z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\n\
trusted comment: timestamp:1633700835\tfile:test\tprehashed\n\
wLMDjy9FLAuxZ3q4NlEvkgtyhrr0gtTu6KC4KBJdITbbOeAi1zBIYo0v4iTgt8jJpIidRJnp94ABQkJAgAooBQ==";

        let pk = PublicKey::from_base64(pubkey_b64).expect("测试公钥应有效");
        let sig = Signature::decode(sig_text).expect("测试签名应可解析");

        // 原始内容通过验签
        let original: &[u8] = b"test";
        assert!(
            pk.verify(original, &sig, false).is_ok(),
            "原始内容应通过验签"
        );

        // 篡改后的 tarball 内容必须被拒绝
        let tampered: &[u8] = b"tampered content - simulates in-flight modification of tarball";
        let result = pk.verify(tampered, &sig, false);
        assert!(
            result.is_err(),
            "篡改后的 tarball 内容应被 minisign 验签拒绝，实际：{result:?}"
        );
    }

    #[test]
    fn classify_commit_conventional_prefixes() {
        assert_eq!(
            classify_commit("feat(amas): 新增甜甜圈"),
            ("feat".into(), Some("amas".into()), "新增甜甜圈".into())
        );
        assert_eq!(
            classify_commit("fix: 修边界 bug"),
            ("fix".into(), None, "修边界 bug".into())
        );
        // breaking `!` 与大写 type 归一化
        assert_eq!(
            classify_commit("FEAT(api)!: drop v1"),
            ("feat".into(), Some("api".into()), "drop v1".into())
        );
        // 白名单外 type → other
        assert_eq!(
            classify_commit("wip(x): 半成品"),
            ("other".into(), None, "wip(x): 半成品".into())
        );
        // 无 conventional 头 → other + 原文
        assert_eq!(
            classify_commit("随手一改"),
            ("other".into(), None, "随手一改".into())
        );
        // 缺 subject → other
        assert_eq!(
            classify_commit("docs:"),
            ("other".into(), None, "docs:".into())
        );
    }

    #[test]
    fn repos_api_base_derivation() {
        let cfg = UpdateCheckConfig {
            api_url: "https://api.github.com/repos/Heartcoolman/wordforge/releases?per_page=10"
                .into(),
            cache_ttl_secs: 3600,
            worker_enabled: false,
            worker_interval_secs: 3600,
            github_token: None,
            allow_downgrade: false,
            install_dir: Some(std::env::temp_dir()),
            max_tarball_bytes: 1024,
            download_mirror_prefix: None,
        };
        let u = Updater::new(&cfg, "v1.0.0").expect("build updater");
        assert_eq!(
            u.repos_api_base().as_deref(),
            Some("https://api.github.com/repos/Heartcoolman/wordforge")
        );

        let cfg_empty = UpdateCheckConfig {
            api_url: String::new(),
            ..cfg.clone()
        };
        let u2 = Updater::new(&cfg_empty, "v1.0.0").expect("build updater");
        assert!(u2.repos_api_base().is_none());
    }
}
