use crate::error_code::ErrorCode;
use crate::model::jsend::JsendResponse;
use crate::service::vnstat_service::VnstatError;
use axum::Json;
use axum::extract::rejection::QueryRejection;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// The single error type returned by all API handlers.
///
/// Every error response is serialized as a JSend envelope via
/// [`JsendResponse`] — raw `json!` bodies are never produced directly.
/// The `status` field follows the JSend spec: `"fail"` for client errors
/// (4xx) and `"error"` for server errors (5xx).
#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: ErrorCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }
}

impl From<VnstatError> for ApiError {
    fn from(e: VnstatError) -> Self {
        match e {
            VnstatError::FetchFailed(inner) => {
                // Debug output includes the full anyhow context chain.
                tracing::error!("vnstat data fetch failed: {:?}", inner);
                ApiError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    ErrorCode::GetDataFailed,
                    inner.to_string(),
                )
            }
            VnstatError::InterfaceNotFound(name) => {
                tracing::warn!("interface not found: {}", name);
                ApiError::new(
                    StatusCode::NOT_FOUND,
                    ErrorCode::NoSuchInterface,
                    format!("no such interface: {}", name),
                )
            }
        }
    }
}

impl From<QueryRejection> for ApiError {
    fn from(e: QueryRejection) -> Self {
        tracing::warn!("invalid query parameters: {}", e);
        ApiError::new(
            StatusCode::BAD_REQUEST,
            ErrorCode::InvalidParameter,
            e.to_string(),
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = if self.status.is_server_error() {
            JsendResponse::<()>::error(self.code, self.message)
        } else {
            JsendResponse::<()>::fail_with_message(self.code, self.message)
        };
        (self.status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;

    async fn body_json(res: Response) -> serde_json::Value {
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn fetch_failure_maps_to_503_error() {
        let err = ApiError::from(VnstatError::FetchFailed(anyhow::anyhow!(
            "subprocess failed"
        )));
        let res = err.into_response();
        assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
        let json = body_json(res).await;
        assert_eq!(json["status"], "error");
        assert_eq!(json["code"], 10000);
        assert!(
            json["message"]
                .as_str()
                .unwrap()
                .contains("subprocess failed")
        );
    }

    #[tokio::test]
    async fn interface_not_found_maps_to_404_fail() {
        let err = ApiError::from(VnstatError::InterfaceNotFound("eth9".into()));
        let res = err.into_response();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let json = body_json(res).await;
        assert_eq!(json["status"], "fail");
        assert_eq!(json["code"], 10001);
        assert_eq!(json["message"], "no such interface: eth9");
    }

    #[tokio::test]
    async fn query_rejection_maps_to_400() {
        use axum::extract::{FromRequest, Query};
        use serde::Deserialize;

        #[derive(Deserialize, Debug)]
        struct Q {
            #[allow(dead_code)]
            limit: u32,
        }

        let req = Request::builder()
            .uri("/?limit=abc")
            .body(Body::empty())
            .unwrap();
        let rejection = Query::<Q>::from_request(req, &()).await.unwrap_err();
        let err = ApiError::from(rejection);
        let res = err.into_response();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let json = body_json(res).await;
        assert_eq!(json["status"], "fail");
        assert_eq!(json["code"], 10002);
        assert!(json["message"].as_str().unwrap().contains("limit"));
    }
}
