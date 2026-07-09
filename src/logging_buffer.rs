//! M0-P5：进程内日志环形缓冲 + tracing Layer（P1 升级版）。
//!
//! 单一汇聚点（chokepoint）：所有 tracing 事件经本 Layer 的 `on_event` 落地，
//! 在此完成「字段抽取 → 脱敏 → 入环形缓冲 → 广播实时流 → 落每日归档文件」。
//! 受 init_tracing 里的 EnvFilter 全局过滤约束 —— 只有被启用级别的事件才会到达，
//! 开销可控。缓冲随进程生命周期，重启清零；归档文件按天滚动持久化。
//!
//! P1 升级要点：
//!   - `LogRecord` 加宽：request_id（从 span 抽取）/ module / line / fields / seq。
//!   - 实时流：全局 `broadcast` 通道 + `subscribe()`，供 `/logs/stream` SSE。
//!   - 脱敏：`redact()` 在入缓冲/广播/落盘前对 message + fields 统一脱敏，
//!     密钥不再以明文进入任何下游（环形缓冲 / SSE / 归档文件 / `/logs`）。
//!   - 归档：`ENABLE_FILE_LOGS` 开启时，本 Layer 把**已脱敏**记录按 JSON 行追加到
//!     `{LOG_DIR}/learning-backend.YYYY-MM-DD.log`（取代 logging.rs 里的 fmt json appender，
//!     避免明文密钥落盘）。

use std::collections::VecDeque;
use std::fmt;
use std::fs::File;
use std::io::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

/// 环形缓冲容量（条）。P1：1000 → 3000，覆盖更长的实时回放窗口（约 3–4MB 稳态）。
const CAP: usize = 3000;
/// 单条 message 最大字节，超长按字符边界截断。
const MAX_MSG_LEN: usize = 512;
/// 单条 fields（压平后的结构化字段）最大字节，同样按字符边界截断。
const MAX_FIELDS_LEN: usize = 512;
/// 实时广播通道容量。慢消费者落后即收 `Lagged(n)`，不回压生产者。
const BROADCAST_CAP: usize = 512;

/// 单调递增序号。SSE「回放 + 续传」交接时供前端按 seq 去重（避免时间戳冲突）。
static SEQ: AtomicU64 = AtomicU64::new(0);

/// 命中即按值脱敏的敏感键（大小写不敏感、子串匹配）。
const SENSITIVE_KEYS: &[&str] = &["authorization", "password", "secret", "bearer", "token"];
/// 认证方案前缀：脱敏时保留方案词，仅掩盖其后的凭据（如 `Bearer ***`）。
const AUTH_SCHEMES: &[&str] = &["bearer", "basic", "digest"];

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogRecord {
    /// unix 毫秒
    pub ts_ms: i64,
    /// 单调序号（进程内唯一、递增）
    pub seq: u64,
    /// "ERROR" / "WARN" / "INFO" / "DEBUG" / "TRACE"
    pub level: String,
    pub target: String,
    pub message: String,
    /// 从所属 `request` span 抽取的 request_id（无 span 上下文时为 None）
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub request_id: Option<String>,
    /// 事件源码模块路径
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub module: Option<String>,
    /// 事件源码行号
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub line: Option<u32>,
    /// 除 message 外的结构化字段，压平为 "k=v k=v"（已脱敏）
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fields: Option<String>,
}

/// `/logs`、`/logs/stream`、`/logs/archive/:date` 三处共用的过滤条件。
#[derive(Clone, Default)]
pub struct LogFilter {
    /// 精确级别（大小写不敏感）
    pub level: Option<String>,
    /// target 子串匹配
    pub target: Option<String>,
    /// message 子串匹配（ASCII 大小写不敏感）
    pub q: Option<String>,
    /// request_id 精确匹配
    pub request_id: Option<String>,
    /// 仅返回 ts_ms >= since_ms 的记录
    pub since_ms: Option<i64>,
}

impl LogFilter {
    /// 判定一条记录是否命中全部过滤条件。`/logs/stream` 实时分支复用，确保与
    /// `snapshot_filtered` 语义一致。
    pub fn matches(&self, r: &LogRecord) -> bool {
        if let Some(level) = self.level.as_deref() {
            let want = level.trim();
            if !want.is_empty() && !r.level.eq_ignore_ascii_case(want) {
                return false;
            }
        }
        if let Some(target) = self.target.as_deref() {
            if !target.is_empty() && !r.target.contains(target) {
                return false;
            }
        }
        if let Some(q) = self.q.as_deref() {
            if !q.is_empty() {
                // ASCII 大小写不敏感的 message 子串匹配（契约：仅 message）。
                let hay = r.message.to_ascii_lowercase();
                if !hay.contains(&q.to_ascii_lowercase()) {
                    return false;
                }
            }
        }
        if let Some(rid) = self.request_id.as_deref() {
            if !rid.is_empty() && r.request_id.as_deref() != Some(rid) {
                return false;
            }
        }
        if let Some(since) = self.since_ms {
            if r.ts_ms < since {
                return false;
            }
        }
        true
    }
}

