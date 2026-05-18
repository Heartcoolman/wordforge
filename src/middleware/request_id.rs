use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use http_body_util::BodyExt;
use tracing::Instrument;

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

    let method = req.method().clone();
    let uri = req.uri().clone();

    let start = std::time::Instant::now();
    let mut response = next.run(req).instrument(span.clone()).await;
    let latency_ms = start.elapsed().as_millis();

    span.in_scope(|| {
        tracing::info!(
            method = %method,
            path = %uri.path(),
            status = %response.status().as_u16(),
            latency_ms = %latency_ms,
            "request completed"
        );
    });

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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{HeaderValue, Request, StatusCode};
    use axum::routing::{get, post};
    use axum::Router;
    use tower::ServiceExt;

    fn build_app() -> Router {
        Router::new()
            .route("/ok", get(|| async { "ok" }))
            .route(
                "/err-json",
                get(|| async {
                    crate::response::AppError::bad_request("BAD", "validation failed")
                        .into_response()
                }),
            )
            .route(
                "/err-plain",
                get(|| async {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        [(axum::http::header::CONTENT_TYPE, "text/plain")],
                        "raw error body",
                    )
                        .into_response()
                }),
            )
            .route(
                "/err-empty-plain",
                get(|| async {
                    (
                        StatusCode::BAD_GATEWAY,
                        [(axum::http::header::CONTENT_TYPE, "text/plain")],
                        "",
                    )
                        .into_response()
                }),
            )
            .route(
                "/err-non-array-json",
                get(|| async {
                    // JSON 但非对象，inject_trace_id 应该原样返回
                    (
                        StatusCode::BAD_REQUEST,
                        [(axum::http::header::CONTENT_TYPE, "application/json")],
                        "[1,2,3]",
                    )
                        .into_response()
                }),
            )
            .route(
                "/err-malformed-json",
                get(|| async {
                    (
                        StatusCode::BAD_REQUEST,
                        [(axum::http::header::CONTENT_TYPE, "application/json")],
                        "not-json",
                    )
                        .into_response()
                }),
            )
            .route(
                "/echo-body",
                post(|body: String| async move { body }),
            )
            .layer(axum::middleware::from_fn(request_id_middleware))
    }

    #[test]
    fn validates_request_id_constraints() {
        assert!(is_valid_request_id("abc-123_XYZ"));
        assert!(!is_valid_request_id(""));
        assert!(!is_valid_request_id("with space"));
        assert!(!is_valid_request_id("中文"));
        let long = "a".repeat(129);
        assert!(!is_valid_request_id(&long));
        let exactly_128 = "a".repeat(128);
        assert!(is_valid_request_id(&exactly_128));
    }

    #[test]
    fn error_code_mapping_covers_common_statuses() {
        assert_eq!(error_code_for_status(StatusCode::BAD_REQUEST), "BAD_REQUEST");
        assert_eq!(
            error_code_for_status(StatusCode::UNAUTHORIZED),
            "AUTH_UNAUTHORIZED"
        );
        assert_eq!(error_code_for_status(StatusCode::FORBIDDEN), "FORBIDDEN");
        assert_eq!(error_code_for_status(StatusCode::NOT_FOUND), "NOT_FOUND");
        assert_eq!(
            error_code_for_status(StatusCode::METHOD_NOT_ALLOWED),
            "METHOD_NOT_ALLOWED"
        );
        assert_eq!(error_code_for_status(StatusCode::CONFLICT), "CONFLICT");
        assert_eq!(
            error_code_for_status(StatusCode::PAYLOAD_TOO_LARGE),
            "PAYLOAD_TOO_LARGE"
        );
        assert_eq!(
            error_code_for_status(StatusCode::TOO_MANY_REQUESTS),
            "RATE_LIMITED"
        );
        assert_eq!(
            error_code_for_status(StatusCode::INTERNAL_SERVER_ERROR),
            "INTERNAL_ERROR"
        );
        // 未列出 → fallback
        assert_eq!(
            error_code_for_status(StatusCode::IM_A_TEAPOT),
            "INTERNAL_ERROR"
        );
    }

    #[tokio::test]
    async fn ok_response_gets_request_id_header_generated() {
        let app = build_app();
        let req = Request::builder().uri("/ok").body(Body::empty()).unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let rid = resp.headers().get("x-request-id").expect("x-request-id");
        // 应是 uuid
        assert!(rid.to_str().unwrap().len() >= 32);
    }

    #[tokio::test]
    async fn valid_client_provided_request_id_is_kept() {
        let app = build_app();
        let req = Request::builder()
            .uri("/ok")
            .header("x-request-id", "client-trace-abc_123")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(
            resp.headers().get("x-request-id").unwrap(),
            HeaderValue::from_static("client-trace-abc_123")
        );
    }

    #[tokio::test]
    async fn invalid_client_request_id_is_replaced_with_uuid() {
        let app = build_app();
        let req = Request::builder()
            .uri("/ok")
            .header("x-request-id", "has space")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let rid = resp.headers().get("x-request-id").unwrap().to_str().unwrap();
        assert_ne!(rid, "has space");
        // uuid v4: 36 chars
        assert_eq!(rid.len(), 36);
    }

    #[tokio::test]
    async fn json_error_gets_trace_id_injected() {
        let app = build_app();
        let req = Request::builder()
            .uri("/err-json")
            .header("x-request-id", "trace-001")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["traceId"], "trace-001");
        assert_eq!(json["code"], "BAD");
    }

    #[tokio::test]
    async fn plain_text_error_wrapped_as_json_error_body() {
        let app = build_app();
        let req = Request::builder()
            .uri("/err-plain")
            .header("x-request-id", "trace-plain")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["success"], false);
        assert_eq!(json["code"], "INTERNAL_ERROR");
        assert_eq!(json["message"], "raw error body");
        assert_eq!(json["traceId"], "trace-plain");
    }

    #[tokio::test]
    async fn empty_plain_error_uses_canonical_reason_as_message() {
        let app = build_app();
        let req = Request::builder()
            .uri("/err-empty-plain")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["message"], "Bad Gateway");
        assert_eq!(json["code"], "INTERNAL_ERROR");
    }

    #[tokio::test]
    async fn malformed_json_error_passes_through() {
        let app = build_app();
        let req = Request::builder()
            .uri("/err-malformed-json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        // 因为 JSON parse 失败，原 bytes 应原样返回
        assert_eq!(&body[..], b"not-json");
    }

    #[tokio::test]
    async fn json_non_object_array_passes_through_unmodified() {
        let app = build_app();
        let req = Request::builder()
            .uri("/err-non-array-json")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // 数组保持原样，没有 traceId 字段被加上
        assert!(json.is_array());
        assert_eq!(json.as_array().unwrap().len(), 3);
    }
}
