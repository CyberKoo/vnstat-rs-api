use crate::api_error::ApiError;
use crate::model::jsend::JsendResponse;
use crate::model::vnstat::{
    DayRecord, FiveMinuteRecord, HourRecord, Interface, MonthRecord, TopRecord, Total, Traffic,
    Updated, YearRecord,
};
use crate::utils::sse::sse_with_default_headers;
use async_stream::stream;
use axum::extract::FromRequest;
use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, KeepAlive};
use axum::response::{Response, Sse};
use axum::routing::get;
use axum::{Json, Router};
use futures_util::StreamExt;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tracing::trace;

use super::AppState;

// ── Query parameters ────────────────────────────────────────────────────────

/// Optional query parameters accepted by `GET /interfaces/{if_name}`.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct InterfaceQuery {
    /// Comma-separated list of time periods to include (e.g. "day,hour,month").
    /// When absent all periods are returned.
    periods: Option<String>,

    /// Maximum number of records to return per period.
    limit: Option<u32>,
}

/// Query extractor whose rejection is converted into a JSend [`ApiError`].
///
/// Wraps [`axum::extract::Query`] so that malformed query strings produce a
/// JSend-formatted `400` response instead of axum's default plain-text body.
#[derive(Debug)]
struct ApiQuery<T>(pub T);

impl<S, T> FromRequest<S> for ApiQuery<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(req: axum::extract::Request, state: &S) -> Result<Self, Self::Rejection> {
        let Query(value) = axum::extract::Query::<T>::from_request(req, state)
            .await
            .map_err(ApiError::from)?;
        Ok(Self(value))
    }
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
///
/// Liveness probe: returns `200 OK` as long as the server process is
/// running. It intentionally does not invoke vnstat.
pub async fn get_health() -> Json<JsendResponse<String>> {
    Json(JsendResponse::success_with_data("ok".to_string()))
}

