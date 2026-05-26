pub mod admin;
pub mod analytics;
pub mod auth;
pub mod content;
pub mod feedback;
pub mod health;
pub mod learning;
pub mod metrics;
pub mod notifications;
pub mod probe_results;
pub mod realtime;
pub mod records;
pub mod resource_packs;
pub mod status;
pub mod study_config;
pub mod telemetry;
pub mod user_profile;
pub mod users;
pub mod v1;
pub mod word_favorites;
pub mod word_notes;
pub mod word_states;
pub mod wordbook_center;
pub mod wordbooks;
pub mod words;

use axum::extract::DefaultBodyLimit;
use axum::http::{header, HeaderValue, Request};
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::get_service;
use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

use crate::middleware::{
    device, http_metrics, maintenance, rate_limit, request_id, server_time, strict_mode,
};
use crate::state::AppState;

/// Maximum request body size: 2 MiB.
const MAX_BODY_SIZE: usize = 2 * 1024 * 1024;
const LEGACY_USER_SPA_PATHS: &[&str] = &[
    "/",
    "/login",
    "/register",
    "/learning",
    "/flashcard",
    "/vocabulary",
    "/wordbooks",
    "/wordbook-center",
    "/statistics",
    "/history",
    "/profile",
    "/notifications",
];

pub fn build_router(state: AppState) -> Router {
    // 认证路由组添加专用速率限制
    let auth_routes = auth::router().layer(axum::middleware::from_fn_with_state(
        state.clone(),
        rate_limit::auth_rate_limit_middleware,
    ));

    // 管理员后台不受速率限制
    let admin_auth_routes = admin::auth_router();

    // admin 认证公开路由（如 /status）不受 auth rate limit 约束
    let admin_auth_public_routes = admin::auth_public_router();

    // M1-A6：豁免路由组 — 不受 strict-mode / maintenance 中间件约束。
    // 豁免依据：admin 后台用浏览器原生 UA；status/realtime/telemetry 为基础设施通道；
    //           v1 全部 410 无客户端契约意义，维护模式下也应直接 410 而非 503。
    // 注意：admin auth 路由已在外层 merge，此处 /admin 指业务 admin 路由。
    let exempt_routes = Router::new()
        .nest(
            "/admin/auth",
            admin_auth_routes.merge(admin_auth_public_routes),
        )
        .nest("/admin", admin::router())
        .nest("/realtime", realtime::router())
        .nest("/v1", v1::router())
        .nest("/status", status::router())
        .nest("/telemetry", telemetry::router());

    // 受 strict-mode 与 maintenance 校验的路由组
    let checked_routes = Router::new()
        .nest("/auth", auth_routes)
        .nest("/users", users::router())
        .nest("/words", words::router())
        .nest("/records", records::router())
        .nest("/analytics", analytics::router())
        .nest("/amas", admin::amas::router())
        .nest("/wordbooks", wordbooks::router())
        .nest("/study-config", study_config::router())
        .nest("/learning", learning::router())
        .nest("/word-states", word_states::router())
        .nest("/user-profile", user_profile::router())
        .nest("/notifications", notifications::router())
        .nest("/content", content::router())
        .nest("/feedback", feedback::router())
        .nest("/word-favorites", word_favorites::router())
        .nest("/word-notes", word_notes::router())
        .nest("/resource-packs", resource_packs::router())
        .nest("/wordbook-center", wordbook_center::user_router())
        .nest("/probe", probe_results::router())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            strict_mode::strict_mode_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            maintenance::maintenance_middleware,
        ));

    let api_routes = Router::new()
        .merge(exempt_routes)
        .merge(checked_routes)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            device::device_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            rate_limit::rate_limit_middleware,
        ))
        .layer(DefaultBodyLimit::max(MAX_BODY_SIZE));

    let mut app = Router::new()
        .nest("/api", api_routes)
        .nest("/health", health::router())
        // M0-P1: Prometheus /metrics 端点（admin 鉴权，独立于 /api 中间件链）
        .route("/metrics", axum::routing::get(metrics::metrics_handler));

    if !state.config().api_only {
        let static_files = ServeDir::new("static").append_index_html_on_directories(false);
        let admin_spa = ServeDir::new("static").fallback(ServeFile::new("static/index.html"));
        app = LEGACY_USER_SPA_PATHS.iter().fold(app, |router, path| {
            router.route_service(path, get_service(ServeFile::new("static/index.html")))
        });
        app = app
            .nest_service("/admin", admin_spa)
            .fallback_service(static_files)
            .layer(axum::middleware::from_fn(static_cache_headers));
    }

    // M0-P1 补充：histogram middleware 挂在 request_id 外侧，确保所有请求都被采集
    // server_time 注入 X-Server-Time 响应头（最外层，所有响应通用），用于前端时钟对齐
    app.layer(axum::middleware::from_fn(
        server_time::server_time_middleware,
    ))
    .layer(axum::middleware::from_fn(request_id::request_id_middleware))
    .layer(axum::middleware::from_fn(http_metrics::record_http_metrics))
    .with_state(state)
}

async fn static_cache_headers(req: Request<axum::body::Body>, next: Next) -> Response {
    let path = req.uri().path().to_string();
    let mut response = next.run(req).await;

    // Skip API and health routes
    if path.starts_with("/api/") || path.starts_with("/health") {
        return response;
    }

    let is_html = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.contains("text/html"));

    let cache_value = if is_html {
        "no-cache, must-revalidate"
    } else if path.starts_with("/assets/") || path.starts_with("/packs/") {
        // v1.1-P0.5：资源包 payload 走版本号路径 /packs/<pack>/<ver>/payload.json，
        // 与 /assets/* 同策略：版本号即不可变标识，可永久缓存。
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=3600"
    };

    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(cache_value));
    response
}
