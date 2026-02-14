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
pub mod wordbook_center;
pub mod wordbooks;
pub mod words;

use axum::extract::DefaultBodyLimit;
use axum::http::{header, HeaderValue, Request};
use axum::middleware::Next;
use axum::response::Response;
use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

use crate::middleware::{rate_limit, request_id};
use crate::state::AppState;

/// Maximum request body size: 2 MiB.
const MAX_BODY_SIZE: usize = 2 * 1024 * 1024;

pub fn build_router(state: AppState) -> Router {
    // 认证路由组添加专用速率限制
    let auth_routes = auth::router().layer(axum::middleware::from_fn_with_state(
        state.clone(),
        rate_limit::auth_rate_limit_middleware,
    ));

    // admin 认证路由：写操作添加专用速率限制
    let admin_auth_routes = admin::auth_router().layer(axum::middleware::from_fn_with_state(
        state.clone(),
        rate_limit::auth_rate_limit_middleware,
    ));

    // admin 认证公开路由（如 /status）不受 auth rate limit 约束
    let admin_auth_public_routes = admin::auth_public_router();

    let api_routes = Router::new()
        .nest("/auth", auth_routes)
        .nest("/users", users::router())
        .nest("/words", words::router())
        .nest("/records", records::router())
        .nest("/amas", admin::amas::router())
        .nest(
            "/admin/auth",
            admin_auth_routes.merge(admin_auth_public_routes),
        )
        .nest("/admin", admin::router())
        .nest("/realtime", realtime::router())
        .nest("/wordbooks", wordbooks::router())
        .nest("/study-config", study_config::router())
        .nest("/learning", learning::router())
        .nest("/word-states", word_states::router())
        .nest("/user-profile", user_profile::router())
        .nest("/notifications", notifications::router())
        .nest("/content", content::router())
        .nest("/wordbook-center", wordbook_center::user_router())
        .nest("/v1", v1::router())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            rate_limit::rate_limit_middleware,
        ))
        .layer(DefaultBodyLimit::max(MAX_BODY_SIZE));

    // B29: Static file serving with SPA fallback
    let spa_fallback =
        ServeDir::new("static").not_found_service(ServeFile::new("static/index.html"));

    Router::new()
        .nest("/api", api_routes)
        .nest("/health", health::router())
        .fallback_service(spa_fallback)
        .layer(axum::middleware::from_fn(static_cache_headers))
        .layer(axum::middleware::from_fn(request_id::request_id_middleware))
        .with_state(state)
}

async fn static_cache_headers(req: Request<axum::body::Body>, next: Next) -> Response {
    let path = req.uri().path().to_string();
    let mut response = next.run(req).await;

    // Skip API and health routes
    if path.starts_with("/api/") || path.starts_with("/health") {
        return response;
    }

    let cache_value = if path.ends_with(".html") || path == "/" {
        "no-cache, must-revalidate"
    } else if path.starts_with("/assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=3600"
    };

    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_value),
    );
    response
}
