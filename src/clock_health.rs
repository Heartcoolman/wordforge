//! 时钟健康探测：用 HTTP Date header (RFC 7231) 对比公网时间，检测本机时钟漂移。
//!
//! ## 为什么需要这个
//!
//! JWT 的 `iat`/`exp` 是基于本机 `Utc::now()` 签发的。若服务器时钟与浏览器实际时间
//! 漂移超过 token TTL，会出现"后端签发即失效"的现象：浏览器拿到 token 后立刻 decode
//! 检查 exp，发现"已过期"（实际是服务器在过去签的），从 storage 删 token、跳登录页。
//!
//! 服务器本机自测时一切正常（同机时钟一致），现象只在跨端时出现 —— 极难排查。
//!
//! 本模块在启动时与周期性运行时探测漂移，超阈则把 `/health` 状态降级为 `degraded`，
//! 并打 ERROR 日志，让运维在被业务问题误诊之前就发现真实根因。
//!
//! ## 为什么用 HTTP Date 而不是 SNTP
//!
//! - **零新依赖**：项目已有 `reqwest`。
//! - **零防火墙问题**：HTTPS/443 几乎任何部署环境都通；NTP 的 UDP/123 在 microVM、
//!   出云防火墙、企业内网中常被拦截。
//! - **精度足够**：RFC 7231 Date header 精度到秒，配合多端点中位数取样可达 ±2s 误差，
//!   远低于 JWT 可容忍的几十秒漂移阈值。

use chrono::{DateTime, Utc};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// 默认探测端点（多 vendor 互不依赖；选稳定且全球可达的公网站点）。
const DEFAULT_PROBE_HOSTS: &[&str] = &[
    "https://www.cloudflare.com",
    "https://www.google.com",
    "https://www.apple.com",
];

/// 单次 HEAD 请求超时。
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// 触发 `/health` degraded 的漂移阈值（秒）。低于此值视为正常 NTP 波动。
pub const DRIFT_DEGRADED_THRESHOLD_SECS: i64 = 60;

/// 周期性探测的间隔（秒）。设 1 小时；漂移在 microVM suspend/resume 后才会突变，
/// 不需要更激进的频率。
pub const PERIODIC_CHECK_INTERVAL_SECS: u64 = 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClockCheckStatus {
    /// 尚未做过探测（启动初）。
    Unknown,
    /// 探测成功，漂移在阈值内。
    Healthy,
    /// 探测成功，漂移超阈。
    Drifted,
    /// 所有端点都失败（无网络或全部被防火墙拦截）。无法判断时钟是否健康。
    Failed,
}

#[derive(Clone)]
pub struct ClockHealth {
    drift_secs: Arc<AtomicI64>,
    last_check_at: Arc<AtomicI64>,
    status: Arc<RwLock<ClockCheckStatus>>,
}

impl ClockHealth {
    pub fn new() -> Self {
        Self {
            drift_secs: Arc::new(AtomicI64::new(0)),
            last_check_at: Arc::new(AtomicI64::new(0)),
            status: Arc::new(RwLock::new(ClockCheckStatus::Unknown)),
        }
    }

    /// 最近一次探测得到的漂移秒数（server - local）。未探测时返回 0。
    pub fn drift_secs(&self) -> i64 {
        self.drift_secs.load(Ordering::Relaxed)
    }

    /// 最近一次探测完成的本机 Unix 时间戳；0 表示从未成功探测。
    pub fn last_check_at(&self) -> i64 {
        self.last_check_at.load(Ordering::Relaxed)
    }

    pub async fn status(&self) -> ClockCheckStatus {
        *self.status.read().await
    }

    pub async fn is_degraded(&self) -> bool {
        matches!(self.status().await, ClockCheckStatus::Drifted)
    }

