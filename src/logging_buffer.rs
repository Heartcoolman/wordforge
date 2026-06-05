//! M0-P5：进程内日志环形缓冲 + tracing Layer。
//!
//! 把 tracing 事件（level/target/message/字段）落入一个有界 VecDeque，
//! 供 admin 监控页「实时日志」面板轮询读取。受 init_tracing 里的 EnvFilter
//! 全局过滤约束 —— 只有被启用级别的事件才会到达本 Layer，开销可控。
//! 缓冲随进程生命周期，重启清零。

use std::collections::VecDeque;
use std::fmt;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use once_cell::sync::Lazy;
use serde::Serialize;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

/// 环形缓冲容量（条）。
const CAP: usize = 1000;
/// 单条 message 最大字节，超长按字符边界截断。
const MAX_MSG_LEN: usize = 512;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogRecord {
    /// unix 毫秒
    pub ts_ms: i64,
    /// "ERROR" / "WARN" / "INFO" / "DEBUG" / "TRACE"
    pub level: String,
    pub target: String,
    pub message: String,
}

static RING: Lazy<Mutex<VecDeque<LogRecord>>> =
    Lazy::new(|| Mutex::new(VecDeque::with_capacity(CAP)));

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn truncate_on_char_boundary(s: &mut String, max: usize) {
    if s.len() > max {
        let mut idx = max;
        while idx > 0 && !s.is_char_boundary(idx) {
            idx -= 1;
        }
        s.truncate(idx);
    }
}

/// 抽取事件的 message 字段 + 其余结构化字段。
#[derive(Default)]
struct MessageVisitor {
    message: String,
    fields: Vec<String>,
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        } else {
            self.fields.push(format!("{}={value:?}", field.name()));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.fields.push(format!("{}={value}", field.name()));
        }
    }
}

/// 零尺寸 Layer：on_event 把事件落入环形缓冲。
pub struct RingBufferLayer;

pub fn layer() -> RingBufferLayer {
    RingBufferLayer
}

impl<S> Layer<S> for RingBufferLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);

        let mut message = visitor.message;
        if !visitor.fields.is_empty() {
            let extra = visitor.fields.join(" ");
            if message.is_empty() {
                message = extra;
            } else {
                message.push(' ');
                message.push_str(&extra);
            }
        }
        truncate_on_char_boundary(&mut message, MAX_MSG_LEN);

        let rec = LogRecord {
            ts_ms: now_ms(),
            level: meta.level().to_string(),
            target: meta.target().to_string(),
            message,
        };

        if let Ok(mut ring) = RING.lock() {
            if ring.len() >= CAP {
                ring.pop_front();
            }
            ring.push_back(rec);
        }
    }
}

/// 读取最近日志（最新在前）。level 传 Some("WARN") 时按精确级别过滤。
pub fn snapshot(limit: usize, level: Option<&str>) -> Vec<LogRecord> {
    let ring = match RING.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let want = level
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_uppercase());
    let mut out: Vec<LogRecord> = ring
        .iter()
        .filter(|r| match &want {
            Some(l) => &r.level == l,
            None => true,
        })
        .cloned()
        .collect();
    out.reverse();
    out.truncate(limit);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_respects_utf8_boundary() {
        let mut s = "中文测试".repeat(200); // 远超 512 字节
        truncate_on_char_boundary(&mut s, MAX_MSG_LEN);
        assert!(s.len() <= MAX_MSG_LEN);
        // 截断点必须是有效 UTF-8（重新 chars 不 panic）
        assert!(s.chars().count() > 0);
    }

    #[test]
    fn snapshot_level_filter_and_order() {
        {
            let mut ring = RING.lock().unwrap();
            ring.clear();
            for i in 0..5 {
                ring.push_back(LogRecord {
                    ts_ms: i,
                    level: if i % 2 == 0 { "INFO" } else { "WARN" }.into(),
                    target: "t".into(),
                    message: format!("m{i}"),
                });
            }
        }
        let all = snapshot(10, None);
        assert_eq!(all.len(), 5);
        // 最新在前
        assert_eq!(all[0].message, "m4");
        let warns = snapshot(10, Some("warn"));
        assert!(warns.iter().all(|r| r.level == "WARN"));
        assert_eq!(warns.len(), 2);
    }
}
