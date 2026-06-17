use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::http::{header, HeaderName, HeaderValue, Method};
use learning_backend::amas::config::AMASConfig;
use learning_backend::amas::engine::AMASEngine;
use learning_backend::config::Config;
use learning_backend::logging::{init_tracing, LogConfig};
use learning_backend::middleware::rate_limit::{
    auth_rate_limit_cleanup_loop, rate_limit_cleanup_loop,
};
use learning_backend::routes::build_router;
use learning_backend::services::llm_provider::LlmProvider;
use learning_backend::state::AppState;
use learning_backend::store::Store;
use learning_backend::workers::WorkerManager;
use tokio::sync::broadcast;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

const CSP_HEADER: &str = "default-src 'self'; script-src 'self' 'sha256-wEjozNdwHz/9ujnOuYJi4PZ89BSuTa/abtYO9C7bcNw='; style-src 'self'; font-src 'self'; connect-src 'self' https: capacitor: ionic:; img-src 'self' data: blob:; worker-src 'self' blob:; frame-ancestors 'none'; base-uri 'self'; form-action 'self'";
const HSTS_HEADER: &str = "max-age=31536000; includeSubDomains";

/// v1.1.0-beta.2：自更新子进程 stderr 救场。
///
/// `updater::spawn_replacement` 起新进程时把 stdin/stdout/stderr 全 redirect 到
/// `Stdio::null()`（v1.0 编译时写死，无法回头修），导致升级失败时新进程的 panic /
/// migrate 错误**完全无 trace**（v1.0 → v1.1.0-beta.1 升级失败的诊断盲区根因）。
///
/// 本函数在 main 第一行就跑：通过 `/proc/<ppid>/cmdline` 检测是否被自更新 sh wrapper 起
/// （wrapper 的 argv[1] 是固定字符串 `wordforge-restart`），如果是 → 立即 dup2 stdout/stderr
/// 到 `<install_dir>/logs/updater-child-<unix_ts>.log`。非自更新启动（systemd fresh start）
/// 不改 stderr，让 journal 继续接管。
#[cfg(unix)]
fn redirect_self_update_logs_if_applicable() {
    let ppid = unsafe { libc::getppid() };
    let cmdline_path = format!("/proc/{ppid}/cmdline");
    let cmdline = match std::fs::read_to_string(&cmdline_path) {
        Ok(s) => s,
        Err(_) => return,
    };
    // /proc/<pid>/cmdline 是 \0 分隔的 argv 拼串；wrapper 第二段是 "wordforge-restart"
    if !cmdline.contains("wordforge-restart") {
        return;
    }

    let log_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("logs")))
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    let _ = std::fs::create_dir_all(&log_dir);

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let log_path = log_dir.join(format!("updater-child-{ts}.log"));

    let file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        Ok(f) => f,
        Err(_) => return,
    };

    use std::os::unix::io::AsRawFd;
    let fd = file.as_raw_fd();
    unsafe {
        // 1=STDOUT_FILENO, 2=STDERR_FILENO
        let _ = libc::dup2(fd, 1);
        let _ = libc::dup2(fd, 2);
    }
    // file 离开 scope 关闭原 fd，但 stdout/stderr 已经各持一个 dup 出来的独立 fd
    drop(file);

    eprintln!(
        "[updater-child] self-update child detected (ppid={ppid} cmdline contains 'wordforge-restart'), logs at {log_path:?}"
    );
}

#[cfg(not(unix))]
fn redirect_self_update_logs_if_applicable() {
    // 非 unix（Windows/其他）平台无 /proc 也无 dup2 语义，跳过。
}