    /// 执行一轮探测，更新内部状态。
    pub async fn probe_once(&self) -> ClockCheckStatus {
        let drift_opt = probe_drift(DEFAULT_PROBE_HOSTS).await;
        let now = Utc::now().timestamp();
        self.last_check_at.store(now, Ordering::Relaxed);

        let new_status = match drift_opt {
            Some(d) => {
                self.drift_secs.store(d, Ordering::Relaxed);
                if d.abs() > DRIFT_DEGRADED_THRESHOLD_SECS {
                    tracing::error!(
                        drift_secs = d,
                        threshold = DRIFT_DEGRADED_THRESHOLD_SECS,
                        "时钟漂移检测：本机时钟与公网相差 {d}s，超过阈值 {DRIFT_DEGRADED_THRESHOLD_SECS}s。\
                         JWT 签发后在浏览器视角下可能立即失效，导致登录后被踢回登录页。\
                         请立即同步 NTP（如 `sudo ntpdate -u pool.ntp.org`）。"
                    );
                    ClockCheckStatus::Drifted
                } else {
                    tracing::info!(drift_secs = d, "时钟健康检测：正常");
                    ClockCheckStatus::Healthy
                }
            }
            None => {
                tracing::warn!(
                    "时钟健康检测：所有探测端点均失败（无网络或被防火墙拦截）。\
                     时钟健康无法判定，请确保服务器能访问公网或自行确认 NTP 状态。"
                );
                ClockCheckStatus::Failed
            }
        };

        *self.status.write().await = new_status;
        new_status
    }

    /// 起一个 detached task 周期性探测，永不返回。返回的 JoinHandle 用于优雅关停。
    pub fn spawn_periodic(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let me = Arc::clone(self);
        tokio::spawn(async move {
            let mut ticker =
                tokio::time::interval(Duration::from_secs(PERIODIC_CHECK_INTERVAL_SECS));
            // first tick fires immediately — we let main.rs do the bootstrap probe explicitly
            // so the periodic loop's first iteration shouldn't probe immediately again.
            ticker.tick().await;
            loop {
                ticker.tick().await;
                me.probe_once().await;
            }
        })
    }
}

impl Default for ClockHealth {
    fn default() -> Self {
        Self::new()
    }
}

/// 对一组 HTTPS 端点并发做 HEAD 请求，读 Date header，返回中位漂移秒数（server - local）。
/// 单点成功也返回（精度差但仍能识别大漂移）；全失败返回 None。
async fn probe_drift(hosts: &[&str]) -> Option<i64> {
    let client = reqwest::Client::builder()
        .timeout(PROBE_TIMEOUT)
        .build()
        .ok()?;

    let futures = hosts.iter().map(|h| probe_one(&client, h));
    let results = futures::future::join_all(futures).await;

    let mut drifts: Vec<i64> = results.into_iter().flatten().collect();

    if drifts.is_empty() {
        return None;
    }

    drifts.sort();
    Some(drifts[drifts.len() / 2])
}

async fn probe_one(client: &reqwest::Client, url: &str) -> Option<i64> {
    let resp = match client.head(url).send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!(url, error = %e, "时钟探测：单个端点失败");
            return None;
        }
    };
    let date_str = resp.headers().get(reqwest::header::DATE)?.to_str().ok()?;
    let remote = parse_http_date(date_str)?;
    let local = Utc::now().timestamp();
    Some(remote - local)
}

/// 解析 RFC 7231 IMF-fixdate 格式 (e.g. "Sun, 06 Nov 1994 08:49:37 GMT")。
/// 该格式与 RFC 2822 兼容，复用 chrono 的现成解析器。
pub fn parse_http_date(s: &str) -> Option<i64> {
    DateTime::parse_from_rfc2822(s.trim())
        .ok()
        .map(|dt| dt.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rfc7231_imf_fixdate() {
        // RFC 7231 §7.1.1.1 example
        let ts = parse_http_date("Sun, 06 Nov 1994 08:49:37 GMT").unwrap();
        // 1994-11-06T08:49:37Z = 784111777
        assert_eq!(ts, 784_111_777);
    }

    #[test]
    fn parses_with_trailing_whitespace() {
        assert!(parse_http_date("  Sun, 06 Nov 1994 08:49:37 GMT  ").is_some());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_http_date("not a date").is_none());
        assert!(parse_http_date("").is_none());
    }

    #[test]
    fn clock_health_starts_unknown() {
        let h = ClockHealth::new();
        assert_eq!(h.drift_secs(), 0);
        assert_eq!(h.last_check_at(), 0);
    }
}
