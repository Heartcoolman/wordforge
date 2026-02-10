use axum::{extract::Request, middleware::Next, response::Response};

pub async fn request_id_middleware(req: Request, next: Next) -> Response {
    let request_id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let span = tracing::info_span!("request", request_id = %request_id);

    let mut response = {
        let _guard = span.enter();
        let method = req.method().clone();
        let uri = req.uri().clone();

        let start = std::time::Instant::now();
        let response = next.run(req).await;
        let latency_ms = start.elapsed().as_millis();

        tracing::info!(
            method = %method,
            path = %uri.path(),
            status = %response.status().as_u16(),
            latency_ms = %latency_ms,
            "request completed"
        );

        response
    };

    if let Ok(value) = request_id.parse() {
        response.headers_mut().insert("x-request-id", value);
    }

    response
}