/// C(v1.2.0-beta.11) 升级冒烟自检：被 `apply` 以 `--selfcheck` 调起。
///
/// 设计为「严格但零副作用」：
/// 1. 能执行到这里即证明二进制在本机 arch/glibc 成功加载运行（捕获 wrong-arch 的
///    `Exec format error`、缺 glibc 符号、损坏导致的 SIGILL/SIGSEGV——这些在 exec/加载期
///    就让进程异常退出，永远到不了这里）。
/// 2. `Config::from_env()` 解析 + 校验 env 配置（坏配置 panic → 进程非 0 退出 → apply 拦截）。
/// 3. 关键字段健全性校验 + AMAS 配置可构造。
///
/// 全程**不开库、不绑端口、不连网、不起 worker**。通过 → 打印 JSON 退 0；否则非 0 退出，
/// apply 据非 0 退出码判定坏包，在进维护态 / 换文件之前中止本次升级，现役无损。
fn run_selfcheck() {
    let version = env!("GIT_VERSION");
    // Config::from_env 内含 env 解析与校验；坏配置直接 panic → 非 0 退出 → apply 拦截。
    let config = Config::from_env();
    let mut problems: Vec<String> = Vec::new();
    if config.database_url.trim().is_empty() {
        problems.push("database_url 为空".into());
    }
    if config.port == 0 {
        problems.push("port 非法(0)".into());
    }
    if version.trim().is_empty() {
        problems.push("GIT_VERSION 为空".into());
    }
    // AMAS 配置构造（捕获结构性问题；from_env 内部对非法值有兜底，主要验证不 panic）。
    let _amas = AMASConfig::from_env(&config.amas);
    if problems.is_empty() {
        println!("{{\"selfcheck\":\"ok\",\"version\":\"{version}\"}}");
    } else {
        eprintln!("selfcheck FAILED: {}", problems.join("; "));
        std::process::exit(1);
    }
}

