use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub data: T,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedResponse<T: Serialize> {
    pub data: Vec<T>,
    pub total: u64,
    pub page: u64,
    pub per_page: u64,
    pub total_pages: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody {
    pub success: bool,
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

    pub fn payload_too_large(message: &str) -> Self {
        Self {
            status: StatusCode::PAYLOAD_TOO_LARGE,
            code: "PAYLOAD_TOO_LARGE".to_string(),
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
            "服务器内部错误".to_string()
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
                code: self.code,
                message: exposed_message,
                trace_id: None,
            }),
        )
            .into_response()
    }
}

// 安全说明：StoreError 转换映射：
// - Validation 错误 -> 400 Bad Request（用户输入问题，可安全暴露消息）
// - 其他错误 -> 500 Internal（is_operational=false，IntoResponse 中会替换为通用消息）
impl From<crate::store::StoreError> for AppError {
    fn from(value: crate::store::StoreError) -> Self {
        match &value {
            crate::store::StoreError::Validation(msg) => {
                AppError::bad_request("VALIDATION_ERROR", msg)
            }
            _ => AppError::internal(&value.to_string()),
        }
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

pub fn paginated<T: Serialize>(
    data: Vec<T>,
    total: u64,
    page: u64,
    per_page: u64,
) -> impl IntoResponse {
    let total_pages = if per_page > 0 {
        total.div_ceil(per_page)
    } else {
        0
    };
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            data: PaginatedResponse {
                data,
                total,
                page,
                per_page,
                total_pages,
            },
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
        assert!(text.contains("服务器内部错误"));
    }

    #[tokio::test]
    async fn bad_request_keeps_message() {
        let resp = AppError::bad_request("BAD_INPUT", "invalid email").into_response();
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("invalid email"));
        assert!(text.contains("BAD_INPUT"));
    }

    #[tokio::test]
    async fn error_field_is_code() {
        let resp = AppError::bad_request("BAD_INPUT", "invalid email").into_response();
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "BAD_INPUT");
        assert!(json.get("error").is_none());
    }

    #[tokio::test]
    async fn not_found_code_field() {
        let resp = AppError::not_found("Resource not found").into_response();
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "NOT_FOUND");
        assert!(json.get("error").is_none());
    }
}