static RING: Lazy<Mutex<VecDeque<LogRecord>>> =
    Lazy::new(|| Mutex::new(VecDeque::with_capacity(CAP)));

/// 全局实时广播通道（运行时无关：`broadcast::send` 同步、无需 Tokio runtime，
/// 可安全从 Layer 的任意调用线程发出）。
static LOG_TX: Lazy<broadcast::Sender<LogRecord>> =
    Lazy::new(|| broadcast::channel(BROADCAST_CAP).0);

/// 订阅实时日志流。
pub fn subscribe() -> broadcast::Receiver<LogRecord> {
    LOG_TX.subscribe()
}

/// 是否启用每日归档文件落盘（`ENABLE_FILE_LOGS`，默认关闭）。仅启动时读一次 env。
static FILE_LOGS_ENABLED: Lazy<bool> = Lazy::new(|| {
    matches!(
        std::env::var("ENABLE_FILE_LOGS")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
});

/// 归档目录（`LOG_DIR`，默认 "./logs"）。仅启动时读一次 env。
static LOG_DIR: Lazy<String> =
    Lazy::new(|| std::env::var("LOG_DIR").unwrap_or_else(|_| "./logs".to_string()));

/// 归档文件保留上限（`MAX_LOG_FILES`，默认 30 天/文件）。仅启动时读一次 env。
/// 0 = 不裁剪。磁盘压力缓解：跨日翻转时删除超出此数的最旧每日归档。
static MAX_LOG_FILES: Lazy<usize> =
    Lazy::new(|| std::env::var("MAX_LOG_FILES").ok().and_then(|s| s.parse().ok()).unwrap_or(30));

/// 当前打开的归档文件：(日期 "YYYY-MM-DD", 句柄)。跨日重新打开。
static FILE: Lazy<Mutex<Option<(String, File)>>> = Lazy::new(|| Mutex::new(None));

/// 是否启用文件归档（供 `/logs/archive` 上报）。
pub fn file_logs_enabled() -> bool {
    *FILE_LOGS_ENABLED
}

/// 归档目录（供 `/logs/archive` 列目录）。
pub fn log_dir() -> String {
    LOG_DIR.clone()
}

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

/// UTF-8 首字节对应的字符字节数（用于逐字符复制时保持边界）。
fn utf8_char_len(b: u8) -> usize {
    if b < 0x80 {
        1
    } else if b >> 5 == 0b110 {
        2
    } else if b >> 4 == 0b1110 {
        3
    } else if b >> 3 == 0b11110 {
        4
    } else {
        1 // 非法首字节：按 1 字节推进，避免死循环
    }
}

/// 值的终止字节：空白 + 常见结构分隔符。多字节 UTF-8 续字节均 >= 0x80，不会命中，
/// 故按字节扫描不会切断字符。
fn is_value_stop(b: u8) -> bool {
    matches!(
        b,
        b' ' | b'\t'
            | b'\r'
            | b'\n'
            | b'"'
            | b'\''
            | b','
            | b';'
            | b'}'
            | b']'
            | b')'
            | b'&'
            | b'|'
    )
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t')
}

/// 在 `p` 处若为认证方案词（bearer/basic/digest）+ 空白，返回方案与空白之后的位置；
/// 否则返回 `p`。
fn skip_auth_scheme(ob: &[u8], lb: &[u8], p: usize) -> usize {
    let n = ob.len();
    for sc in AUTH_SCHEMES {
        let s = sc.as_bytes();
        if p + s.len() < n && &lb[p..p + s.len()] == s && is_ws(ob[p + s.len()]) {
            let mut q = p + s.len();
            while q < n && is_ws(ob[q]) {
                q += 1;
            }
            return q;
        }
    }
    p
}

/// 单一脱敏入口：把 message / fields 中敏感键的值替换为 `***`。大小写不敏感、best-effort。
/// 覆盖三种结构：`key=value`、`key: value`、`key":"value"`（JSON）；`bearer <token>` 的
/// 凭据按其后首个 token 掩盖，认证方案词保留。无敏感键命中时零成本返回。
fn redact(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let lb = lower.as_bytes();
    if !SENSITIVE_KEYS.iter().any(|k| lower.contains(k)) {
        return s.to_string();
    }
    let ob = s.as_bytes();
    let n = ob.len();
    let mut out: Vec<u8> = Vec::with_capacity(n + 8);
    let mut i = 0usize;
    'outer: while i < n {
        for key in SENSITIVE_KEYS {
            let k = key.as_bytes();
            if i + k.len() <= n && &lb[i..i + k.len()] == k {
                if *key == "bearer" {
                    // bearer <token>：方案词后接空白再接凭据 token。
                    let mut p = i + k.len();
                    let vs_start = p;
                    while p < n && is_ws(ob[p]) {
                        p += 1;
                    }
                    if p > vs_start && p < n && !is_value_stop(ob[p]) {
                        out.extend_from_slice(&ob[i..p]); // 方案词 + 空白
                        let mut ve = p;
                        while ve < n && !is_value_stop(ob[ve]) {
                            ve += 1;
                        }
                        out.extend_from_slice(b"***");
                        i = ve;
                        continue 'outer;
                    }
                    // 非 "bearer <token>" 结构：当作普通文本，落到逐字符复制。
                } else {
                    // key{quote/space}*[:=]{quote/space}*[scheme ]?value
                    let mut p = i + k.len();
                    while p < n && (is_ws(ob[p]) || ob[p] == b'"' || ob[p] == b'\'') {
                        p += 1;
                    }
                    if p < n && (ob[p] == b'=' || ob[p] == b':') {
                        p += 1; // 吃掉分隔符
                        while p < n && (is_ws(ob[p]) || ob[p] == b'"' || ob[p] == b'\'') {
                            p += 1;
                        }
                        // 保留认证方案词（Bearer/Basic/Digest），只掩盖其后的凭据。
                        p = skip_auth_scheme(ob, lb, p);
                        if p < n && !is_value_stop(ob[p]) {
                            out.extend_from_slice(&ob[i..p]); // 键 + 分隔 + 方案词
                            let mut ve = p;
                            while ve < n && !is_value_stop(ob[ve]) {
                                ve += 1;
                            }
                            out.extend_from_slice(b"***");
                            i = ve;
                            continue 'outer;
                        }
                    }
                    // 非 key=value 结构：当作普通文本，落到逐字符复制。
                }
                break; // 该键命中但非可脱敏结构，避免重复尝试其他键
            }
        }
        // 未脱敏：逐字符复制（保持 UTF-8 边界）。
        let cl = utf8_char_len(ob[i]).min(n - i);
        out.extend_from_slice(&ob[i..i + cl]);
        i += cl;
    }
    // out 由原文逐字节区段 + ASCII 替换/分隔拼成，必为合法 UTF-8；兜底回退原串。
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

/// request_id 的 span 扩展载体。
#[derive(Clone)]
struct RequestId(String);

/// 仅抽取 message 字段 + 其余结构化字段。
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

/// 仅抽取 `request_id` span 字段。
#[derive(Default)]
struct RequestIdVisitor {
    request_id: Option<String>,
}

impl Visit for RequestIdVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "request_id" {
            // info_span!("request", request_id = %request_id) 走 record_str；
            // 此处兜底处理 Debug 形态，去掉可能的引号。
            self.request_id = Some(format!("{value:?}").trim_matches('"').to_string());
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "request_id" {
            self.request_id = Some(value.to_string());
        }
    }
}

