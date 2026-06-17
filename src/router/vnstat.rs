use crate::error_code::ErrorCode;
use crate::model::jsend::JsendResponse;
use crate::model::vnstat::{Interface, VnstatData};
use crate::utils::sse::sse_with_default_headers;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::KeepAlive;
use axum::response::{IntoResponse, Redirect, Response, Sse};
use axum::routing::get;
use axum::{Json, Router};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use tracing::{info, trace};

use super::AppState;

/// Builds and returns the Axum [`Router`] for all `/vnstat` endpoints.
///
/// Registers both the flat (canonical) routes and legacy prefixed routes
/// that redirect to their flat counterparts.
///
/// # Routes
///
/// | Method | Path                                    | Handler                     |
/// |--------|-----------------------------------------|-----------------------------|
/// | GET    | `/`                                     | [`get_data`]                |
/// | GET    | `/health`                               | [`get_health`]              |
/// | GET    | `/version`                              | [`get_version`]             |
/// | GET    | `/{if_name}`                            | [`get_interface_data`]      |
/// | GET    | `/{if_name}/live`                       | [`get_interface_live_sse`]  |
/// | GET    | `/interfaces`                           | [`get_interfaces`]          |
/// | GET    | `/interfaces/{if_name}`                 | [`redir_interface_to_flat`] |
/// | GET    | `/interfaces/{if_name}/traffic`         | [`redir_interface_traffic_to_flat`] |
/// | GET    | `/interfaces/{if_name}/live`            | [`redir_interface_live_to_flat`] |
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_data))
        .route("/health", get(get_health))
        .route("/version", get(get_version))
        // New flat routes
        .route("/{if_name}", get(get_interface_data))
        .route("/{if_name}/live", get(get_interface_live_sse))
        // Legacy redirects
        .route("/interfaces", get(get_interfaces))
        .route("/interfaces/{if_name}", get(redir_interface_to_flat))
        .route(
            "/interfaces/{if_name}/traffic",
            get(redir_interface_traffic_to_flat),
        )
        .route(
            "/interfaces/{if_name}/live",
            get(redir_interface_live_to_flat),
        )
}

/// Handler for `GET /vnstat/health`.
///
/// Performs a health check against the underlying `vnstat` daemon.
///
/// # Returns
///
/// - `200 OK` with a JSON body containing `"ok"` when the daemon is
///   reachable.
/// - `503 SERVICE_UNAVAILABLE` with an error JSON body when the health
///   check fails.
async fn get_health(
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<String>>, (StatusCode, Json<serde_json::Value>)> {
    match state.vnstat.check_health().await {
        Ok(_) => Ok(Json(JsendResponse::success_with_data("ok".to_string()))),
        Err(e) => {
            let err_json = serde_json::json!({
                "status": "error",
                "code": ErrorCode::GetDataFailed,
                "message": format!("vnstat health check failed: {}", e),
            });
            Err((StatusCode::SERVICE_UNAVAILABLE, Json(err_json)))
        }
    }
}

/// Handler for `GET /vnstat/version`.
///
/// Fetches the full vnstat data dump from the daemon and extracts the
/// `vnstatversion` field.
///
/// # Returns
///
/// - `200 OK` with the vnstat version string wrapped in a JSend envelope.
/// - `400 BAD_REQUEST` when the data fetch fails.
async fn get_version(
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<String>>, (StatusCode, Json<JsendResponse<String>>)> {
    let data = state.vnstat.fetch_vnstat_data().await.map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(JsendResponse::fail(ErrorCode::GetDataFailed)),
        )
    })?;

    Ok(Json(JsendResponse::success_with_data(data.vnstatversion)))
}

