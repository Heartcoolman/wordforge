//! WordForge 学习端 /api/* 并发承载压测工具。
//!
//! 两个子命令：
//!   provision —— 预注册账号池（解耦认证与稳态负载，避开注册端点 10/60s IP 限流爬坡瓶颈）
//!   run       —— 用账号池跑真实学习循环，交互式加减设备数，实时仪表盘观测承载能力
//!
//! 真实学习循环（忠实复刻客户端时序）：
//!   create session → next-words 取批 → 逐词(pick-next-word → generate-options → 答题)
//!   → 周期 sync-progress → complete-session → 进入下一轮
//!
//! 两种测试目标（服务器侧限流由后端 env 控制，本工具不负责关限流）：
//!   raw       —— 闭环最大吞吐（无 think-time），配合 dev 放开限流 → 测后端裸容量
//!   realistic —— 注入人类 think-time（每词数秒），配合保留限流 → 测限流下真实承载

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::Value;

// ============================ CLI ============================

#[derive(Parser)]
#[command(name = "wordforge-loadtest", version, about = "WordForge 学习端并发承载压测")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// 预注册账号池，写入 JSON 文件（access token 默认 24h 有效，够任何压测窗口）
    Provision(ProvisionArgs),
    /// 用账号池跑学习负载，交互式加减设备数
    Run(RunArgs),
}

#[derive(Parser)]
struct ProvisionArgs {
    /// 目标后端地址，如 http://192.168.1.10:3000
    #[arg(long)]
    url: String,
    /// 要注册的账号数量
    #[arg(long, default_value_t = 200)]
    count: usize,
    /// 输出文件
    #[arg(long, default_value = "accounts.json")]
    out: String,
    /// 邮箱/用户名前缀
    #[arg(long, default_value = "ldt")]
    prefix: String,
    /// 统一密码
    #[arg(long, default_value = "LoadTest!2026")]
    password: String,
    /// 并发注册数（注意：dev 若未放开 AUTH_RATE_LIMIT_MAX，会被 10/60s 卡住）
    #[arg(long, default_value_t = 16)]
    concurrency: usize,
}

