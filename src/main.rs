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

#[tokio::main]
async fn main() {
    // v1.1.0-beta.2：先做自更新子进程 stderr 救场（必须在 dotenvy / Config::from_env 之前，
    // 这样如果 Config::from_env 内 validate_secrets 等校验 panic，消息会落到日志文件而不是
    // 被 spawn_replacement 的 Stdio::null() 吞掉）。
    redirect_self_update_logs_if_applicable();

    dotenvy::dotenv().ok();

    let config = Config::from_env();

    init_tracing(&LogConfig {
        log_level: config.log_level.clone(),
        enable_file_logs: config.enable_file_logs,
        log_dir: config.log_dir.clone(),
    });
    tracing::info!("Starting learning-backend");

    // Validate LLM config at startup (panics if enabled=true, mock=false)
    LlmProvider::validate_config(&config.llm);

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
    let amas_engine = Arc::new(AMASEngine::new(amas_config, store.clone()));

    let initial_maintenance = store
        .get_system_settings()
        .map(|s| s.maintenance_mode)
        .unwrap_or(false);

    let state = AppState::new(
        store.clone(),
        amas_engine.clone(),
        &config,
        shutdown_tx.clone(),
        initial_maintenance,
    );

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
            let flag_path = u.install_dir().join(learning_backend::services::updater::MAINTENANCE_FLAG);
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

    tokio::spawn(learning_backend::workers::heartbeat_watchdog::run(
        state.clone(),
        shutdown_tx.subscribe(),
    ));

    // v1.1-P1 S2：领域事件总线 consumer。`event-bus` feature 关闭时 subscribe()
    // 返回 None，consumer 自动早退。AMAS 同步通路保留为主路径不变；这里只是
    // 旁路计数 / 日志，为后续 outbox + AMAS 真异步化打底。
    {
        let bus = state.event_bus().clone();
        let shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            learning_backend::services::event_bus::run_record_consumer(
                bus.as_ref(),
                shutdown_rx,
            )
            .await;
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

    let app = build_router(state)
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