/// 零尺寸 Layer：捕获 request_id span 字段；on_event 把事件落入环形缓冲/广播/归档。
pub struct RingBufferLayer;

pub fn layer() -> RingBufferLayer {
    RingBufferLayer
}

impl<S> Layer<S> for RingBufferLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let mut visitor = RequestIdVisitor::default();
        attrs.record(&mut visitor);
        if let Some(rid) = visitor.request_id {
            if let Some(span) = ctx.span(id) {
                span.extensions_mut().insert(RequestId(rid));
            }
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let meta = event.metadata();
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);

        let mut message = visitor.message;
        truncate_on_char_boundary(&mut message, MAX_MSG_LEN);
        let mut message = redact(&message);
        truncate_on_char_boundary(&mut message, MAX_MSG_LEN);

        let fields = if visitor.fields.is_empty() {
            None
        } else {
            let mut joined = redact(&visitor.fields.join(" "));
            truncate_on_char_boundary(&mut joined, MAX_FIELDS_LEN);
            Some(joined)
        };

        // 就近向外遍历 event 所属 span 链，取第一个携带 RequestId 的 span。
        // 无 span 上下文（如启动期日志）时 .into_iter().flatten() 直接产出 None。
        let request_id = ctx
            .event_scope(event)
            .into_iter()
            .flatten()
            .find_map(|span| span.extensions().get::<RequestId>().map(|r| r.0.clone()));

        let rec = LogRecord {
            ts_ms: now_ms(),
            seq: SEQ.fetch_add(1, Ordering::Relaxed),
            level: meta.level().to_string(),
            target: meta.target().to_string(),
            message,
            request_id,
            module: meta.module_path().map(str::to_string),
            line: meta.line(),
            fields,
        };

        // 1) 入环形缓冲：锁的持有范围最小化，绝不跨 LOG_TX.send / 文件 IO / tracing::*。
        if let Ok(mut ring) = RING.lock() {
            if ring.len() >= CAP {
                ring.pop_front();
            }
            ring.push_back(rec.clone());
        }

        // 2) 落每日归档文件（已脱敏）。best-effort：任何错误静默吞掉，绝不在此调用 tracing::*。
        if *FILE_LOGS_ENABLED {
            append_archive(&rec);
        }

        // 3) 广播到实时流。无订阅者 / 通道满均返回 Err，忽略即可。
        let _ = LOG_TX.send(rec);
    }
}