#[derive(Parser)]
struct RunArgs {
    /// 目标后端地址
    #[arg(long)]
    url: String,
    /// 账号池文件（provision 产物）
    #[arg(long, default_value = "accounts.json")]
    accounts: String,
    /// 若账号文件不存在，自动先 provision 这么多个
    #[arg(long)]
    provision: Option<usize>,
    /// 测试模式
    #[arg(long, value_enum, default_value_t = Mode::Realistic)]
    mode: Mode,
    /// 起始并发设备数
    #[arg(long, default_value_t = 10)]
    start: usize,
    /// 自动爬坡速率（设备/秒），不填则纯手动加减
    #[arg(long)]
    ramp: Option<usize>,
    /// 每会话目标掌握词数（targetMasteryCount）
    #[arg(long, default_value_t = 12)]
    target_mastery: u32,
    /// realistic 模式每词 think-time 下限(ms)
    #[arg(long, default_value_t = 1200)]
    think_min: u64,
    /// realistic 模式每词 think-time 上限(ms)
    #[arg(long, default_value_t = 3500)]
    think_max: u64,
    /// 每多少个答题事件 sync 一次（上限 100，见后端 SYNC_PROGRESS_EVENTS_LIMIT）
    #[arg(long, default_value_t = 5)]
    sync_every: usize,
    /// 把每秒采样追加到 CSV
    #[arg(long)]
    csv: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Mode {
    /// 闭环最大吞吐，无 think-time（测裸容量，需 dev 放开限流）
    Raw,
    /// 注入人类 think-time（测限流下真实承载）
    Realistic,
}

// ============================ 账号 ============================

#[derive(Clone, Serialize, Deserialize)]
struct Account {
    email: String,
    password: String,
    access_token: String,
    refresh_token: String,
}

// ============================ 指标 ============================

const NUM_BINS: usize = 661;

/// 无锁延迟直方图：低延迟段 1ms 精度，高延迟段降精度，覆盖 0~30s+。
struct Hist {
    bins: Vec<AtomicU64>,
}

impl Hist {
    fn new() -> Self {
        Self {
            bins: (0..NUM_BINS).map(|_| AtomicU64::new(0)).collect(),
        }
    }
    fn record(&self, ms: u64) {
        let i = bin_index(ms);
        self.bins[i].fetch_add(1, Ordering::Relaxed);
    }
    fn snapshot(&self) -> (Vec<u64>, u64) {
        let mut total = 0;
        let v: Vec<u64> = self
            .bins
            .iter()
            .map(|b| {
                let c = b.load(Ordering::Relaxed);
                total += c;
                c
            })
            .collect();
        (v, total)
    }
}

fn bin_index(ms: u64) -> usize {
    if ms < 200 {
        ms as usize
    } else if ms < 2000 {
        200 + ((ms - 200) / 10) as usize
    } else if ms < 30000 {
        380 + ((ms - 2000) / 100) as usize
    } else {
        660
    }
}

fn bin_upper_ms(i: usize) -> u64 {
    if i < 200 {
        i as u64 + 1
    } else if i < 380 {
        200 + (i - 200 + 1) as u64 * 10
    } else if i < 660 {
        2000 + (i - 380 + 1) as u64 * 100
    } else {
        30000
    }
}

fn percentile(bins: &[u64], total: u64, p: f64) -> u64 {
    if total == 0 {
        return 0;
    }
    let target = (total as f64 * p).ceil() as u64;
    let mut acc = 0u64;
    for (i, c) in bins.iter().enumerate() {
        acc += c;
        if acc >= target {
            return bin_upper_ms(i);
        }
    }
    bin_upper_ms(bins.len() - 1)
}

#[repr(usize)]
#[derive(Clone, Copy)]
enum Ep {
    Session = 0,
    NextWords = 1,
    Pick = 2,
    GenOptions = 3,
    Sync = 4,
    Complete = 5,
    Auth = 6,
}
const EP_COUNT: usize = 7;
const EP_NAMES: [&str; EP_COUNT] = [
    "session",
    "next-words",
    "pick-word",
    "gen-options",
    "sync-prog",
    "complete",
    "auth",
];

struct Metrics {
    reqs: Vec<AtomicU64>,
    errs: Vec<AtomicU64>,
    hist: Vec<Hist>,
    throttled: AtomicU64,
    err_codes: Mutex<HashMap<String, u64>>,
}

impl Metrics {
    fn new() -> Self {
        Self {
            reqs: (0..EP_COUNT).map(|_| AtomicU64::new(0)).collect(),
            errs: (0..EP_COUNT).map(|_| AtomicU64::new(0)).collect(),
            hist: (0..EP_COUNT).map(|_| Hist::new()).collect(),
            throttled: AtomicU64::new(0),
            err_codes: Mutex::new(HashMap::new()),
        }
    }
    fn record_ok(&self, ep: Ep, ms: u64) {
        let i = ep as usize;
        self.reqs[i].fetch_add(1, Ordering::Relaxed);
        self.hist[i].record(ms);
    }
    fn record_err(&self, ep: Ep, code: &str) {
        let i = ep as usize;
        self.reqs[i].fetch_add(1, Ordering::Relaxed);
        self.errs[i].fetch_add(1, Ordering::Relaxed);
        if code == "RATE_LIMITED" || code == "AUTH_RATE_LIMITED" {
            self.throttled.fetch_add(1, Ordering::Relaxed);
        }
        let mut m = self.err_codes.lock().unwrap();
        *m.entry(code.to_string()).or_insert(0) += 1;
    }
    fn total_reqs(&self) -> u64 {
        self.reqs.iter().map(|a| a.load(Ordering::Relaxed)).sum()
    }
    fn total_errs(&self) -> u64 {
        self.errs.iter().map(|a| a.load(Ordering::Relaxed)).sum()
    }
}

// ============================ 工具函数 ============================

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// 轻量 xorshift，免 rand 依赖。seed 必须非零。
struct Rng(u64);
impl Rng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn frac(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        if hi <= lo {
            return lo;
        }
        lo + (self.frac() * (hi - lo) as f64) as u64
    }
}

fn client(url: &str) -> reqwest::Client {
    let _ = url;
    reqwest::Client::builder()
        .pool_max_idle_per_host(512)
        .timeout(Duration::from_secs(30))
        .build()
        .expect("build reqwest client")
}

