use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use http_body_util::BodyExt;

use crate::response::ErrorBody;

pub async fn request_id_middleware(req: Request, next: Next) -> Response {
    let request_id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| is_valid_request_id(s))
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

    if !response.status().is_success() {
        if is_json_content_type(&response) {
            // Existing JSON error: inject traceId
            inject_trace_id(response, &request_id).await
        } else if response.status().is_client_error() || response.status().is_server_error() {
            // Non-JSON error (e.g. 413 Payload Too Large from DefaultBodyLimit): wrap in ErrorBody
            wrap_plain_error_as_json(response, &request_id).await
        } else {
            response
        }
    } else {
        response
    }
}

fn is_json_content_type(response: &Response) -> bool {
    response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|ct| ct.contains("application/json"))
        .unwrap_or(false)
}

async fn inject_trace_id(response: Response, request_id: &str) -> Response {
    let (parts, body) = response.into_parts();

    let bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => return Response::from_parts(parts, Body::empty()),
    };

    let patched = match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Ok(mut json) => {
            if let Some(obj) = json.as_object_mut() {
                obj.insert(
                    "traceId".to_string(),
                    serde_json::Value::String(request_id.to_string()),
                );
            }
            serde_json::to_vec(&json).unwrap_or_else(|_| bytes.to_vec())
        }
        Err(_) => bytes.to_vec(),
    };

    Response::from_parts(parts, Body::from(patched))
}

async fn wrap_plain_error_as_json(response: Response, request_id: &str) -> Response {
    let status = response.status();

    // Read the original body to use as the message if present
    let (_, body) = response.into_parts();
    let original_message = body
        .collect()
        .await
        .ok()
        .map(|c| String::from_utf8_lossy(&c.to_bytes()).trim().to_string())
        .filter(|s| !s.is_empty());

    let reason = status.canonical_reason().unwrap_or("Error");
    let code = error_code_for_status(status);
    let message = original_message.unwrap_or_else(|| reason.to_string());

    (
        status,
        axum::Json(ErrorBody {
            success: false,
            code: code.to_string(),
            message,
            trace_id: Some(request_id.to_string()),
        }),
    )
        .into_response()
}

fn error_code_for_status(status: StatusCode) -> &'static str {
    match status {
        StatusCode::BAD_REQUEST => "BAD_REQUEST",
        StatusCode::UNAUTHORIZED => "AUTH_UNAUTHORIZED",
        StatusCode::FORBIDDEN => "FORBIDDEN",
        StatusCode::NOT_FOUND => "NOT_FOUND",
        StatusCode::METHOD_NOT_ALLOWED => "METHOD_NOT_ALLOWED",
        StatusCode::CONFLICT => "CONFLICT",
        StatusCode::PAYLOAD_TOO_LARGE => "PAYLOAD_TOO_LARGE",
        StatusCode::TOO_MANY_REQUESTS => "RATE_LIMITED",
        _ => "INTERNAL_ERROR",
    }
}

/// 校验客户端提供的 x-request-id：长度不超过 128 字符，仅允许字母数字、连字符和下划线
fn is_valid_request_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