/// Handler for `GET /vnstat/`.
///
/// Returns the complete vnstat data object (all interfaces and their
/// traffic history).
///
/// # Returns
///
/// - `200 OK` with a [`VnstatData`] payload wrapped in a JSend envelope.
/// - `400 BAD_REQUEST` when the data fetch fails.
async fn get_data(
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<VnstatData>>, (StatusCode, Json<JsendResponse<String>>)> {
    let data = state.vnstat.fetch_vnstat_data().await.map_err(|e| {
        info!("err: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(JsendResponse::fail(ErrorCode::GetDataFailed)),
        )
    })?;

    Ok(Json(JsendResponse::success_with_data(data)))
}

/// Handler for `GET /vnstat/interfaces`.
///
/// Returns a list of all network interface names known to the vnstat
/// daemon.
///
/// # Returns
///
/// - `200 OK` with a `Vec<String>` of interface names wrapped in a JSend
///   envelope.
/// - `400 BAD_REQUEST` when the interface listing fails.
async fn get_interfaces(
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Vec<String>>>, (StatusCode, Json<JsendResponse<String>>)> {
    let interfaces = state.vnstat.list_vnstat_interfaces().await.map_err(|e| {
        info!("err: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(JsendResponse::fail(ErrorCode::GetDataFailed)),
        )
    })?;

    Ok(Json(JsendResponse::success_with_data(interfaces)))
}

// --- Legacy redirect handlers ---

/// Redirect handler for the legacy route `GET /vnstat/interfaces/{if_name}`.
///
/// Issues a permanent redirect (301) to the flat canonical route
/// `/vnstat/{if_name}`.  The interface name is percent-encoded to
/// preserve special characters.
async fn redir_interface_to_flat(Path(if_name): Path<String>) -> impl IntoResponse {
    let encoded = utf8_percent_encode(&if_name, NON_ALPHANUMERIC).to_string();
    Redirect::permanent(&format!("/api/v1/vnstat/{}", encoded))
}

/// Redirect handler for the legacy route `GET /vnstat/interfaces/{if_name}/traffic`.
///
/// Issues a permanent redirect (301) to the flat canonical route
/// `/vnstat/{if_name}`.  The interface name is percent-encoded to
/// preserve special characters.
async fn redir_interface_traffic_to_flat(Path(if_name): Path<String>) -> impl IntoResponse {
    let encoded = utf8_percent_encode(&if_name, NON_ALPHANUMERIC).to_string();
    Redirect::permanent(&format!("/api/v1/vnstat/{}", encoded))
}

/// Redirect handler for the legacy route `GET /vnstat/interfaces/{if_name}/live`.
///
/// Issues a permanent redirect (301) to the flat canonical route
/// `/vnstat/{if_name}/live`.  The interface name is percent-encoded to
/// preserve special characters.
async fn redir_interface_live_to_flat(Path(if_name): Path<String>) -> impl IntoResponse {
    let encoded = utf8_percent_encode(&if_name, NON_ALPHANUMERIC).to_string();
    Redirect::permanent(&format!("/api/v1/vnstat/{}/live", encoded))
}

/// Handler for `GET /vnstat/{if_name}`.
///
/// Fetches traffic statistics for a single network interface by name.
///
/// # Returns
///
/// - `200 OK` with an [`Interface`] payload wrapped in a JSend envelope.
/// - `400 BAD_REQUEST` when the interface does not exist or the fetch
///   fails.
async fn get_interface_data(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Interface>>, (StatusCode, Json<JsendResponse<String>>)> {
    let data = state
        .vnstat
        .fetch_interface_stats(if_name)
        .await
        .map_err(|e| {
            info!("err: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(JsendResponse::fail(ErrorCode::NoSuchInterface)),
            )
        })?;

    Ok(Json(JsendResponse::success_with_data(data)))
}

/// Handler for `GET /vnstat/{if_name}/live`.
///
/// Opens a Server-Sent Events (SSE) stream that pushes live traffic
/// updates for a single network interface.  The stream is kept alive
/// with periodic keep-alive pings.
///
/// # Returns
///
/// An SSE response with `Cache-Control`, `Connection`, and
/// `X-Accel-Buffering` headers set for optimal streaming behaviour.
pub async fn get_interface_live_sse(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Response {
    trace!("SSE stream for interface `{}` connected.", if_name);

    let stream = state
        .vnstat
        .stream_interface_live_stats(state.task_manager, if_name)
        .await;
    let sse = Sse::new(stream).keep_alive(KeepAlive::default());

    sse_with_default_headers(sse)
}
