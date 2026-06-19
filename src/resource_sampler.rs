//! BA3a：进程内资源历史采样器（CPU / RSS / load avg 时序）。
//!
//! 一个有界 VecDeque 环形缓冲（CAP=1440，约 24h@60s），由 main.rs 启动一个
//! tokio interval(60s) 循环逐点 push。供 admin 监控页「资源历史」(sparkline +
//! 时序折线) 轮询读取。缓冲随进程生命周期，重启清零（实时 sparkline 可接受）。
//!
//! 自带独立 `System` 实例（不复用 monitoring.rs 的 SYS_SNAPSHOT，避免跨模块锁竞争）；
//! sysinfo 要求两次 process refresh 间隔 ≥200ms 才能拿到非零 CPU%，本采样器 60s 间隔
//! 天然满足，首点 CPU 可能为 0。

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use once_cell::sync::Lazy;
use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, RefreshKind, System};

/// 环形缓冲容量（点）。1440 点 @ 60s ≈ 24h。
const CAP: usize = 1440;
/// 采样间隔（秒）。
pub const SAMPLE_INTERVAL_SECS: u64 = 60;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    /// unix 毫秒
    pub ts_ms: i64,
    /// 进程 CPU 使用率（%，可 >100 多核）。
    pub cpu_pct: f64,
    /// 进程 RSS（字节）。
    pub mem_rss_bytes: u64,
    /// 系统 1 分钟负载均值（Windows 恒 0）。
    pub load_one: f64,
}

static RING: Lazy<Mutex<VecDeque<Sample>>> =
    Lazy::new(|| Mutex::new(VecDeque::with_capacity(CAP)));

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 采样器持有的 sysinfo 上下文（进程 CPU+内存）。在循环外创建一次、循环内复用，
/// 以满足 sysinfo「两次 refresh 之间」的 CPU% 语义。
pub struct Sampler {
    sys: System,
    pid: Pid,
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Sampler {
    pub fn new() -> Self {
        let mut sys = System::new_with_specifics(
            RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
        );
        let pid = Pid::from_u32(std::process::id());
        sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
        Self { sys, pid }
    }

    /// 采一点并写入环形缓冲。
    pub fn tick(&mut self) {
        self.sys
            .refresh_processes(sysinfo::ProcessesToUpdate::Some(&[self.pid]), true);
        let (cpu_pct, mem_rss_bytes) = self
            .sys
            .process(self.pid)
            .map(|p| (p.cpu_usage() as f64, p.memory()))
            .unwrap_or((0.0, 0));
        let load_one = System::load_average().one;
        push(Sample {
            ts_ms: now_ms(),
            cpu_pct,
            mem_rss_bytes,
            load_one,
        });
    }
}

fn push(sample: Sample) {
    if let Ok(mut ring) = RING.lock() {
        if ring.len() >= CAP {
            ring.pop_front();
        }
        ring.push_back(sample);
    }
}

/// 读取全部采样点，oldest-first。
pub fn snapshot() -> Vec<Sample> {
    let ring = match RING.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    ring.iter().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_bounded_and_oldest_first() {
        {
            let mut ring = RING.lock().unwrap();
            ring.clear();
        }
        for i in 0..(CAP as i64 + 10) {
            push(Sample {
                ts_ms: i,
                cpu_pct: i as f64,
                mem_rss_bytes: i as u64,
                load_one: 0.0,
            });
        }
        let snap = snapshot();
        assert_eq!(snap.len(), CAP);
        // oldest-first：最早保留的是第 10 个（前 10 个被挤出）。
        assert_eq!(snap.first().unwrap().ts_ms, 10);
        assert!(snap.last().unwrap().ts_ms > snap.first().unwrap().ts_ms);
    }
}