#[tokio::main]
async fn main() {
    // v1.1.0-beta.2：先做自更新子进程 stderr 救场（必须在 dotenvy / Config::from_env 之前，
    // 这样如果 Config::from_env 内 validate_secrets 等校验 panic，消息会落到日志文件而不是
    // 被 spawn_replacement 的 Stdio::null() 吞掉）。
    redirect_self_update_logs_if_applicable();

    dotenvy::dotenv().ok();

    // C(v1.2.0-beta.11) 升级冒烟自检：apply 在换二进制前以 `--selfcheck` 试跑新二进制。
    // 仅验证能在本机 arch/glibc 加载运行 + 解析 env 配置，绝不开库 / 绑端口 / 连网，秒退；
    // 任一步失败非 0 退出，apply 据此在进维护态 / 换文件之前判定坏包并中止。必须在重活之前。
    if std::env::args().any(|a| a == "--selfcheck") {
        run_selfcheck();
        return;
    }

    let config = Config::from_env();

    init_tracing(&LogConfig {
        log_level: config.log_level.clone(),
        enable_file_logs: config.enable_file_logs,
        log_dir: config.log_dir.clone(),
    });
    tracing::info!("Starting learning-backend");

    // Validate LLM config at startup (panics if enabled=true, mock=false)
    LlmProvider::validate_config(&config.llm);

    // v1.2.0-beta.8 最强回滚落地：若上次回滚 apply 暂存了目标版本 DB（<db>.rollback-pending），
    // 在打开库之前、无任何连接持库的此刻把它落地为现役库，规避 WAL 串扰。须在 Store::open 之前。
    learning_backend::services::updater::apply_pending_rollback_db(&config.database_url);

    let store = Arc::new(
        Store::open_with_connection_timeout(
            &config.database_url,
            config.sqlite_busy_timeout_ms,
            config.sqlite_pool_size,
            config.sqlite_connection_timeout_ms,
        )
        .expect("Failed to open SQLite database"),
    );
    store.run_migrations().expect("Failed to run migrations");

    // 启动期对账升级审计：成功的自更新 / 回滚因 exit(0) 重启来不及收尾，残留 in_progress → 标 success。
    match store.reconcile_stale_update_audits() {
        Ok(n) if n > 0 => tracing::info!(count = n, "对账：残留 in_progress 升级审计收尾为 success"),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "升级审计对账失败"),
    }

    // v1.2.0-beta.8 启动自愈：若 static/ 缺失（被打断的 swap / 级联回滚残留），从最近的
    // static.<tag> 备份恢复，避免 admin UI 根路径 404、后台彻底打不开。仅在 static/index.html
    // 不存在时触发；优先非 .failed 备份，按 mtime 取最新。
    heal_missing_static_dir(std::path::Path::new("static"));

    // m036:把持久化的运行时热更设置(live-ratelimit/limits/auth)覆盖到 env 构造的 config 上，
    // 使 admin 在系统设置改过的限流/配额/令牌 TTL 跨重启保留（AppState 持其热替换快照）。
    let config = learning_backend::routes::admin::runtime_settings::overlay_persisted_live_sections(
        config, &store,
    );

    // D3：回灌持久化的可用率小时桶，恢复跨重启的 SLO 30d 窗口（失败仅 warn 不阻断启动）。
    {
        let now_hour = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64 / 3600)
            .unwrap_or(0);
        match store.load_availability_rollup(now_hour - 30 * 24) {
            Ok(rows) => {
                let n = rows.len();
                learning_backend::middleware::http_metrics::import_hour_rollup(rows);
                if n > 0 {
                    tracing::info!(hours = n, "回灌可用率小时桶，SLO 窗口跨重启恢复");
                }
            }
            Err(e) => tracing::warn!(error = %e, "回灌可用率小时桶失败"),
        }
    }

    // C2：启动后 seed AMAS 调参白名单（空表才写自 const，失败仅 warn 不阻断启动）
    if let Err(e) = store.seed_tuning_whitelist_if_empty() {
        tracing::warn!(error = %e, "启动 seed AMAS 调参白名单失败，回退 const fallback");
    }

    learning_backend::blocking::init_blocking_semaphore(config.sqlite_pool_size as usize);

    let (shutdown_tx, _) = broadcast::channel::<()>(8);

    let mut amas_config = AMASConfig::from_env(&config.amas);
    if let Some(ref path) = config.amas_config_file {
        match AMASConfig::load_from_toml(path) {
            Ok(cfg) => {
                tracing::info!(path = %path, "从 TOML 文件加载 AMAS 配置");
                amas_config = cfg;
            }
            Err(e) => tracing::warn!(path = %path, error = %e, "加载 TOML 配置失败，使用默认值"),
        }
    } else {
        let default_path = "amas_config.toml";
        if std::path::Path::new(default_path).exists() {
            match AMASConfig::load_from_toml(default_path) {
                Ok(cfg) => {
                    tracing::info!(path = %default_path, "从默认路径加载 AMAS 配置");
                    amas_config = cfg;
                }
                Err(e) => {
                    tracing::warn!(path = %default_path, error = %e, "加载默认 TOML 配置失败，使用内置默认值")
                }
            }
        } else if let Err(e) = amas_config.write_to_toml(default_path) {
            tracing::warn!(error = %e, "写出默认 TOML 配置失败");
        } else {
            tracing::info!(path = %default_path, "已生成默认 AMAS 配置文件");
        }
    }
    // 启动期信任门：TOML 加载路径绕过了 validate()，未校验配置会让 AMASEngine::new 构建出
    // 含 NaN/Inf/反转曲线/越界 clamp 边界的引擎，污染调度并在首次复习/记录写入时 panic。
    // 在此显式校验，非法配置直接拒绝启动（fail loud），不带病上线。
    if let Err(e) = amas_config.validate() {
        panic!("AMAS 配置校验失败，拒绝启动: {e}");
    }
    let amas_engine = Arc::new(AMASEngine::new(amas_config, store.clone()));

    let initial_maintenance = store
        .get_system_settings()
        .map(|s| s.maintenance_mode)
        .unwrap_or(false);

    let mut state = AppState::new(
        store.clone(),
        amas_engine.clone(),
        &config,
        shutdown_tx.clone(),
        initial_maintenance,
    );

    // m027：GeoIP（可选）。data/GeoLite2-Country.mmdb 在 binary 同目录或 cwd 都试一次。
    {
        let candidates = [
            std::path::PathBuf::from("data/GeoLite2-Country.mmdb"),
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("data/GeoLite2-Country.mmdb")))
                .unwrap_or_default(),
        ];
        let reader = candidates
            .iter()
            .find(|p| p.exists())
            .and_then(learning_backend::services::geoip::try_load);
        state.set_geoip(reader);
    }
    let state = state;

    // v1.1-P0.6：资源包 Ed25519 签名器（与 db 同目录的 keys/）
    {
        let key_dir = std::path::Path::new(&config.database_url)
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join("keys");
        match learning_backend::services::resource_pack_signing::ResourcePackSigner::load_or_generate(
            &key_dir,
        ) {
            Ok(signer) => {
                tracing::info!(dir = ?key_dir, pubkey = signer.public_key_base64(), "资源包签名器就绪");
                state.set_resource_pack_signer(std::sync::Arc::new(signer));
            }
            Err(e) => {
                tracing::warn!(error = %e, dir = ?key_dir, "资源包签名器初始化失败，相关端点将返 503");
            }
        }
    }

    // 自更新服务（含缓存 + ETag），不打 GitHub，纯本地构造
    let updater = match learning_backend::services::updater::Updater::new(
        &config.update_check,
        env!("GIT_VERSION"),
    ) {
        Ok(u) => {
            // M0-R4：启动时检查 maintenance flag。上次自更新在 Swapping 阶段后崩溃
            // 或新进程健康检查超时回滚，父进程 exit 前无法清除 flag，由新进程代劳。
            let flag_path = u
                .install_dir()
                .join(learning_backend::services::updater::MAINTENANCE_FLAG);
            if flag_path.exists() {
                tracing::warn!(path = ?flag_path, "发现 maintenance flag，上次自更新异常；清理 maintenance 模式");
                state.set_maintenance(false);
                let _ = std::fs::remove_file(&flag_path);
            }
            state.set_updater(u.clone());
            Some(u)
        }
        Err(e) => {
            tracing::warn!(error = %e, "Updater 初始化失败，自更新功能禁用");
            None
        }
    };

    {
        let watch_path = config
            .amas_config_file
            .clone()
            .unwrap_or_else(|| "amas_config.toml".to_string());
        tokio::spawn(learning_backend::workers::config_watcher::run(
            watch_path,
            amas_engine.clone(),
        ));
    }

    // 心跳看门狗下发 DataCorrupted SSE + 推进 miss 计数，非幂等，仅 leader 跑，
    // 避免多实例对同一设备重复告警。
    if config.worker.is_leader {
        tokio::spawn(learning_backend::workers::heartbeat_watchdog::run(
            state.clone(),
            shutdown_tx.subscribe(),
        ));
    }

    // v1.1-P1 S2：领域事件总线 consumer。`event-bus` feature 关闭时 subscribe()
    // 返回 None，consumer 自动早退。AMAS 同步通路保留为主路径不变；这里只是
    // 旁路计数 / 日志，为后续 outbox + AMAS 真异步化打底。
    {
        let bus = state.event_bus().clone();
        let shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            learning_backend::services::event_bus::run_record_consumer(bus.as_ref(), shutdown_rx)
                .await;
        });
    }

    // S2-1：outbox 异步消费 worker（领域事件持久化处理 + 指数退避重试 + 死信兜底）。
    // 默认 records 走同步老路时 outbox 为空，本 loop 每 10s 一次空查询、零影响；opt-in
    // (RECORDS_OUTBOX_ASYNC=true) 后驱动异步消费，关闭后仍排空残留事件。需 AppState 故走 interval loop。
    // claim 非原子，多实例会重复处理同一事件致 AMAS 状态双累加，仅 leader 跑。
    if config.worker.is_leader {
        let outbox_state = state.clone();
        let mut shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(10));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            tick.tick().await; // 跳过启动即触发
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => break,
                    _ = tick.tick() => {
                        learning_backend::workers::outbox_processor::run(&outbox_state).await;
                    }
                }
            }
        });
    }

    tokio::spawn(learning_backend::workers::probe_confirm_sweeper::run(
        state.clone(),
        shutdown_tx.subscribe(),
    ));

    tokio::spawn(learning_backend::workers::probe_cleanup::run(
        state.clone(),
        shutdown_tx.subscribe(),
    ));

    // 遥测两表（telemetry_events / telemetry_summaries）24h retention 清理。
    // 清理非幂等，仅 leader 执行（守卫在 sweep_once 内部），避免多实例并发删同一批行。
    tokio::spawn(learning_backend::workers::telemetry_cleanup::run(
        state.clone(),
        shutdown_tx.subscribe(),
    ));

    // m042/D2:定时广播下发 worker（每 60s 扫到期 scheduled_broadcasts 并 fan-out）。
    // fan-out 非幂等，多实例会重复下发，仅 leader 跑。
    if config.worker.is_leader {
        tokio::spawn(learning_backend::workers::scheduled_broadcast::run(
            state.clone(),
            shutdown_tx.subscribe(),
        ));
    }

    // 时钟健康探测：启动时打一次（detached，不阻塞 listen），之后每小时再打。
    // 漂移超阈会 ERROR 日志告警 + `/health` 状态降级，详见 clock_health 模块文档。
    {
        let ch = state.clock_health().clone();
        tokio::spawn(async move {
            ch.probe_once().await;
        });
        let _periodic_handle = state.clock_health().spawn_periodic();
    }

    tokio::spawn(rate_limit_cleanup_loop(
        state.rate_limit().clone(),
        config.rate_limit.window_secs,
        shutdown_tx.subscribe(),
    ));
    tokio::spawn(auth_rate_limit_cleanup_loop(
        state.auth_rate_limit().clone(),
        config.auth_rate_limit.window_secs,
        shutdown_tx.subscribe(),
    ));
    // W3-1：遥测限频器独立 cleanup loop（与业务限流并列，周期=遥测窗口）。
    tokio::spawn(rate_limit_cleanup_loop(
        state.telemetry_rate_limit().clone(),
        config.telemetry_rate_limit.window_secs,
        shutdown_tx.subscribe(),
    ));

    // 每日 DB 备份（独立 interval 循环，与 cron worker 解耦）。
    // 备份 + 离站上传/prune 非幂等，多实例会并发上传/互删备份，仅 leader 跑。
    if config.worker.is_leader {
        let store = store.clone();
        let backups_dir = std::path::Path::new(&config.database_url)
            .parent()
            .map(|p| p.join("backups"));
        if let Some(dir) = backups_dir {
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(Duration::from_secs(24 * 3600));
                tick.tick().await; // 跳过启动即触发
                loop {
                    tick.tick().await;
                    match learning_backend::workers::db_backup::run_once(&store, &dir, 30).await {
                        Ok(path) => {
                            // B1:本地备份成功后逐 target 推送离站（file/rsync/s3），失败仅告警不中断
                            learning_backend::workers::backup_offsite::run(&store, &path).await;
                        }
                        Err(e) => tracing::warn!(error = %e, "每日 DB 备份失败"),
                    }
                }
            });
        }
    }

    let worker_handle = if config.worker.is_leader {
        let mut worker_manager = WorkerManager::new(
            store.clone(),
            amas_engine.clone(),
            shutdown_tx.subscribe(),
            &config.worker,
        )
        .with_llm_config(config.llm.clone())
        .with_llm_advisor_state(state.clone())
        // M0-P4 + M1-A5：注入 AppState 以启用 error_rate_watchdog 和调度器健康告警
        .with_watchdog_state(state.clone())
        .with_health_state(state.clone());
        if let Some(u) = updater.clone() {
            worker_manager = worker_manager.with_update_checker(
                u,
                state.clone(),
                config.update_check.worker_enabled,
            );
        }
        Some(tokio::spawn(async move {
            if let Err(e) = worker_manager.start().await {
                tracing::error!(error = %e, "Worker manager failed");
            }
        }))
    } else {
        None
    };

    let cors_layer = build_cors_layer(&config);

    let app = build_router(state.clone())
        .layer(cors_layer)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(CatchPanicLayer::new())
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("content-security-policy"),
            HeaderValue::from_static(CSP_HEADER),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("strict-transport-security"),
            HeaderValue::from_static(HSTS_HEADER),
        ));

    let addr = SocketAddr::new(config.host, config.port);
    let listener = bind_listener_with_retry(addr, 20, Duration::from_millis(250))
        .await
        .expect("Failed to bind TCP listener");
    tracing::info!(%addr, "Listening");
    spawn_self_watchdog(addr, &config.self_watchdog);

    let server_future = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(shutdown_tx.clone()));

    if let Some(handle) = worker_handle {
        // Worker 作为独立后台任务运行，panic 仅记录错误，不终止 HTTP 服务器
        tokio::spawn(async move {
            match handle.await {
                Err(e) => {
                    tracing::error!(error = %e, "Worker task panicked, HTTP server continues")
                }
                Ok(()) => tracing::info!("Worker manager exited normally"),
            }
        });
    }

    if let Err(e) = server_future.await {
        tracing::error!(error = %e, "HTTP server crashed");
    }

    // W3-2：关停前补一次可用率落盘。此刻 server_future 已返回、不再有新请求写内存 hour 桶，
    // 补落最近一次 cron flush（≤5min）到关停之间的请求/5xx 增量，消除重启后登录 SLO 桶缺口。
    learning_backend::workers::metrics_flush::flush_availability_rollup(state.store());
    tracing::info!("可用率小时桶已最终落盘");

    tracing::info!("Shutdown complete");
}

