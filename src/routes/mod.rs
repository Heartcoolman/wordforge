pub mod admin;
pub mod auth;
pub mod content;
pub mod health;
pub mod learning;
pub mod notifications;
pub mod realtime;
pub mod records;
pub mod study_config;
pub mod user_profile;
pub mod users;
pub mod v1;
pub mod word_states;
pub mod wordbooks;
pub mod words;

use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{Json, Router};
use tower_http::services::{ServeDir, ServeFile};

use crate::middleware::{rate_limit, request_id};
use crate::response::ErrorBody;
use crate::state::AppState;

/// Maximum request body size: 2 MiB.
const MAX_BODY_SIZE: usize = 2 * 1024 * 1024;

pub fn build_router(state: AppState) -> Router {
    let api_routes = Router::new()
        .nest("/auth", auth::router())
        .nest("/users", users::router())
        .nest("/words", words::router())
        .nest("/records", records::router())
        .nest("/amas", admin::amas::router())
        .nest("/admin", admin::router())
        .nest("/realtime", realtime::router())
        .nest("/wordbooks", wordbooks::router())
        .nest("/study-config", study_config::router())
        .nest("/learning", learning::router())
        .nest("/word-states", word_states::router())
        .nest("/user-profile", user_profile::router())
        .nest("/notifications", notifications::router())
        .nest("/content", content::router())
        .nest("/v1", v1::router())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            rate_limit::rate_limit_middleware,
        ))
        .layer(DefaultBodyLimit::max(MAX_BODY_SIZE));

    // B29: Static file serving with SPA fallback
    let spa_fallback = ServeDir::new("static").not_found_service(ServeFile::new("static/index.html"));

    Router::new()
        .nest("/api", api_routes)
        .nest("/health", health::router())
        .fallback_service(spa_fallback)
        .layer(axum::middleware::from_fn(request_id::request_id_middleware))
        .with_state(state)
}

#[allow(dead_code)]
async fn fallback_404() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorBody {
            success: false,
            error: "Not found".to_string(),
            code: "NOT_FOUND".to_string(),
            message: "Not found".to_string(),
            trace_id: None,
        }),
    )
}
