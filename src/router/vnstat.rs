use crate::error_code::ErrorCode;
use crate::model::jsend::JsendResponse;
use crate::model::vnstat::{
    DayRecord, FiveMinuteRecord, HourRecord, Interface, MonthRecord, TopRecord, Total, Traffic,
    Updated, YearRecord,
};
use crate::utils::sse::sse_with_default_headers;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::KeepAlive;
use axum::response::{Response, Sse};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tracing::{info, trace};

use super::AppState;

// ── Query parameters ────────────────────────────────────────────────────────

/// Optional query parameters accepted by `GET /interfaces/{if_name}`.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct InterfaceQuery {
    /// Comma-separated list of time periods to include (e.g. "day,hour,month").
    /// When absent all periods are returned.
    periods: Option<String>,

    /// Maximum number of records to return per period.
    limit: Option<u32>,
}

// ── Router ──────────────────────────────────────────────────────────────────

/// Builds and returns the Axum [`Router`] for all `/interfaces` endpoints.
///
/// # Routes
///
/// | Method | Path                              | Handler                          |
/// |--------|-----------------------------------|----------------------------------|
/// | GET    | `/`                               | [`get_interfaces`]               |
/// | GET    | `/summary`                        | [`get_all_interfaces_summary`]   |
/// | GET    | `/stats`                          | [`get_interfaces_stats`]         |
/// | GET    | `/{if_name}`                      | [`get_interface_data`]           |
/// | GET    | `/{if_name}/summary`              | [`get_interface_summary`]        |
/// | GET    | `/{if_name}/updated`              | [`get_interface_updated`]        |
/// | GET    | `/{if_name}/live`                 | [`get_interface_live_sse`]       |
/// | GET    | `/{if_name}/periods/day`          | [`get_interface_period_day`]     |
/// | GET    | `/{if_name}/periods/hour`         | [`get_interface_period_hour`]    |
/// | GET    | `/{if_name}/periods/month`        | [`get_interface_period_month`]   |
/// | GET    | `/{if_name}/periods/year`         | [`get_interface_period_year`]    |
/// | GET    | `/{if_name}/periods/fiveminute`   | [`get_interface_period_fiveminute`] |
/// | GET    | `/{if_name}/periods/top`          | [`get_interface_period_top`]     |
/// | GET    | `/{if_name}/periods/total`        | [`get_interface_period_total`]   |
pub fn interfaces_router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_interfaces))
        .route("/summary", get(get_all_interfaces_summary))
        .route("/stats", get(get_interfaces_stats))
        .route("/{if_name}", get(get_interface_data))
        .route("/{if_name}/summary", get(get_interface_summary))
        .route("/{if_name}/updated", get(get_interface_updated))
        .route("/{if_name}/live", get(get_interface_live_sse))
        .route("/{if_name}/periods/day", get(get_interface_period_day))
        .route("/{if_name}/periods/hour", get(get_interface_period_hour))
        .route("/{if_name}/periods/month", get(get_interface_period_month))
        .route("/{if_name}/periods/year", get(get_interface_period_year))
        .route(
            "/{if_name}/periods/fiveminute",
            get(get_interface_period_fiveminute),
        )
        .route("/{if_name}/periods/top", get(get_interface_period_top))
        .route("/{if_name}/periods/total", get(get_interface_period_total))
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Filters `traffic` in-place according to the requested `periods` and `limit`.
fn apply_traffic_filter(traffic: &mut Traffic, query: &InterfaceQuery) {
    if let Some(ref periods_str) = query.periods {
        let requested: Vec<&str> = periods_str.split(',').map(|s| s.trim()).collect();

        if !requested.contains(&"day") {
            traffic.day.clear();
        }
        if !(requested.contains(&"hour") || requested.contains(&"hours")) {
            traffic.hour.clear();
        }
        if !(requested.contains(&"month") || requested.contains(&"months")) {
            traffic.month.clear();
        }
        if !(requested.contains(&"year") || requested.contains(&"years")) {
            traffic.year.clear();
        }
        if !(requested.contains(&"fiveminute") || requested.contains(&"5min")) {
            traffic.fiveminute.clear();
        }
        if !requested.contains(&"top") {
            traffic.top.clear();
        }
        if !requested.contains(&"total") {
            traffic.total = Total { rx: 0, tx: 0 };
        }
    }

    if let Some(limit) = query.limit {
        let n = limit as usize;
        traffic.day.truncate(n);
        traffic.hour.truncate(n);
        traffic.month.truncate(n);
        traffic.year.truncate(n);
        traffic.fiveminute.truncate(n);
        traffic.top.truncate(n);
    }
}

// ── Service-level handlers ──────────────────────────────────────────────────