/// Handler for `GET /version`.
pub async fn get_version(
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<String>>, ApiError> {
    let version = state.vnstat.get_vnstat_version().await?;
    Ok(Json(JsendResponse::success_with_data(version)))
}

// ── Interface collection handlers ───────────────────────────────────────────

/// Handler for `GET /interfaces`.
async fn get_interfaces(
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Vec<String>>>, ApiError> {
    let names = state.vnstat.list_interfaces().await?;
    Ok(Json(JsendResponse::success_with_data(names)))
}

/// Handler for `GET /interfaces/summary`.
///
/// Returns a compact summary for every monitored interface.
async fn get_all_interfaces_summary(
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Vec<InterfaceSummary>>>, ApiError> {
    let data = state.vnstat.fetch_vnstat_data().await?;
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
) -> Result<Json<JsendResponse<AggregateStats>>, ApiError> {
    let data = state.vnstat.fetch_vnstat_data().await?;
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
    ApiQuery(query): ApiQuery<InterfaceQuery>,
) -> Result<Json<JsendResponse<Interface>>, ApiError> {
    let mut data = state.vnstat.get_interface(&if_name).await?;
    apply_traffic_filter(&mut data.traffic, &query);
    Ok(Json(JsendResponse::success_with_data(data)))
}

/// Handler for `GET /interfaces/{if_name}/summary`.
async fn get_interface_summary(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<InterfaceSummary>>, ApiError> {
    let data = state.vnstat.get_interface(&if_name).await?;
    Ok(Json(JsendResponse::success_with_data(
        InterfaceSummary::from_interface(&data),
    )))
}

/// Handler for `GET /interfaces/{if_name}/updated`.
async fn get_interface_updated(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Updated>>, ApiError> {
    let data = state.vnstat.get_interface(&if_name).await?;
    Ok(Json(JsendResponse::success_with_data(data.updated)))
}

// ── Period handlers ─────────────────────────────────────────────────────────

/// Handler for `GET /interfaces/{if_name}/periods/day`.
async fn get_interface_period_day(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Vec<DayRecord>>>, ApiError> {
    let data = state.vnstat.get_interface(&if_name).await?;
    Ok(Json(JsendResponse::success_with_data(data.traffic.day)))
}

/// Handler for `GET /interfaces/{if_name}/periods/hour`.
async fn get_interface_period_hour(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Vec<HourRecord>>>, ApiError> {
    let data = state.vnstat.get_interface(&if_name).await?;
    Ok(Json(JsendResponse::success_with_data(data.traffic.hour)))
}

/// Handler for `GET /interfaces/{if_name}/periods/month`.
async fn get_interface_period_month(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Vec<MonthRecord>>>, ApiError> {
    let data = state.vnstat.get_interface(&if_name).await?;
    Ok(Json(JsendResponse::success_with_data(data.traffic.month)))
}

/// Handler for `GET /interfaces/{if_name}/periods/year`.
async fn get_interface_period_year(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Vec<YearRecord>>>, ApiError> {
    let data = state.vnstat.get_interface(&if_name).await?;
    Ok(Json(JsendResponse::success_with_data(data.traffic.year)))
}

/// Handler for `GET /interfaces/{if_name}/periods/fiveminute`.
async fn get_interface_period_fiveminute(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Vec<FiveMinuteRecord>>>, ApiError> {
    let data = state.vnstat.get_interface(&if_name).await?;
    Ok(Json(JsendResponse::success_with_data(
        data.traffic.fiveminute,
    )))
}

/// Handler for `GET /interfaces/{if_name}/periods/top`.
async fn get_interface_period_top(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Vec<TopRecord>>>, ApiError> {
    let data = state.vnstat.get_interface(&if_name).await?;
    Ok(Json(JsendResponse::success_with_data(data.traffic.top)))
}

/// Handler for `GET /interfaces/{if_name}/periods/total`.
async fn get_interface_period_total(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Total>>, ApiError> {
    let data = state.vnstat.get_interface(&if_name).await?;
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
/// When the server begins a graceful shutdown, the stream emits a final
/// `shutdown` event and closes, so the long-lived connection does not block
/// the shutdown drain.
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

    // End the stream when graceful shutdown begins (emitting a farewell
    // event first), so the server can drain this connection instead of
    // waiting for the client to disconnect.
    let shutdown_token = state.shutdown_token.clone();
    let stream = stream! {
        let mut stream = stream;
        loop {
            tokio::select! {
                biased;
                _ = shutdown_token.cancelled() => {
                    yield Ok(Event::default().event("shutdown").data("server is shutting down"));
                    break;
                }
                item = stream.next() => match item {
                    Some(item) => yield item,
                    None => break,
                },
            }
        }
    };

    let sse = Sse::new(stream).keep_alive(KeepAlive::default());

    sse_with_default_headers(sse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::response::IntoResponse;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    fn app_state() -> AppState {
        let script = test_support::fake_vnstat_script();
        AppState {
            vnstat: Arc::new(crate::service::vnstat_service::VnstatService::new(
                script.to_str().unwrap().to_string(),
                5,
            )),
            task_registry: Arc::new(crate::task_registry::TaskRegistry::new(4)),
            shutdown_token: CancellationToken::new(),
        }
    }

    fn state() -> State<AppState> {
        State(app_state())
    }

    fn json_of<T: serde::Serialize>(json: Json<JsendResponse<T>>) -> serde_json::Value {
        serde_json::to_value(json.0).unwrap()
    }

    async fn assert_error<T: serde::Serialize + std::fmt::Debug>(
        result: Result<Json<JsendResponse<T>>, ApiError>,
        status: StatusCode,
        code: i32,
        status_str: &str,
    ) {
        let res = result.unwrap_err().into_response();
        assert_eq!(res.status(), status);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], status_str);
        assert_eq!(json["code"], code);
    }

    fn sample_traffic() -> Traffic {
        let data: crate::model::vnstat::VnstatData =
            serde_json::from_str(test_support::SAMPLE_VNSTAT_JSON).unwrap();
        data.interfaces.into_iter().next().unwrap().traffic
    }

    #[test]
    fn builds_interfaces_router() {
        let _ = interfaces_router();
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let json = serde_json::to_value(get_health().await.0).unwrap();
        assert_eq!(json["status"], "success");
        assert_eq!(json["data"], "ok");
    }

    #[tokio::test]
    async fn version_returns_vnstat_version() {
        let json = json_of(get_version(state()).await.unwrap());
        assert_eq!(json["data"], "2.13");
    }

    #[tokio::test]
    async fn version_fetch_failure_returns_503() {
        let script = test_support::garbage_vnstat_script();
        let bad_state = AppState {
            vnstat: Arc::new(crate::service::vnstat_service::VnstatService::new(
                script.to_str().unwrap().to_string(),
                5,
            )),
            task_registry: Arc::new(crate::task_registry::TaskRegistry::new(4)),
            shutdown_token: CancellationToken::new(),
        };
        assert_error(
            get_version(State(bad_state)).await,
            StatusCode::SERVICE_UNAVAILABLE,
            10000,
            "error",
        )
        .await;
    }

    #[tokio::test]
    async fn interfaces_lists_names() {
        let json = json_of(get_interfaces(state()).await.unwrap());
        assert_eq!(json["data"], serde_json::json!(["eth0", "wlan0"]));
    }

    #[tokio::test]
    async fn all_interfaces_summary() {
        let json = json_of(get_all_interfaces_summary(state()).await.unwrap());
        let data = json["data"].as_array().unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0]["name"], "eth0");
        assert_eq!(data[0]["todayRx"], 138825634);
        assert_eq!(data[0]["todayTx"], 7089952);
        assert_eq!(data[0]["total"]["rx"], 123456789);
        assert_eq!(data[0]["updatedTimestamp"], 1780331400);
        assert_eq!(data[1]["alias"], "Wireless");
        assert_eq!(data[1]["todayRx"], 0);
    }

    #[tokio::test]
    async fn interfaces_stats_aggregates() {
        let json = json_of(get_interfaces_stats(state()).await.unwrap());
        let data = json["data"].clone();
        assert_eq!(data["totalInterfaces"], 2);
        assert_eq!(data["totalRx"], 123456790);
        assert_eq!(data["totalTx"], 987654323);
    }

    #[tokio::test]
    async fn interface_data_full() {
        let json = json_of(
            get_interface_data(
                Path("eth0".into()),
                state(),
                ApiQuery(InterfaceQuery::default()),
            )
            .await
            .unwrap(),
        );
        let data = json["data"].clone();
        assert_eq!(data["name"], "eth0");
        assert_eq!(data["traffic"]["day"].as_array().unwrap().len(), 2);
        assert_eq!(data["traffic"]["total"]["rx"], 123456789);
    }

    #[tokio::test]
    async fn interface_data_filters_periods_and_limit() {
        let query = InterfaceQuery {
            periods: Some("day,total".into()),
            limit: Some(1),
        };
        let json = json_of(
            get_interface_data(Path("eth0".into()), state(), ApiQuery(query))
                .await
                .unwrap(),
        );
        let data = json["data"].clone();
        assert_eq!(data["traffic"]["day"].as_array().unwrap().len(), 1);
        assert!(data["traffic"]["hour"].as_array().unwrap().is_empty());
        assert!(data["traffic"]["month"].as_array().unwrap().is_empty());
        assert_eq!(data["traffic"]["total"]["rx"], 123456789);
    }

    #[tokio::test]
    async fn interface_data_unknown_interface_returns_404() {
        let result: Result<Json<JsendResponse<Interface>>, ApiError> = get_interface_data(
            Path("eth9".into()),
            state(),
            ApiQuery(InterfaceQuery::default()),
        )
        .await;
        let err = result.expect_err("expected a 404 error");
        assert_error::<Interface>(Err(err), StatusCode::NOT_FOUND, 10001, "fail").await;
    }

    #[tokio::test]
    async fn interface_summary_and_updated() {
        let summary = json_of(
            get_interface_summary(Path("eth0".into()), state())
                .await
                .unwrap(),
        );
        assert_eq!(summary["data"]["name"], "eth0");
        assert_eq!(summary["data"]["todayRx"], 138825634);

        let updated = json_of(
            get_interface_updated(Path("eth0".into()), state())
                .await
                .unwrap(),
        );
        assert_eq!(updated["data"]["timestamp"], 1780331400);
    }

    #[tokio::test]
    async fn period_endpoints_return_slices() {
        let day = json_of(
            get_interface_period_day(Path("eth0".into()), state())
                .await
                .unwrap(),
        );
        assert_eq!(day["data"].as_array().unwrap().len(), 2);

        let hour = json_of(
            get_interface_period_hour(Path("eth0".into()), state())
                .await
                .unwrap(),
        );
        assert_eq!(hour["data"].as_array().unwrap().len(), 1);
        assert_eq!(hour["data"][0]["rx"], 5000);

        let month = json_of(
            get_interface_period_month(Path("eth0".into()), state())
                .await
                .unwrap(),
        );
        assert_eq!(month["data"].as_array().unwrap().len(), 1);

        let year = json_of(
            get_interface_period_year(Path("eth0".into()), state())
                .await
                .unwrap(),
        );
        assert_eq!(year["data"].as_array().unwrap().len(), 1);

        let fiveminute = json_of(
            get_interface_period_fiveminute(Path("eth0".into()), state())
                .await
                .unwrap(),
        );
        assert_eq!(fiveminute["data"].as_array().unwrap().len(), 1);
        assert_eq!(fiveminute["data"][0]["rx"], 100);

        let top = json_of(
            get_interface_period_top(Path("eth0".into()), state())
                .await
                .unwrap(),
        );
        assert_eq!(top["data"].as_array().unwrap().len(), 1);

        let total = json_of(
            get_interface_period_total(Path("eth0".into()), state())
                .await
                .unwrap(),
        );
        assert_eq!(total["data"]["rx"], 123456789);
        assert_eq!(total["data"]["tx"], 987654321);
    }

    #[tokio::test]
    async fn period_endpoint_unknown_interface_returns_404() {
        let result: Result<Json<JsendResponse<Vec<DayRecord>>>, ApiError> =
            get_interface_period_day(Path("eth9".into()), state()).await;
        let err = result.expect_err("expected a 404 error");
        assert_error::<Vec<DayRecord>>(Err(err), StatusCode::NOT_FOUND, 10001, "fail").await;
    }

    #[tokio::test]
    async fn api_query_parses_valid_and_rejects_invalid() {
        use axum::extract::FromRequest;

        let req = Request::builder()
            .uri("/?periods=day&limit=7")
            .body(Body::empty())
            .unwrap();
        let parsed = ApiQuery::<InterfaceQuery>::from_request(req, &())
            .await
            .unwrap();
        assert_eq!(parsed.0.limit, Some(7));
        assert_eq!(parsed.0.periods.as_deref(), Some("day"));

        let bad = Request::builder()
            .uri("/?limit=abc")
            .body(Body::empty())
            .unwrap();
        let err = ApiQuery::<InterfaceQuery>::from_request(bad, &())
            .await
            .unwrap_err();
        let res = err.into_response();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn live_sse_streams_events() {
        use http_body_util::BodyExt;

        let res = get_interface_live_sse(Path("eth0".into()), state()).await;
        assert_eq!(
            res.headers().get("Cache-Control").unwrap(),
            "no-cache, no-transform"
        );

        let mut body = res.into_body();
        let mut saw_data = false;
        for _ in 0..3 {
            let frame = tokio::time::timeout(std::time::Duration::from_secs(3), body.frame())
                .await
                .expect("body should yield within timeout")
                .expect("body should not end")
                .expect("no body error");
            if let Ok(bytes) = frame.into_data() {
                saw_data = true;
                assert!(String::from_utf8_lossy(&bytes).contains("jsonversion"));
                break;
            }
        }
        assert!(saw_data, "expected at least one data frame");
    }

    /// The stream must end (after a farewell event) when the shutdown token
    /// is cancelled, so active SSE connections do not block graceful shutdown.
    #[tokio::test]
    async fn live_sse_ends_when_shutdown_token_cancelled() {
        use http_body_util::BodyExt;

        let app_state = app_state();
        let res = get_interface_live_sse(Path("eth0".into()), State(app_state.clone())).await;
        let mut body = res.into_body();

        // Confirm the stream is live first.
        tokio::time::timeout(std::time::Duration::from_secs(3), body.frame())
            .await
            .expect("stream should be live within timeout")
            .expect("body should not end")
            .expect("no body error");

        // Cancel the shutdown token: the stream should emit a farewell event
        // and then close, so the server can drain the connection.
        app_state.shutdown_token.cancel();

        let mut farewell = false;
        loop {
            let frame = tokio::time::timeout(std::time::Duration::from_secs(3), body.frame())
                .await
                .expect("stream should end within timeout after shutdown");
            match frame {
                Some(Ok(frame)) => {
                    if let Ok(bytes) = frame.into_data() {
                        if String::from_utf8_lossy(&bytes).contains("shutting down") {
                            farewell = true;
                        }
                    }
                }
                Some(Err(_)) | None => break,
            }
        }
        assert!(farewell, "expected a shutdown farewell event before EOF");
    }

    #[test]
    fn filter_keeps_requested_periods_only() {
        let mut traffic = sample_traffic();
        let query = InterfaceQuery {
            periods: Some("day,month".into()),
            limit: None,
        };
        apply_traffic_filter(&mut traffic, &query);
        assert_eq!(traffic.day.len(), 2);
        assert_eq!(traffic.month.len(), 1);
        assert!(traffic.hour.is_empty());
        assert!(traffic.year.is_empty());
        assert!(traffic.fiveminute.is_empty());
        assert!(traffic.top.is_empty());
        // `total` is zeroed (not omitted) when not requested.
        assert_eq!(traffic.total, Total { rx: 0, tx: 0 });
    }

    #[test]
    fn filter_keeps_total_when_requested() {
        let mut traffic = sample_traffic();
        let query = InterfaceQuery {
            periods: Some("total".into()),
            limit: None,
        };
        apply_traffic_filter(&mut traffic, &query);
        assert_eq!(
            traffic.total,
            Total {
                rx: 123456789,
                tx: 987654321
            }
        );
    }

    #[test]
    fn filter_accepts_plural_aliases() {
        let mut traffic = sample_traffic();
        let query = InterfaceQuery {
            periods: Some("hours,months,years,5min".into()),
            limit: None,
        };
        apply_traffic_filter(&mut traffic, &query);
        assert!(!traffic.hour.is_empty());
        assert!(!traffic.month.is_empty());
        assert!(!traffic.year.is_empty());
        assert!(!traffic.fiveminute.is_empty());
        assert!(traffic.day.is_empty());
        assert!(traffic.top.is_empty());
    }

    #[test]
    fn filter_without_periods_keeps_everything() {
        let mut traffic = sample_traffic();
        apply_traffic_filter(&mut traffic, &InterfaceQuery::default());
        assert_eq!(traffic.day.len(), 2);
        assert_eq!(
            traffic.total,
            Total {
                rx: 123456789,
                tx: 987654321
            }
        );
    }

    #[test]
    fn filter_applies_limit() {
        let mut traffic = sample_traffic();
        let query = InterfaceQuery {
            periods: None,
            limit: Some(1),
        };
        apply_traffic_filter(&mut traffic, &query);
        assert_eq!(traffic.day.len(), 1);
        assert_eq!(traffic.hour.len(), 1);
        assert_eq!(traffic.month.len(), 1);
        assert_eq!(traffic.year.len(), 1);
        assert_eq!(traffic.fiveminute.len(), 1);
        assert_eq!(traffic.top.len(), 1);
    }
}