fn apply_headers(rb: reqwest::RequestBuilder, token: Option<&str>, device_id: &str) -> reqwest::RequestBuilder {
    let mut rb = rb
        .header("user-agent", "WordForge-Android/1.1.2 (loadtest)")
        .header("x-device-platform", "android")
        .header("x-device-id", device_id)
        .header("x-app-version", "1.1.2");
    if let Some(t) = token {
        rb = rb.header("authorization", format!("Bearer {t}"));
    }
    rb
}

/// 发 POST，计时 + 记指标。2xx 返回 Ok(data)；失败返回 Err(状态码)，
/// 传输错误(超时/连不上)状态码为 None。调用方据此区分"token 过期(401/403)"
/// 与"后端被打满(5xx/超时/429)"，避免把后者误判为过期而盲目重登污染指标。
async fn post(
    cli: &reqwest::Client,
    base: &str,
    path: &str,
    token: Option<&str>,
    device_id: &str,
    body: Value,
    ep: Ep,
    m: &Metrics,
) -> Result<Value, Option<u16>> {
    let start = Instant::now();
    let rb = apply_headers(cli.post(format!("{base}{path}")), token, device_id).json(&body);
    let resp = match rb.send().await {
        Ok(r) => r,
        Err(e) => {
            let code = if e.is_timeout() { "TIMEOUT" } else { "CONNECT" };
            m.record_err(ep, code);
            return Err(None);
        }
    };
    let status = resp.status();
    let ms = start.elapsed().as_millis() as u64;
    if status.is_success() {
        m.record_ok(ep, ms);
        let v: Value = resp.json().await.unwrap_or(Value::Null);
        return Ok(v.get("data").cloned().unwrap_or(v));
    }
    // 非 2xx：尽量取 code 字段记账
    let code = match resp.json::<Value>().await {
        Ok(v) => v
            .get("code")
            .and_then(|c| c.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("HTTP_{}", status.as_u16())),
        Err(_) => format!("HTTP_{}", status.as_u16()),
    };
    m.record_err(ep, &code);
    Err(Some(status.as_u16()))
}

// ============================ Provision ============================

async fn provision(args: ProvisionArgs) -> std::io::Result<()> {
    println!(
        "provision: 注册 {} 个账号 -> {} (并发 {})",
        args.count, args.out, args.concurrency
    );
    println!("提示: dev 若未放开 AUTH_RATE_LIMIT_MAX，注册会被 10/60s/IP 限流卡住。");
    let cli = client(&args.url);
    let base = args.url.clone();
    let prefix = args.prefix.clone();
    let password = args.password.clone();

    let next = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicUsize::new(0));
    let out: Arc<Mutex<Vec<Account>>> = Arc::new(Mutex::new(Vec::with_capacity(args.count)));

    let mut handles = Vec::new();
    for _ in 0..args.concurrency.max(1) {
        let cli = cli.clone();
        let base = base.clone();
        let prefix = prefix.clone();
        let password = password.clone();
        let next = next.clone();
        let done = done.clone();
        let out = out.clone();
        let count = args.count;
        handles.push(tokio::spawn(async move {
            loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= count {
                    break;
                }
                let email = format!("{prefix}{i:06}@loadtest.local");
                let username = format!("{prefix}{i:06}");
                let device_id = format!("ldt-{i:06}");
                let mut backoff = 1u64;
                loop {
                    let acct = register_or_login(
                        &cli, &base, &email, &username, &password, &device_id,
                    )
                    .await;
                    match acct {
                        Ok(a) => {
                            out.lock().unwrap().push(a);
                            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                            if d % 50 == 0 {
                                println!("  已就绪 {d}/{count}");
                            }
                            break;
                        }
                        Err(Throttled) => {
                            tokio::time::sleep(Duration::from_secs(backoff)).await;
                            backoff = (backoff * 2).min(30);
                        }
                        Err(Fatal(msg)) => {
                            eprintln!("  账号 {i} 失败: {msg}");
                            break;
                        }
                    }
                }
            }
        }));
    }
    for h in handles {
        let _ = h.await;
    }

    let accounts = out.lock().unwrap().clone();
    let json = serde_json::to_string_pretty(&accounts).unwrap();
    std::fs::write(&args.out, json)?;
    println!("完成: 写入 {} 个账号 -> {}", accounts.len(), args.out);
    Ok(())
}

enum ProvErr {
    Throttled,
    Fatal(String),
}
use ProvErr::{Fatal, Throttled};

