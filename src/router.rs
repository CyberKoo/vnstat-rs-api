use crate::error_code::ErrorCode;
use crate::model::jsend::JsendResponse;
use crate::service::vnstat_service::VnstatService;
use crate::task_registry::TaskRegistry;
use axum::extract::OriginalUri;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use std::sync::Arc;

mod vnstat;

/// Application-wide shared state injected into every Axum handler via
/// [axum::extract::State].
///
/// Holds the long-lived services that handlers need to query vnstat data
/// and manage background tasks.
#[derive(Clone)]
pub struct AppState {
    /// Service for fetching and streaming network statistics from the
    /// underlying `vnstat` daemon.
    pub vnstat: Arc<VnstatService>,

    /// Registry of named subprocesses whose output is broadcast to
    /// multiple subscribers (used for SSE live streams).
    pub task_registry: Arc<TaskRegistry>,
}

/// Handler for `GET /` — a simple service banner served at the root path.
pub async fn home() -> Json<JsendResponse<String>> {
    Json(JsendResponse::success_with_data(
        concat!(
            env!("CARGO_PKG_NAME"),
            " v",
            env!("CARGO_PKG_VERSION"),
            " is up and running"
        )
        .to_string(),
    ))
}

/// Fallback handler for unmatched paths — JSend-formatted `404`.
pub async fn not_found(OriginalUri(uri): OriginalUri) -> (StatusCode, Json<JsendResponse<()>>) {
    (
        StatusCode::NOT_FOUND,
        Json(JsendResponse::fail_with_message(
            ErrorCode::ResourceNotFound,
            format!("resource not found: {}", uri),
        )),
    )
}

/// Fallback handler for unsupported methods — JSend-formatted `405`.
pub async fn method_not_allowed() -> (StatusCode, Json<JsendResponse<()>>) {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        Json(JsendResponse::fail_with_message(
            ErrorCode::InvalidParameter,
            "method not allowed".to_string(),
        )),
    )
}

/// Assembles the top-level Axum [`Router`] and returns it.
///
/// Service-level endpoints (`/health`, `/version`) are registered directly.
/// Interface-related routes live under `/interfaces`.
pub fn get_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(vnstat::get_health))
        .route("/version", get(vnstat::get_version))
        .nest("/interfaces", vnstat::interfaces_router())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn home_returns_service_banner() {
        let json = serde_json::to_value(home().await.0).unwrap();
        assert_eq!(json["status"], "success");
        assert_eq!(json["code"], 0);
        let banner = json["data"].as_str().unwrap();
        assert!(banner.contains(env!("CARGO_PKG_NAME")));
        assert!(banner.contains(env!("CARGO_PKG_VERSION")));
        assert!(banner.contains("is up and running"));
    }

    #[tokio::test]
    async fn not_found_returns_jsend_404_with_path() {
        let (status, json) =
            not_found(OriginalUri(axum::http::Uri::from_static("/api/v1/nope"))).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let value = serde_json::to_value(json.0).unwrap();
        assert_eq!(value["status"], "fail");
        assert_eq!(value["code"], 10003);
        assert_eq!(value["message"], "resource not found: /api/v1/nope");
    }

    #[tokio::test]
    async fn method_not_allowed_returns_jsend_405() {
        let (status, json) = method_not_allowed().await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
        let value = serde_json::to_value(json.0).unwrap();
        assert_eq!(value["status"], "fail");
        assert_eq!(value["code"], 10002);
    }

    #[test]
    fn builds_router_with_routes() {
        let _ = get_router();
    }
}
