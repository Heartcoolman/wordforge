use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub data: T,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    pub success: bool,
    pub error: String,
    pub code: String,
    pub message: String,
    pub trace_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AppError {
    pub status: StatusCode,
    pub code: String,
    pub message: String,
    pub is_operational: bool,
}

impl AppError {
    pub fn bad_request(code: &str, message: &str) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: code.to_string(),
            message: message.to_string(),
            is_operational: true,
        }
    }

    pub fn unauthorized(message: &str) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "AUTH_UNAUTHORIZED".to_string(),
            message: message.to_string(),
            is_operational: true,
        }
    }

    pub fn forbidden(message: &str) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: "FORBIDDEN".to_string(),
            message: message.to_string(),
            is_operational: true,
        }
    }

    pub fn not_found(message: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "NOT_FOUND".to_string(),
            message: message.to_string(),
            is_operational: true,
        }
    }

    pub fn conflict(code: &str, message: &str) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code: code.to_string(),
            message: message.to_string(),
            is_operational: true,
        }
    }

    pub fn too_many_requests(message: &str) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            code: "RATE_LIMITED".to_string(),
            message: message.to_string(),
            is_operational: true,
        }
    }

    pub fn internal(message: &str) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "INTERNAL_ERROR".to_string(),
            message: message.to_string(),
            is_operational: false,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let exposed_message = if self.is_operational {
            self.message.clone()
        } else {
            "Internal server error".to_string()
        };

        if self.is_operational {
            tracing::warn!(status = %self.status, code = %self.code, error = %self.message, "API error");
        } else {
            tracing::error!(status = %self.status, code = %self.code, error = %self.message, "Internal API error");
        }

        (
            self.status,
            Json(ErrorBody {
                success: false,
                error: self.code.clone(),
                code: self.code,
                message: exposed_message,
                trace_id: None,
            }),
        )
            .into_response()
    }
}

impl From<crate::store::StoreError> for AppError {
    fn from(value: crate::store::StoreError) -> Self {
        AppError::internal(&value.to_string())
    }
}

pub fn ok<T: Serialize>(data: T) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            data,
        }),
    )
}

pub fn created<T: Serialize>(data: T) -> impl IntoResponse {
    (
        StatusCode::CREATED,
        Json(ApiResponse {
            success: true,
            data,
        }),
    )
}

#[cfg(test)]
mod tests {
    use axum::body::to_bytes;
    use axum::response::IntoResponse;

    use super::*;

    #[tokio::test]
    async fn internal_error_is_redacted() {
        let resp = AppError::internal("db crash").into_response();
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(!text.contains("db crash"));
        assert!(text.contains("Internal server error"));
    }

    #[tokio::test]
    async fn bad_request_keeps_message() {
        let resp = AppError::bad_request("BAD_INPUT", "invalid email").into_response();
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("invalid email"));
        assert!(text.contains("BAD_INPUT"));
    }
}
