use std::net::SocketAddr;
use std::sync::Arc;

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

#[tokio::main]
async fn main() {
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
        Store::open(
            &config.database_url,
            config.sqlite_busy_timeout_ms,
            config.sqlite_pool_size,
        )
        .expect("Failed to open SQLite database"),
    );
    store.run_migrations().expect("Failed to run migrations");

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
        let worker_manager = WorkerManager::new(
            store.clone(),
            amas_engine.clone(),
            shutdown_tx.subscribe(),
            &config.worker,
        );
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
    tracing::info!(%addr, "Listening");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind TCP listener");

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