/// Handler for `GET /health`.
pub async fn get_health(
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

/// Handler for `GET /version`.
pub async fn get_version(
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<String>>, (StatusCode, Json<JsendResponse<String>>)> {
    let version = state.vnstat.get_vnstat_version().await.map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(JsendResponse::fail(ErrorCode::GetDataFailed)),
        )
    })?;

    Ok(Json(JsendResponse::success_with_data(version)))
}

// ── Interface collection handlers ───────────────────────────────────────────

/// Handler for `GET /interfaces`.
async fn get_interfaces(
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Vec<String>>>, (StatusCode, Json<JsendResponse<String>>)> {
    let names = state.vnstat.list_interfaces().await.map_err(|e| {
        info!("err: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(JsendResponse::fail(ErrorCode::GetDataFailed)),
        )
    })?;

    Ok(Json(JsendResponse::success_with_data(names)))
}

/// Handler for `GET /interfaces/summary`.
///
/// Returns a compact summary for every monitored interface.
async fn get_all_interfaces_summary(
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Vec<InterfaceSummary>>>, (StatusCode, Json<JsendResponse<String>>)> {
    let data = state.vnstat.fetch_vnstat_data().await.map_err(|e| {
        info!("err: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(JsendResponse::fail(ErrorCode::GetDataFailed)),
        )
    })?;
    let summaries: Vec<InterfaceSummary> = data
        .interfaces
        .iter()
        .map(InterfaceSummary::from_interface)
        .collect();
    Ok(Json(JsendResponse::success_with_data(summaries)))
}

/// Handler for `GET /interfaces/stats`.
///
/// Returns aggregate traffic statistics across all interfaces.
async fn get_interfaces_stats(
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<AggregateStats>>, (StatusCode, Json<JsendResponse<String>>)> {
    let data = state.vnstat.fetch_vnstat_data().await.map_err(|e| {
        info!("err: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(JsendResponse::fail(ErrorCode::GetDataFailed)),
        )
    })?;
    let stats = AggregateStats::from_interfaces(&data.interfaces);
    Ok(Json(JsendResponse::success_with_data(stats)))
}

// ── Single-interface handlers ───────────────────────────────────────────────

/// Handler for `GET /interfaces/{if_name}`.
///
/// Accepts optional query parameters:
/// - `periods` — comma-separated list of periods to include (e.g. `?periods=day,hour`).
/// - `limit`   — maximum records per period (e.g. `?limit=7`).
async fn get_interface_data(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<InterfaceQuery>,
) -> Result<Json<JsendResponse<Interface>>, (StatusCode, Json<JsendResponse<String>>)> {
    let mut data = state.vnstat.get_interface(&if_name).await.map_err(|e| {
        info!("err: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(JsendResponse::fail(ErrorCode::NoSuchInterface)),
        )
    })?;
    apply_traffic_filter(&mut data.traffic, &query);
    Ok(Json(JsendResponse::success_with_data(data)))
}

/// Handler for `GET /interfaces/{if_name}/summary`.
async fn get_interface_summary(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<InterfaceSummary>>, (StatusCode, Json<JsendResponse<String>>)> {
    let data = state.vnstat.get_interface(&if_name).await.map_err(|e| {
        info!("err: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(JsendResponse::fail(ErrorCode::NoSuchInterface)),
        )
    })?;
    Ok(Json(JsendResponse::success_with_data(
        InterfaceSummary::from_interface(&data),
    )))
}

/// Handler for `GET /interfaces/{if_name}/updated`.
async fn get_interface_updated(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Updated>>, (StatusCode, Json<JsendResponse<String>>)> {
    let data = state.vnstat.get_interface(&if_name).await.map_err(|e| {
        info!("err: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(JsendResponse::fail(ErrorCode::NoSuchInterface)),
        )
    })?;
    Ok(Json(JsendResponse::success_with_data(data.updated)))
}

// ── Period handlers ─────────────────────────────────────────────────────────

/// Handler for `GET /interfaces/{if_name}/periods/day`.
async fn get_interface_period_day(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Vec<DayRecord>>>, (StatusCode, Json<JsendResponse<String>>)> {
    let data = state.vnstat.get_interface(&if_name).await.map_err(|e| {
        info!("err: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(JsendResponse::fail(ErrorCode::NoSuchInterface)),
        )
    })?;
    Ok(Json(JsendResponse::success_with_data(data.traffic.day)))
}

/// Handler for `GET /interfaces/{if_name}/periods/hour`.
async fn get_interface_period_hour(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Vec<HourRecord>>>, (StatusCode, Json<JsendResponse<String>>)> {
    let data = state.vnstat.get_interface(&if_name).await.map_err(|e| {
        info!("err: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(JsendResponse::fail(ErrorCode::NoSuchInterface)),
        )
    })?;
    Ok(Json(JsendResponse::success_with_data(data.traffic.hour)))
}

/// Handler for `GET /interfaces/{if_name}/periods/month`.
async fn get_interface_period_month(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Vec<MonthRecord>>>, (StatusCode, Json<JsendResponse<String>>)> {
    let data = state.vnstat.get_interface(&if_name).await.map_err(|e| {
        info!("err: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(JsendResponse::fail(ErrorCode::NoSuchInterface)),
        )
    })?;
    Ok(Json(JsendResponse::success_with_data(data.traffic.month)))
}

/// Handler for `GET /interfaces/{if_name}/periods/year`.
async fn get_interface_period_year(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Vec<YearRecord>>>, (StatusCode, Json<JsendResponse<String>>)> {
    let data = state.vnstat.get_interface(&if_name).await.map_err(|e| {
        info!("err: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(JsendResponse::fail(ErrorCode::NoSuchInterface)),
        )
    })?;
    Ok(Json(JsendResponse::success_with_data(data.traffic.year)))
}

/// Handler for `GET /interfaces/{if_name}/periods/fiveminute`.
async fn get_interface_period_fiveminute(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Vec<FiveMinuteRecord>>>, (StatusCode, Json<JsendResponse<String>>)> {
    let data = state.vnstat.get_interface(&if_name).await.map_err(|e| {
        info!("err: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(JsendResponse::fail(ErrorCode::NoSuchInterface)),
        )
    })?;
    Ok(Json(JsendResponse::success_with_data(
        data.traffic.fiveminute,
    )))
}

/// Handler for `GET /interfaces/{if_name}/periods/top`.
async fn get_interface_period_top(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Vec<TopRecord>>>, (StatusCode, Json<JsendResponse<String>>)> {
    let data = state.vnstat.get_interface(&if_name).await.map_err(|e| {
        info!("err: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(JsendResponse::fail(ErrorCode::NoSuchInterface)),
        )
    })?;
    Ok(Json(JsendResponse::success_with_data(data.traffic.top)))
}

/// Handler for `GET /interfaces/{if_name}/periods/total`.
async fn get_interface_period_total(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Total>>, (StatusCode, Json<JsendResponse<String>>)> {
    let data = state.vnstat.get_interface(&if_name).await.map_err(|e| {
        info!("err: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(JsendResponse::fail(ErrorCode::NoSuchInterface)),
        )
    })?;
    Ok(Json(JsendResponse::success_with_data(data.traffic.total)))
}

// ── Response types ──────────────────────────────────────────────────────────

/// Compact summary of a single interface.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct InterfaceSummary {
    /// The system-level interface name (e.g. `eth0`).
    name: String,
    /// A human-readable alias (may be empty).
    alias: String,
    /// Cumulative traffic since the interface was created.
    total: Total,
    /// Bytes received today (from the most recent day record).
    today_rx: u64,
    /// Bytes transmitted today (from the most recent day record).
    today_tx: u64,
    /// Unix epoch timestamp (seconds) of the most recent update.
    updated_timestamp: i64,
}

impl InterfaceSummary {
    fn from_interface(iface: &Interface) -> Self {
        let today = iface.traffic.day.last();
        Self {
            name: iface.name.clone(),
            alias: iface.alias.clone(),
            total: iface.traffic.total.clone(),
            today_rx: today.map(|d| d.rx).unwrap_or(0),
            today_tx: today.map(|d| d.tx).unwrap_or(0),
            updated_timestamp: iface.updated.timestamp,
        }
    }
}

/// Aggregate traffic statistics across all monitored interfaces.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AggregateStats {
    /// Number of interfaces tracked by vnstat.
    total_interfaces: usize,
    /// Cumulative bytes received across all interfaces.
    total_rx: u64,
    /// Cumulative bytes transmitted across all interfaces.
    total_tx: u64,
}

impl AggregateStats {
    fn from_interfaces(interfaces: &[Interface]) -> Self {
        let total_rx: u64 = interfaces.iter().map(|i| i.traffic.total.rx).sum();
        let total_tx: u64 = interfaces.iter().map(|i| i.traffic.total.tx).sum();
        Self {
            total_interfaces: interfaces.len(),
            total_rx,
            total_tx,
        }
    }
}

// ── SSE live stream ─────────────────────────────────────────────────────────

/// Handler for `GET /interfaces/{if_name}/live`.
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
        .stream_interface_live_stats(state.task_registry, if_name)
        .await;
    let sse = Sse::new(stream).keep_alive(KeepAlive::default());

    sse_with_default_headers(sse)
}
