use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};
use axum::response::IntoResponse;
use serde::de::DeserializeOwned;

use crate::response::AppError;

/// A wrapper around `axum::Json<T>` that returns `AppError` on deserialization failure
/// instead of Axum's default plain-text rejection.
pub struct JsonBody<T>(pub T);

#[axum::async_trait]
impl<S, T> FromRequest<S> for JsonBody<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match axum::Json::<T>::from_request(req, state).await {
            Ok(axum::Json(value)) => Ok(JsonBody(value)),
            Err(rejection) => Err(json_rejection_to_app_error(rejection)),
        }
    }
}

fn json_rejection_to_app_error(rejection: JsonRejection) -> AppError {
    match rejection {
        JsonRejection::JsonDataError(e) => {
            tracing::warn!(error = %e, "JSON data deserialization failed");
            AppError::bad_request("INVALID_REQUEST_BODY", "请求体格式无效")
        }
        JsonRejection::JsonSyntaxError(e) => {
            tracing::warn!(error = %e, "JSON syntax parsing failed");
            AppError::bad_request("INVALID_REQUEST_BODY", "请求体格式无效")
        }
        JsonRejection::MissingJsonContentType(e) => {
            tracing::warn!(error = %e, "Missing or invalid JSON Content-Type");
            AppError::bad_request("INVALID_REQUEST_BODY", "请求体格式无效")
        }
        JsonRejection::BytesRejection(e) => {
            tracing::warn!(error = %e, "Failed to read request body bytes");
            AppError::bad_request("INVALID_REQUEST_BODY", "请求体格式无效")
        }
        other => {
            tracing::warn!(error = %other, "Unexpected JSON body rejection");
            AppError::bad_request("INVALID_REQUEST_BODY", "请求体格式无效")
        }
    }
}

// Allow destructuring like `JsonBody(req)` in handler parameters
impl<T> std::ops::Deref for JsonBody<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: serde::Serialize> IntoResponse for JsonBody<T> {
    fn into_response(self) -> axum::response::Response {
        axum::Json(self.0).into_response()
    }
}