fn spawn_self_watchdog(addr: SocketAddr, config: &learning_backend::config::SelfWatchdogConfig) {
    if !config.enabled {
        return;
    }

    let probe_addr = loopback_addr_for(addr);
    let interval = Duration::from_secs(config.interval_secs.max(1));
    let threshold = config.failure_threshold.max(1);

    std::thread::Builder::new()
        .name("self-watchdog".to_string())
        .spawn(move || {
            let mut failures = 0_u32;
            std::thread::sleep(interval);

            loop {
                std::thread::sleep(interval);
                if http_liveness_probe(probe_addr, Duration::from_secs(3)) {
                    failures = 0;
                    continue;
                }

                failures = failures.saturating_add(1);
                tracing::error!(
                    %probe_addr,
                    failures,
                    threshold,
                    "Self watchdog liveness probe failed"
                );

                if failures >= threshold {
                    tracing::error!(
                        %probe_addr,
                        "Self watchdog aborting unresponsive process"
                    );
                    std::process::abort();
                }
            }
        })
        .expect("failed to spawn self watchdog");
}

/// 启动时绑定监听端口；若被旧进程短暂占用（自更新重启窗口期 / TIME_WAIT），
/// 按 `attempts × delay` 节奏重试。补 v0.4.4 dangling tag 的修复（main 未带）。
async fn bind_listener_with_retry(
    addr: SocketAddr,
    attempts: usize,
    delay: Duration,
) -> std::io::Result<tokio::net::TcpListener> {
    let mut last_err = None;
    for attempt in 1..=attempts.max(1) {
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => return Ok(listener),
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse && attempt < attempts => {
                tracing::warn!(
                    %addr,
                    attempt,
                    attempts,
                    "TCP listener address is still in use; retrying"
                );
                last_err = Some(e);
                tokio::time::sleep(delay).await;
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_err
        .unwrap_or_else(|| std::io::Error::new(std::io::ErrorKind::AddrInUse, "address in use")))
}

/// v1.2.0-beta.8 启动自愈：`static/` 缺失（被打断的 swap / 级联回滚消耗了备份）时，
/// 从最近的 `static.<tag>` 备份目录 rename 就位，避免 admin UI 根路径 404、后台打不开。
/// 仅在 `static/index.html` 不存在时触发；优先非 `.failed` 备份、按 mtime 取最新。
/// 消耗一个 static 备份无碍——回滚 apply 会从 tarball 重新解出 static。失败仅 warn 不阻断启动。
fn heal_missing_static_dir(static_path: &std::path::Path) {
    if static_path.join("index.html").is_file() {
        return;
    }
    let dir = static_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let prefix = format!(
        "{}.",
        static_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("static")
    );
    let mut candidates: Vec<(std::time::SystemTime, bool, std::path::PathBuf)> =
        std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                let name = e.file_name().to_string_lossy().into_owned();
                if !p.is_dir() || !name.starts_with(&prefix) || !p.join("index.html").is_file() {
                    return None;
                }
                let mtime = e.metadata().ok()?.modified().ok()?;
                Some((mtime, name.ends_with(".failed"), p))
            })
            .collect();
    if candidates.is_empty() {
        tracing::warn!("static/ 缺失且无可用 static.<tag> 备份，admin UI 将不可用");
        return;
    }
    // 非 .failed 优先（false<true），同类按 mtime 新→旧
    candidates.sort_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)));
    let src = candidates[0].2.clone();
    match std::fs::rename(&src, static_path) {
        Ok(()) => tracing::info!(restored_from = ?src, "启动自愈：从备份恢复缺失的 static/"),
        Err(e) => tracing::error!(error = %e, src = ?src, "启动自愈恢复 static/ 失败"),
    }
}

