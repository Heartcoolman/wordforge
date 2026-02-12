use std::net::SocketAddr;
use std::sync::Arc;

use axum::http::{header, HeaderName, HeaderValue};
use learning_backend::amas::config::AMASConfig;
use learning_backend::amas::engine::AMASEngine;
use learning_backend::config::Config;
use learning_backend::logging::{init_tracing, LogConfig};
use learning_backend::routes::build_router;
use learning_backend::services::llm_provider::LlmProvider;
use learning_backend::state::AppState;
use learning_backend::store::Store;
use learning_backend::workers::WorkerManager;
use tokio::sync::broadcast;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

const CSP_HEADER: &str = "default-src 'self'; script-src 'self'; style-src 'self' https://fonts.googleapis.com; font-src https://fonts.gstatic.com; connect-src 'self'; img-src 'self' data: blob:; worker-src 'self' blob:; frame-ancestors 'none'; base-uri 'self'; form-action 'self'";
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

    let store = Arc::new(Store::open(&config.sled_path).expect("Failed to open sled database"));
    store.run_migrations().expect("Failed to run migrations");

    let (shutdown_tx, _) = broadcast::channel::<()>(8);

    let amas_config = AMASConfig::from_env(&config.amas);
    let amas_engine = Arc::new(AMASEngine::new(amas_config, store.clone()));

    let state = AppState::new(
        store.clone(),
        amas_engine.clone(),
        &config,
        shutdown_tx.clone(),
    );

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

    tracing::info!("Flushing store before exit");
    if let Err(e) = store.flush() {
        tracing::error!(error = %e, "Failed to flush store before exit");
    }
    tracing::info!("Shutdown complete");
}

fn build_cors_layer(config: &Config) -> CorsLayer {
    if config.cors_origin.trim() == "*" {
        // 通配符模式仅用于开发环境，通配符与 credentials 互斥
        return CorsLayer::new()
            .allow_origin(Any)
            .allow_credentials(false)
            .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT])
            .allow_methods(Any);
    }

    match config.cors_origin.parse::<axum::http::HeaderValue>() {
        Ok(origin) => CorsLayer::new()
            .allow_origin(origin)
            .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT])
            .allow_methods(Any),
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