/// 把一条已脱敏记录按 JSON 行追加到当日归档文件；跨日自动重开。best-effort，吞错。
fn append_archive(rec: &LogRecord) {
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let mut guard = match FILE.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let need_reopen = guard.as_ref().map(|(d, _)| d != &date).unwrap_or(true);
    if need_reopen {
        let _ = std::fs::create_dir_all(LOG_DIR.as_str());
        let path = format!("{}/learning-backend.{}.log", LOG_DIR.as_str(), date);
        match std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            Ok(f) => *guard = Some((date.clone(), f)),
            Err(_) => return,
        }
        // 跨日翻转时裁剪旧归档（每日仅一次，不在每条日志的热路径）。
        prune_archive(*MAX_LOG_FILES);
    }
    if let Some((_, file)) = guard.as_mut() {
        if let Ok(line) = serde_json::to_string(rec) {
            let _ = writeln!(file, "{line}");
        }
    }
}

/// 删除超出 `max_files` 的最旧每日归档（文件名 `learning-backend.YYYY-MM-DD.log`，
/// 日期字典序=时间序）。max_files==0 不裁剪。best-effort，吞错。
fn prune_archive(max_files: usize) {
    if max_files == 0 {
        return;
    }
    let rd = match std::fs::read_dir(LOG_DIR.as_str()) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    let mut files: Vec<String> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            (name.starts_with("learning-backend.") && name.ends_with(".log")).then_some(name)
        })
        .collect();
    if files.len() <= max_files {
        return;
    }
    files.sort(); // 旧→新（日期字典序）
    let drop_n = files.len() - max_files;
    for name in files.into_iter().take(drop_n) {
        let _ = std::fs::remove_file(format!("{}/{}", LOG_DIR.as_str(), name));
    }
}

/// 按过滤条件读取最近日志（最新在前），单次遍历。
pub fn snapshot_filtered(limit: usize, filter: &LogFilter) -> Vec<LogRecord> {
    let ring = match RING.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let mut out: Vec<LogRecord> = ring
        .iter()
        .rev() // 最新在前
        .filter(|r| filter.matches(r))
        .take(limit)
        .cloned()
        .collect();
    out.shrink_to_fit();
    out
}