fn loopback_addr_for(addr: SocketAddr) -> SocketAddr {
    let ip = match addr.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    SocketAddr::new(ip, addr.port())
}

fn http_liveness_probe(addr: SocketAddr, timeout: Duration) -> bool {
    use std::io::{Read, Write};

    let mut stream = match std::net::TcpStream::connect_timeout(&addr, timeout) {
        Ok(stream) => stream,
        Err(_) => return false,
    };

    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));

    if stream
        .write_all(b"GET /health/live HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }

    let mut buf = [0_u8; 64];
    match stream.read(&mut buf) {
        Ok(n) => {
            let response = &buf[..n];
            response.starts_with(b"HTTP/1.1 200") || response.starts_with(b"HTTP/1.0 200")
        }
        Err(_) => false,
    }
}

fn build_cors_layer(config: &Config) -> CorsLayer {
    let origin_str = config.cors_origin.trim();
    // 跨域时浏览器默认不允许 JS 读非简单响应头，要让前端读到 X-Server-Time 做时钟对齐，
    // 必须显式 expose（见 middleware/server_time.rs）。
    let expose_headers = [HeaderName::from_static("x-server-time")];

    if origin_str == "*" {
        return CorsLayer::new()
            .allow_origin(Any)
            .allow_credentials(false)
            .allow_headers([
                header::AUTHORIZATION,
                header::CONTENT_TYPE,
                header::ACCEPT,
                HeaderName::from_static("x-device-id"),
                HeaderName::from_static("x-device-platform"),
            ])
            .expose_headers(expose_headers)
            .allow_methods(Any);
    }

    if origin_str.contains(',') {
        let origins: Vec<HeaderValue> = origin_str
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.parse::<HeaderValue>().unwrap_or_else(|e| {
                    panic!("FATAL: Invalid origin '{}' in CORS_ORIGIN: {}", s, e);
                })
            })
            .collect();

        if origins.is_empty() {
            panic!("FATAL: CORS_ORIGIN contains no valid origins");
        }

        return CorsLayer::new()
            .allow_origin(origins)
            .allow_credentials(true)
            .allow_headers([
                header::AUTHORIZATION,
                header::CONTENT_TYPE,
                header::ACCEPT,
                HeaderName::from_static("x-device-id"),
                HeaderName::from_static("x-device-platform"),
            ])
            .expose_headers(expose_headers)
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::PATCH,
                Method::OPTIONS,
            ]);
    }

    match origin_str.parse::<HeaderValue>() {
        Ok(origin) => CorsLayer::new()
            .allow_origin(origin)
            .allow_credentials(true)
            .allow_headers([
                header::AUTHORIZATION,
                header::CONTENT_TYPE,
                header::ACCEPT,
                HeaderName::from_static("x-device-id"),
                HeaderName::from_static("x-device-platform"),
            ])
            .expose_headers(expose_headers)
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::DELETE,
                Method::PATCH,
                Method::OPTIONS,
            ]),
        Err(e) => {
            panic!(
                "FATAL: Invalid CORS_ORIGIN '{}': {}. \
                 Fix the CORS_ORIGIN environment variable.",
                config.cors_origin, e
            );
        }
    }
}

async fn shutdown_signal(shutdown_tx: broadcast::Sender<()>) {
    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = sigterm.recv() => {},
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }

    tracing::info!("Shutdown signal received");
    let _ = shutdown_tx.send(());
}
