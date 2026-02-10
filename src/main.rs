use std::net::SocketAddr;
use std::sync::Arc;

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

    let app = build_router(state)
        .layer(tower_http::cors::CorsLayer::permissive())
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let addr = SocketAddr::new(config.host, config.port);
    tracing::info!(%addr, "Listening");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind TCP listener");

    let server_future = axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal(shutdown_tx.clone()));

    if let Some(handle) = worker_handle {
        tokio::select! {
            result = server_future => {
                if let Err(e) = result {
                    tracing::error!(error = %e, "HTTP server crashed");
                }
            }
            result = handle => {
                match result {
                    Err(e) => tracing::error!(error = %e, "Worker task panicked"),
                    Ok(()) => tracing::info!("Worker manager exited"),
                }
            }
        }
    } else {
        server_future.await.expect("HTTP server crashed");
    }

    tracing::info!("Flushing store before exit");
    let _ = store.flush();
    tracing::info!("Shutdown complete");
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