async fn register_or_login(
    cli: &reqwest::Client,
    base: &str,
    email: &str,
    username: &str,
    password: &str,
    device_id: &str,
) -> Result<Account, ProvErr> {
    // 先 register；409(邮箱已存在) 则 login，幂等可重跑。
    let reg = apply_headers(cli.post(format!("{base}/api/auth/register")), None, device_id)
        .json(&json!({ "email": email, "username": username, "password": password }))
        .send()
        .await
        .map_err(|e| Fatal(e.to_string()))?;
    let st = reg.status();
    if st.is_success() {
        let v: Value = reg.json().await.map_err(|e| Fatal(e.to_string()))?;
        return parse_tokens(&v, email, password);
    }
    if st.as_u16() == 429 {
        return Err(Throttled);
    }
    // 409 或其它 → 尝试登录
    let login = apply_headers(cli.post(format!("{base}/api/auth/login")), None, device_id)
        .json(&json!({ "email": email, "password": password }))
        .send()
        .await
        .map_err(|e| Fatal(e.to_string()))?;
    if login.status().as_u16() == 429 {
        return Err(Throttled);
    }
    if !login.status().is_success() {
        return Err(Fatal(format!("register={} login={}", st, login.status())));
    }
    let v: Value = login.json().await.map_err(|e| Fatal(e.to_string()))?;
    parse_tokens(&v, email, password)
}

fn parse_tokens(v: &Value, email: &str, password: &str) -> Result<Account, ProvErr> {
    let data = v.get("data").unwrap_or(v);
    let at = data
        .get("accessToken")
        .and_then(|x| x.as_str())
        .ok_or_else(|| Fatal("响应缺 accessToken".into()))?;
    let rt = data
        .get("refreshToken")
        .and_then(|x| x.as_str())
        .unwrap_or("");
    Ok(Account {
        email: email.to_string(),
        password: password.to_string(),
        access_token: at.to_string(),
        refresh_token: rt.to_string(),
    })
}

// ============================ Run ============================

struct RunCtx {
    cli: reqwest::Client,
    base: String,
    mode: Mode,
    target_mastery: u32,
    think_min: u64,
    think_max: u64,
    sync_every: usize,
    metrics: Arc<Metrics>,
    shutdown: Arc<AtomicBool>,
}

