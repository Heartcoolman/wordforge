//! `X-Server-Time` 响应头中间件。
//!
//! 在每个响应上注入 `X-Server-Time: <unix_seconds>` header，前端读后计算
//! `skew = server_time - client_time`，再用 `nowSecs() = Date.now()/1000 + skew`
//! 来做任何与时间相关的判断（如 JWT exp 预判）。
//!
//! 这一层抗的是**客户端**与**服务器**之间的时钟漂移：浏览器用真实时间、服务器
//! 可能时钟跑偏，两者裸比较会让前端误判 token "已过期" 而主动清 storage。
//!
//! 配合 `clock_health` 模块，前者抗服务器漂移、本中间件抗客户端/服务器相对漂移，
//! 形成完整防御。

use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;
use chrono::Utc;

pub const X_SERVER_TIME: HeaderName = HeaderName::from_static("x-server-time");

pub async fn server_time_middleware(req: Request<Body>, next: Next) -> Response {
    let mut response = next.run(req).await;
    let ts = Utc::now().timestamp();
    // i64 → HeaderValue 经 from_str 保证 ASCII，不会失败；fallback "0" 仅形式上兜底
    let value =
        HeaderValue::from_str(&ts.to_string()).unwrap_or_else(|_| HeaderValue::from_static("0"));
    response.headers_mut().insert(X_SERVER_TIME, value);
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    #[tokio::test]
    async fn injects_x_server_time_header() {
        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(server_time_middleware));

        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let header = response
            .headers()
            .get(X_SERVER_TIME)
            .expect("X-Server-Time header missing");
        let ts: i64 = header.to_str().unwrap().parse().expect("not a valid i64");

        let now = Utc::now().timestamp();
        assert!((now - ts).abs() < 5, "server time should be close to now");
    }
}
