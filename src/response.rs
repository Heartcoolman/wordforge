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

    pub fn service_unavailable(code: &str, message: &str) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: code.to_string(),
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

    // v1.1-P0.9：资源包热更专用错误，对齐 docs/backend-handoff-resource-pack-v1.1.md §3。
    pub fn resource_pack_not_found(message: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "RESOURCE_PACK_NOT_FOUND".to_string(),
            message: message.to_string(),
            is_operational: true,
        }
    }

    pub fn resource_pack_app_version_too_low(message: &str) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code: "RESOURCE_PACK_APP_VERSION_TOO_LOW".to_string(),
            message: message.to_string(),
            is_operational: true,
        }
    }

    pub fn resource_pack_channel_forbidden(message: &str) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: "RESOURCE_PACK_CHANNEL_FORBIDDEN".to_string(),
            message: message.to_string(),
            is_operational: true,
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

impl From<crate::blocking::BlockingTaskError> for AppError {
    fn from(value: crate::blocking::BlockingTaskError) -> Self {
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

    #[tokio::test]
    async fn unauthorized_sets_401_and_code() {
        let err = AppError::unauthorized("need login");
        assert_eq!(err.status, axum::http::StatusCode::UNAUTHORIZED);
        assert_eq!(err.code, "AUTH_UNAUTHORIZED");
        let resp = err.into_response();
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "AUTH_UNAUTHORIZED");
        assert_eq!(json["message"], "need login");
    }

    #[tokio::test]
    async fn forbidden_sets_403_and_code() {
        let err = AppError::forbidden("no access");
        assert_eq!(err.status, axum::http::StatusCode::FORBIDDEN);
        assert_eq!(err.code, "FORBIDDEN");
        let resp = err.into_response();
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "FORBIDDEN");
    }

    #[tokio::test]
    async fn conflict_carries_custom_code() {
        let err = AppError::conflict("DUPLICATE", "already exists");
        assert_eq!(err.status, axum::http::StatusCode::CONFLICT);
        let resp = err.into_response();
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "DUPLICATE");
        assert_eq!(json["message"], "already exists");
    }

    #[tokio::test]
    async fn too_many_requests_sets_429() {
        let err = AppError::too_many_requests("slow down");
        assert_eq!(err.status, axum::http::StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(err.code, "RATE_LIMITED");
        let resp = err.into_response();
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "RATE_LIMITED");
    }

    #[tokio::test]
    async fn payload_too_large_sets_413() {
        let err = AppError::payload_too_large("body too big");
        assert_eq!(err.status, axum::http::StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(err.code, "PAYLOAD_TOO_LARGE");
        let resp = err.into_response();
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "PAYLOAD_TOO_LARGE");
        assert_eq!(json["message"], "body too big");
    }

    #[tokio::test]
    async fn ok_wraps_data_with_success_true() {
        let resp = ok(serde_json::json!({"hello": "world"})).into_response();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["success"], true);
        assert_eq!(json["data"]["hello"], "world");
    }

    #[tokio::test]
    async fn created_returns_201() {
        let resp = created(serde_json::json!({"id": 1})).into_response();
        assert_eq!(resp.status(), axum::http::StatusCode::CREATED);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["success"], true);
        assert_eq!(json["data"]["id"], 1);
    }

    #[tokio::test]
    async fn paginated_computes_total_pages() {
        let resp = paginated(vec![1, 2, 3], 25, 1, 10).into_response();
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["success"], true);
        assert_eq!(json["data"]["total"], 25);
        assert_eq!(json["data"]["page"], 1);
        assert_eq!(json["data"]["perPage"], 10);
        assert_eq!(json["data"]["totalPages"], 3);
        assert_eq!(json["data"]["data"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn paginated_with_zero_per_page_returns_zero_total_pages() {
        let resp = paginated::<i32>(vec![], 100, 1, 0).into_response();
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["data"]["totalPages"], 0);
    }

    #[test]
    fn store_validation_error_maps_to_bad_request() {
        let err: AppError = crate::store::StoreError::Validation("bad input".to_string()).into();
        assert_eq!(err.status, axum::http::StatusCode::BAD_REQUEST);
        assert_eq!(err.code, "VALIDATION_ERROR");
        assert!(err.is_operational);
        assert!(err.message.contains("bad input"));
    }

    #[test]
    fn store_not_found_error_maps_to_internal() {
        let err: AppError = crate::store::StoreError::NotFound {
            entity: "user".to_string(),
            key: "abc".to_string(),
        }
        .into();
        assert_eq!(err.status, axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.code, "INTERNAL_ERROR");
        assert!(!err.is_operational);
    }

    #[tokio::test]
    async fn blocking_task_error_maps_to_internal() {
        // 构造一个 panic 的 JoinError
        let handle: tokio::task::JoinHandle<()> = tokio::spawn(async {
            panic!("boom");
        });
        let join_err = handle.await.unwrap_err();
        let bte = crate::blocking::BlockingTaskError::new("test-task", join_err);
        let err: AppError = bte.into();
        assert_eq!(err.status, axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        assert!(!err.is_operational);
        let resp = err.into_response();
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        // 内部错误消息被隐藏
        assert!(!text.contains("boom"));
        assert!(text.contains("服务器内部错误"));
    }

    #[test]
    fn error_body_serializes_camel_case_trace_id() {
        let body = ErrorBody {
            success: false,
            code: "X".to_string(),
            message: "m".to_string(),
            trace_id: Some("tid".to_string()),
        };
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["traceId"], "tid");
        assert!(json.get("trace_id").is_none());
    }
}
