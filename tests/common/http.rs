use axum::body::{to_bytes, Body};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::response::Response;
use axum::Router;
use serde_json::Value;
use tower::util::ServiceExt;

pub async fn request(
    app: &Router,
    method: Method,
    path: &str,
    body: Option<Value>,
    headers: &[(&str, String)],
) -> Response {
    let mut builder = Request::builder().method(method).uri(path);

    for (k, v) in headers {
        builder = builder.header(*k, v.as_str());
    }

    let req = if let Some(payload) = body {
        builder
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .expect("request body")
    } else {
        builder.body(Body::empty()).expect("empty body")
    };

    app.clone().oneshot(req).await.expect("oneshot response")
}

pub async fn response_json(resp: Response) -> (StatusCode, HeaderMap, Value) {
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("read body bytes");

    let json = if bytes.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_slice::<Value>(&bytes).expect("parse json body")
    };

    (status, headers, json)
}

pub fn assert_json_error(body: &Value, code: &str) {
    assert_eq!(body["success"], false);
    assert_eq!(body["code"], code);
    assert!(body.get("message").is_some());
}

pub fn assert_status_ok_json(status: StatusCode, body: &Value) {
    assert!(status.is_success());
    assert_eq!(body["success"], true);
    assert!(body.get("data").is_some());
}