/// 读取最近日志（最新在前）。level 传 Some("WARN") 时按精确级别过滤。
/// 兼容旧调用方，内部复用 `snapshot_filtered`。
pub fn snapshot(limit: usize, level: Option<&str>) -> Vec<LogRecord> {
    let filter = LogFilter {
        level: level
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        ..LogFilter::default()
    };
    snapshot_filtered(limit, &filter)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RING 是进程级全局静态；多个改写 RING 的测试默认并行会相互踩踏，
    /// 用此串行锁把它们排队（中毒后取回内部值，避免连锁 panic）。
    static TEST_GUARD: Mutex<()> = Mutex::new(());

    fn rec(seq: u64, ts: i64, level: &str, target: &str, msg: &str) -> LogRecord {
        LogRecord {
            ts_ms: ts,
            seq,
            level: level.into(),
            target: target.into(),
            message: msg.into(),
            request_id: None,
            module: None,
            line: None,
            fields: None,
        }
    }

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
        let _g = TEST_GUARD.lock().unwrap_or_else(|p| p.into_inner());
        {
            let mut ring = RING.lock().unwrap();
            ring.clear();
            for i in 0..5 {
                ring.push_back(rec(
                    i as u64,
                    i,
                    if i % 2 == 0 { "INFO" } else { "WARN" },
                    "t",
                    &format!("m{i}"),
                ));
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

    #[test]
    fn snapshot_filtered_target_q_request_id() {
        let _g = TEST_GUARD.lock().unwrap_or_else(|p| p.into_inner());
        {
            let mut ring = RING.lock().unwrap();
            ring.clear();
            let mut r0 = rec(0, 0, "INFO", "amas::engine", "Hello World");
            r0.request_id = Some("req-1".into());
            let mut r1 = rec(1, 1, "ERROR", "routes::auth", "login Failed");
            r1.request_id = Some("req-2".into());
            ring.push_back(r0);
            ring.push_back(r1);
        }
        // target 子串
        let t = snapshot_filtered(
            10,
            &LogFilter {
                target: Some("amas".into()),
                ..Default::default()
            },
        );
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].target, "amas::engine");
        // q：message 子串、ASCII 大小写不敏感
        let q = snapshot_filtered(
            10,
            &LogFilter {
                q: Some("failed".into()),
                ..Default::default()
            },
        );
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].message, "login Failed");
        // request_id 精确
        let r = snapshot_filtered(
            10,
            &LogFilter {
                request_id: Some("req-1".into()),
                ..Default::default()
            },
        );
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].request_id.as_deref(), Some("req-1"));
        // since_ms
        let s = snapshot_filtered(
            10,
            &LogFilter {
                since_ms: Some(1),
                ..Default::default()
            },
        );
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].ts_ms, 1);
    }

    #[test]
    fn redact_masks_common_secret_shapes() {
        // key=value
        assert_eq!(redact("token=abcdef"), "token=***");
        // key: value（保留后续字段）
        assert_eq!(redact("password: hunter2 user=bob"), "password: *** user=bob");
        // JSON 形态 key":"value"
        assert_eq!(redact(r#"{"secret":"s3cr3t"}"#), r#"{"secret":"***"}"#);
        // 引号包裹 + 认证方案词：保留 Bearer，掩盖凭据
        assert_eq!(
            redact(r#"authorization="Bearer xyz123""#),
            r#"authorization="Bearer ***""#
        );
        // 裸 Authorization header：保留 Bearer 方案词
        assert_eq!(
            redact("Authorization: Bearer eyJabc.def"),
            "Authorization: Bearer ***"
        );
        // 独立 bearer <token>
        assert_eq!(redact("got Bearer tok_999 now"), "got Bearer *** now");
    }

    #[test]
    fn captures_request_id_from_enclosing_span() {
        use tracing_subscriber::layer::SubscriberExt;

        let _g = TEST_GUARD.lock().unwrap_or_else(|p| p.into_inner());
        {
            let mut ring = RING.lock().unwrap();
            ring.clear();
        }
        // 本地 subscriber（仅含本 Layer，无 EnvFilter）覆盖全局默认，限定在闭包内生效。
        let subscriber = tracing_subscriber::registry().with(layer());
        tracing::subscriber::with_default(subscriber, || {
            tracing::info_span!("request", request_id = %"req-xyz").in_scope(|| {
                tracing::info!("inside span");
            });
            // span 外的事件不应带 request_id
            tracing::info!("outside span");
        });

        let ring = RING.lock().unwrap();
        let inside = ring
            .iter()
            .find(|r| r.message == "inside span")
            .expect("应捕获 span 内事件");
        assert_eq!(inside.request_id.as_deref(), Some("req-xyz"));
        let outside = ring
            .iter()
            .find(|r| r.message == "outside span")
            .expect("应捕获 span 外事件");
        assert_eq!(outside.request_id, None);
        // module 路径应被填充（本测试模块）
        assert!(inside.module.is_some());
    }

    #[test]
    fn redact_is_noop_without_secrets() {
        let s = "user=bob action=login latency_ms=12 中文日志";
        assert_eq!(redact(s), s);
    }

    #[test]
    fn redact_handles_non_keyvalue_keyword() {
        // "token" 出现但非 key=value 结构：不误伤
        assert_eq!(redact("the token is fine"), "the token is fine");
    }
}