async fn run(args: RunArgs) -> std::io::Result<()> {
    // 账号池
    if std::fs::metadata(&args.accounts).is_err() {
        match args.provision {
            Some(n) => {
                provision(ProvisionArgs {
                    url: args.url.clone(),
                    count: n,
                    out: args.accounts.clone(),
                    prefix: "ldt".into(),
                    password: "LoadTest!2026".into(),
                    concurrency: 16,
                })
                .await?;
            }
            None => {
                eprintln!(
                    "账号文件 {} 不存在。先跑 provision，或加 --provision N 自动注册。",
                    args.accounts
                );
                std::process::exit(1);
            }
        }
    }
    let accounts: Vec<Account> =
        serde_json::from_str(&std::fs::read_to_string(&args.accounts)?).expect("解析账号文件");
    if accounts.is_empty() {
        eprintln!("账号池为空");
        std::process::exit(1);
    }
    let pool_len = accounts.len();
    let accounts = Arc::new(accounts);

    let ctx = Arc::new(RunCtx {
        cli: client(&args.url),
        base: args.url.clone(),
        mode: args.mode,
        target_mastery: args.target_mastery,
        think_min: args.think_min,
        think_max: args.think_max,
        sync_every: args.sync_every.clamp(1, 100),
        metrics: Arc::new(Metrics::new()),
        shutdown: Arc::new(AtomicBool::new(false)),
    });

    let target = Arc::new(AtomicUsize::new(args.start.min(pool_len)));
    let running = Arc::new(AtomicUsize::new(0));
    // 空闲账号栈（按 index）
    let free: Arc<Mutex<Vec<usize>>> = Arc::new(Mutex::new((0..pool_len).rev().collect()));

    println!(
        "run: 模式={} 账号池={} 起始={} 上限={}{}",
        match args.mode { Mode::Raw => "raw(裸容量)", Mode::Realistic => "realistic(真实节奏)" },
        pool_len,
        target.load(Ordering::Relaxed),
        pool_len,
        match args.ramp { Some(r) => format!(" 自动爬坡={r}/s"), None => String::new() }
    );
    println!("交互: 输入数字=设为该并发 | +N 加 | -N 减 | q 退出 | 回车=刷新");
    println!();

    // 手动干预标志：用户一旦手动设并发即停止自动爬坡，交还控制权
    let manual = Arc::new(AtomicBool::new(false));

    // 自动爬坡
    if let Some(rate) = args.ramp {
        let target = target.clone();
        let shutdown = ctx.shutdown.clone();
        let manual = manual.clone();
        tokio::spawn(async move {
            let mut t = tokio::time::interval(Duration::from_secs(1));
            loop {
                t.tick().await;
                if shutdown.load(Ordering::Relaxed) || manual.load(Ordering::Relaxed) {
                    break;
                }
                let cur = target.load(Ordering::Relaxed);
                if cur >= pool_len {
                    break;
                }
                target.store((cur + rate).min(pool_len), Ordering::Relaxed);
            }
        });
    }

    // 交互输入（独立线程读 stdin，避免阻塞 runtime）
    {
        let target = target.clone();
        let shutdown = ctx.shutdown.clone();
        let manual = manual.clone();
        std::thread::spawn(move || {
            let stdin = std::io::stdin();
            let mut line = String::new();
            loop {
                line.clear();
                if stdin.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                let s = line.trim();
                if s.eq_ignore_ascii_case("q") {
                    shutdown.store(true, Ordering::Relaxed);
                    break;
                }
                let cur = target.load(Ordering::Relaxed);
                let newv = if let Some(rest) = s.strip_prefix('+') {
                    rest.trim().parse::<usize>().ok().map(|n| (cur + n).min(pool_len))
                } else if let Some(rest) = s.strip_prefix('-') {
                    rest.trim().parse::<usize>().ok().map(|n| cur.saturating_sub(n))
                } else if let Ok(n) = s.parse::<usize>() {
                    Some(n.min(pool_len))
                } else {
                    None
                };
                if let Some(v) = newv {
                    target.store(v, Ordering::Relaxed);
                    manual.store(true, Ordering::Relaxed);
                }
            }
        });
    }

    // spawner：每 100ms 把 running 补齐到 target（批量 spawn 加速爬坡）
    {
        let ctx = ctx.clone();
        let target = target.clone();
        let running = running.clone();
        let free = free.clone();
        let accounts = accounts.clone();
        tokio::spawn(async move {
            loop {
                if ctx.shutdown.load(Ordering::Relaxed) {
                    break;
                }
                let t = target.load(Ordering::Relaxed);
                let mut r = running.load(Ordering::Relaxed);
                let mut spawned = 0;
                while r < t && spawned < 100 {
                    // 占坑
                    if running
                        .compare_exchange(r, r + 1, Ordering::SeqCst, Ordering::SeqCst)
                        .is_err()
                    {
                        r = running.load(Ordering::Relaxed);
                        continue;
                    }
                    let acct_idx = match free.lock().unwrap().pop() {
                        Some(i) => i,
                        None => {
                            running.fetch_sub(1, Ordering::Relaxed);
                            break;
                        }
                    };
                    let ctx2 = ctx.clone();
                    let target2 = target.clone();
                    let running2 = running.clone();
                    let free2 = free.clone();
                    let accounts2 = accounts.clone();
                    tokio::spawn(async move {
                        user_loop(ctx2, accounts2, acct_idx, target2, running2.clone()).await;
                        free2.lock().unwrap().push(acct_idx);
                    });
                    spawned += 1;
                    r = running.load(Ordering::Relaxed);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
    }

    // 仪表盘
    dashboard(ctx.clone(), target.clone(), running.clone(), pool_len, args.csv).await;
    ctx.shutdown.store(true, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(300)).await;
    println!("\n已退出。");
    Ok(())
}

/// 单个虚拟设备：不断跑学习会话，直到被缩容(running>target)或全局 shutdown。
async fn user_loop(
    ctx: Arc<RunCtx>,
    accounts: Arc<Vec<Account>>,
    acct_idx: usize,
    target: Arc<AtomicUsize>,
    running: Arc<AtomicUsize>,
) {
    let device_id = format!("ldt-{acct_idx:06}");
    let mut token = accounts[acct_idx].access_token.clone();
    let mut rng = Rng(0x9E3779B97F4A7C15 ^ ((acct_idx as u64).wrapping_mul(0x2545F4914F6CDD1D) | 1));
    let mut session_seq: u64 = 0;

    loop {
        // 缩容判定：running > target 时本任务带 CAS 退休，保证不过冲
        loop {
            let r = running.load(Ordering::Relaxed);
            let t = target.load(Ordering::Relaxed);
            if ctx.shutdown.load(Ordering::Relaxed) {
                running.fetch_sub(1, Ordering::Relaxed);
                return;
            }
            if r > t {
                if running
                    .compare_exchange(r, r - 1, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    return;
                }
                // CAS 失败(多任务争抢同一原子)：让出后重试，避免忙等自旋占满 worker
                tokio::task::yield_now().await;
            } else {
                break;
            }
        }

        session_seq += 1;
        if !run_one_session(&ctx, &device_id, &mut token, &accounts[acct_idx], &mut rng, session_seq)
            .await
        {
            // token 失效已在内部重登；其它错误小憩避免打爆
            tokio::time::sleep(Duration::from_millis(rng.range(200, 800))).await;
        }

        if ctx.mode == Mode::Realistic {
            // 会话间歇
            tokio::time::sleep(Duration::from_millis(rng.range(800, 2500))).await;
        }
    }
}

/// 跑一个完整学习会话。返回 false 表示中途出错（已记指标）。
async fn run_one_session(
    ctx: &RunCtx,
    device_id: &str,
    token: &mut String,
    account: &Account,
    rng: &mut Rng,
    session_seq: u64,
) -> bool {
    let m = &ctx.metrics;
    // 1) 建会话
    let sess = match post(
        &ctx.cli,
        &ctx.base,
        "/api/learning/session",
        Some(token),
        device_id,
        json!({ "targetMasteryCount": ctx.target_mastery }),
        Ep::Session,
        m,
    )
    .await
    {
        Ok(v) => v,
        Err(st) => {
            // 仅在真为 401/403(token 过期/无效)时重登；5xx/超时/429 不重登，
            // 否则拐点附近的失败会被误判为过期、放大 auth 流量污染 RPS 曲线。
            if matches!(st, Some(401) | Some(403)) {
                maybe_relogin(ctx, device_id, token, account).await;
            }
            return false;
        }
    };
    let session_id = match sess.get("sessionId").and_then(|x| x.as_str()) {
        Some(s) => s.to_string(),
        None => return false,
    };

    // 2) 取一批词
    let next = post(
        &ctx.cli,
        &ctx.base,
        "/api/learning/next-words",
        Some(token),
        device_id,
        json!({
            "sessionId": session_id,
            "batchIndex": 0,
            "excludeWordIds": [],
            "masteredWordIds": [],
            "sessionPerformance": {
                "recentAccuracy": 0.0,
                "masteredCount": 0,
                "targetMasteryCount": ctx.target_mastery,
                "errorProneWordIds": []
            }
        }),
        Ep::NextWords,
        m,
    )
    .await
    .ok();
    let words: Vec<String> = next
        .as_ref()
        .and_then(|v| v.get("words"))
        .and_then(|w| w.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|w| w.get("id").and_then(|x| x.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    if words.is_empty() {
        // 服务器无内容/取词失败：直接收尾，避免空转
        complete_session(ctx, device_id, token, &session_id, &[], rng).await;
        return false;
    }

    // 3) 逐词答题
    let mut events: Vec<Value> = Vec::new();
    let mut mastered: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut event_seq = 0u64;
    let mut total_questions = 0u32;
    let mut sum_rt = 0u64;

    for wid in &words {
        // pick-next-word（复刻真实时序，加压）
        let _ = post(
            &ctx.cli,
            &ctx.base,
            "/api/learning/pick-next-word",
            Some(token),
            device_id,
            json!({
                "activeWordIds": words,
                "errorWordIds": errors,
                "lastShownMap": {},
                "priorityMap": {}
            }),
            Ep::Pick,
            m,
        )
        .await;

        // generate-options
        let _ = post(
            &ctx.cli,
            &ctx.base,
            "/api/learning/generate-options",
            Some(token),
            device_id,
            json!({
                "wordId": wid,
                "mode": "word-to-meaning",
                "poolWordIds": words
            }),
            Ep::GenOptions,
            m,
        )
        .await;

        // 模拟作答：~78% 正确
        let response_time = rng.range(600, 4000);
        sum_rt += response_time;
        let is_correct = rng.frac() < 0.78;
        if is_correct {
            mastered.push(wid.clone());
        } else if !errors.contains(wid) {
            errors.push(wid.clone());
        }
        event_seq += 1;
        total_questions += 1;
        events.push(json!({
            "clientEventId": format!("{device_id}-{session_seq}-{event_seq}"),
            "wordId": wid,
            "isCorrect": is_correct,
            "responseTimeMs": response_time,
            "clientTsMs": now_ms()
        }));

        // think-time
        if ctx.mode == Mode::Realistic {
            tokio::time::sleep(Duration::from_millis(rng.range(ctx.think_min, ctx.think_max))).await;
        }

        // 周期 sync
        if events.len() >= ctx.sync_every {
            let batch = std::mem::take(&mut events);
            let _ = post(
                &ctx.cli,
                &ctx.base,
                "/api/learning/sync-progress",
                Some(token),
                device_id,
                json!({
                    "sessionId": session_id,
                    "totalQuestions": total_questions,
                    "contextShifts": total_questions / 5,
                    "events": batch
                }),
                Ep::Sync,
                m,
            )
            .await;
        }

        if ctx.shutdown.load(Ordering::Relaxed) {
            break;
        }
    }

    // 4) 收尾
    let avg = if total_questions > 0 {
        sum_rt / total_questions as u64
    } else {
        1800
    };
    complete_session_full(
        ctx, device_id, token, &session_id, &mastered, &errors, &events, avg,
    )
    .await;
    true
}

async fn complete_session(
    ctx: &RunCtx,
    device_id: &str,
    token: &str,
    session_id: &str,
    events: &[Value],
    _rng: &mut Rng,
) {
    complete_session_full(ctx, device_id, token, session_id, &[], &[], events, 1800).await;
}

#[allow(clippy::too_many_arguments)]
async fn complete_session_full(
    ctx: &RunCtx,
    device_id: &str,
    token: &str,
    session_id: &str,
    mastered: &[String],
    errors: &[String],
    events: &[Value],
    avg: u64,
) {
    let _ = post(
        &ctx.cli,
        &ctx.base,
        "/api/learning/complete-session",
        Some(token),
        device_id,
        json!({
            "sessionId": session_id,
            "masteredWordIds": mastered,
            "errorProneWordIds": errors,
            "avgResponseTimeMs": avg,
            "events": events
        }),
        Ep::Complete,
        &ctx.metrics,
    )
    .await;
}

/// token 过期时重登换新 access token。
async fn maybe_relogin(ctx: &RunCtx, device_id: &str, token: &mut String, account: &Account) {
    let v = post(
        &ctx.cli,
        &ctx.base,
        "/api/auth/login",
        None,
        device_id,
        json!({ "email": account.email, "password": account.password }),
        Ep::Auth,
        &ctx.metrics,
    )
    .await
    .ok();
    if let Some(at) = v
        .as_ref()
        .and_then(|d| d.get("accessToken"))
        .and_then(|x| x.as_str())
    {
        *token = at.to_string();
    }
}

// ============================ 仪表盘 ============================

async fn dashboard(
    ctx: Arc<RunCtx>,
    target: Arc<AtomicUsize>,
    running: Arc<AtomicUsize>,
    pool_len: usize,
    csv: Option<String>,
) {
    let m = &ctx.metrics;
    let start = Instant::now();
    let mut last_total = 0u64;
    let mut last_t = Instant::now();
    let mut csv_file = csv.as_ref().map(|p| {
        let exists = std::path::Path::new(p).exists();
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
            .expect("open csv");
        if !exists {
            use std::io::Write;
            let _ = writeln!(
                f,
                "elapsed_s,running,target,rps,total_reqs,total_errs,err_pct,p50,p95,p99,throttled"
            );
        }
        f
    });

    let mut tick = tokio::time::interval(Duration::from_secs(1));
    loop {
        tick.tick().await;
        if ctx.shutdown.load(Ordering::Relaxed) {
            break;
        }
        let now = Instant::now();
        let total = m.total_reqs();
        let errs = m.total_errs();
        let dt = now.duration_since(last_t).as_secs_f64().max(0.001);
        let rps = (total - last_total) as f64 / dt;
        last_total = total;
        last_t = now;

        // 总体延迟分位（聚合各端点直方图）
        let mut agg = vec![0u64; NUM_BINS];
        let mut agg_total = 0u64;
        for h in &m.hist {
            let (bins, t) = h.snapshot();
            for (i, c) in bins.iter().enumerate() {
                agg[i] += c;
            }
            agg_total += t;
        }
        let p50 = percentile(&agg, agg_total, 0.50);
        let p95 = percentile(&agg, agg_total, 0.95);
        let p99 = percentile(&agg, agg_total, 0.99);
        let err_pct = if total > 0 {
            errs as f64 / total as f64 * 100.0
        } else {
            0.0
        };
        let throttled = m.throttled.load(Ordering::Relaxed);
        let elapsed = start.elapsed().as_secs();

        // 清屏重绘
        print!("\x1b[2J\x1b[H");
        println!(
            "WordForge 承载压测  模式={}  运行 {}s",
            match ctx.mode { Mode::Raw => "raw", Mode::Realistic => "realistic" },
            elapsed
        );
        println!("──────────────────────────────────────────────");
        println!(
            "并发设备: {:>5} / 目标 {:>5} / 池上限 {:>5}",
            running.load(Ordering::Relaxed),
            target.load(Ordering::Relaxed),
            pool_len
        );
        println!(
            "RPS: {:>8.0}   总请求 {:>10}   错误 {:>8} ({:.2}%)   限流 {}",
            rps, total, errs, err_pct, throttled
        );
        println!(
            "延迟(全端点·仅成功)  p50 {:>5}ms   p95 {:>6}ms   p99 {:>6}ms",
            p50, p95, p99
        );
        println!("──────────────────────────────────────────────");
        println!(
            "{:<12} {:>10} {:>8} {:>7} {:>7} {:>7}",
            "endpoint", "reqs", "errs", "p50", "p95", "p99"
        );
        for i in 0..EP_COUNT {
            let (bins, t) = m.hist[i].snapshot();
            let r = m.reqs[i].load(Ordering::Relaxed);
            let e = m.errs[i].load(Ordering::Relaxed);
            if r == 0 {
                continue;
            }
            // 成功样本为 0 时分位无意义，渲染 '-' 而非误导性的 0ms
            let (c50, c95, c99) = if t == 0 {
                ("-".to_string(), "-".to_string(), "-".to_string())
            } else {
                (
                    format!("{}ms", percentile(&bins, t, 0.50)),
                    format!("{}ms", percentile(&bins, t, 0.95)),
                    format!("{}ms", percentile(&bins, t, 0.99)),
                )
            };
            println!(
                "{:<12} {:>10} {:>8} {:>7} {:>7} {:>7}",
                EP_NAMES[i], r, e, c50, c95, c99
            );
        }
        // 错误码 top
        {
            let codes = m.err_codes.lock().unwrap();
            if !codes.is_empty() {
                let mut v: Vec<_> = codes.iter().collect();
                v.sort_by(|a, b| b.1.cmp(a.1));
                let top: Vec<String> =
                    v.iter().take(5).map(|(k, c)| format!("{k}={c}")).collect();
                println!("──────────────────────────────────────────────");
                println!("错误码: {}", top.join("  "));
            }
        }
        println!("──────────────────────────────────────────────");
        println!("[数字]设并发  [+N]加  [-N]减  [q]退出");

        if let Some(f) = csv_file.as_mut() {
            use std::io::Write;
            let _ = writeln!(
                f,
                "{},{},{},{:.0},{},{},{:.2},{},{},{},{}",
                elapsed,
                running.load(Ordering::Relaxed),
                target.load(Ordering::Relaxed),
                rps,
                total,
                errs,
                err_pct,
                p50,
                p95,
                p99,
                throttled
            );
        }
    }
}

// ============================ main ============================

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // 仪表盘每秒清屏，子任务 panic 易被淹没；落盘到文件兜底。
    std::panic::set_hook(Box::new(|info| {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("loadtest-panic.log")
        {
            let _ = writeln!(f, "[{}] {info}", now_ms());
        }
        eprintln!("panic: {info}");
    }));

    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Provision(a) => provision(a).await,
        Cmd::Run(a) => run(a).await,
    }
}
