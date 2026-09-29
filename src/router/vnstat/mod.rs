use crate::api::error::ApiError;
use crate::api::response::JsendResponse;
use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use super::AppState;

mod query;
mod response;
mod sse;

use query::{ApiQuery, InterfaceQuery, apply_traffic_filter};
use response::{
    AggregateStats, DayRecord, FiveMinuteRecord, HourRecord, Interface, InterfaceSummary,
    MonthRecord, TopRecord, Total, Updated, YearRecord,
};
pub(super) use sse::get_interface_live_sse;

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
        .route("/{if_name}/link-speed", get(get_interface_link_speed))
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
    let summaries: Vec<InterfaceSummary> =
        data.interfaces.iter().map(InterfaceSummary::from).collect();
    Ok(Json(JsendResponse::success_with_data(summaries)))
}

/// Handler for `GET /interfaces/stats`.
///
/// Returns aggregate traffic statistics across all interfaces.
async fn get_interfaces_stats(
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<AggregateStats>>, ApiError> {
    let data = state.vnstat.fetch_vnstat_data().await?;
    let stats = AggregateStats::from(data.interfaces.as_slice());
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
    let response = Interface::from(data);
    Ok(Json(JsendResponse::success_with_data(response)))
}

/// Handler for `GET /interfaces/{if_name}/summary`.
async fn get_interface_summary(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<InterfaceSummary>>, ApiError> {
    let data = state.vnstat.get_interface(&if_name).await?;
    Ok(Json(JsendResponse::success_with_data(
        InterfaceSummary::from(&data),
    )))
}

/// Handler for `GET /interfaces/{if_name}/updated`.
async fn get_interface_updated(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Updated>>, ApiError> {
    let data = state.vnstat.get_interface(&if_name).await?;
    Ok(Json(JsendResponse::success_with_data(data.updated.into())))
}

/// Reported link speed of an interface.
#[derive(Debug, Serialize)]
struct LinkSpeed {
    interface: String,
    /// RX link speed in Mbps.
    rx: u64,
    /// TX link speed in Mbps.
    tx: u64,
}

/// Handler for `GET /interfaces/{if_name}/link-speed`.
///
/// Returns the configured RX/TX link speed (in Mbps) for the interface,
/// defaulting to 1000 Mbps for interfaces without an entry in the
/// `[link_speed]` configuration section.
async fn get_interface_link_speed(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<LinkSpeed>>, ApiError> {
    // Validate the interface exists, consistent with the other
    // `/{if_name}` routes (404 for unknown interfaces).
    state.vnstat.get_interface(&if_name).await?;

    let speed = state.link_speed.get(&if_name);
    Ok(Json(JsendResponse::success_with_data(LinkSpeed {
        interface: if_name,
        rx: speed.rx,
        tx: speed.tx,
    })))
}

// ── Period handlers ─────────────────────────────────────────────────────────

/// Handler for `GET /interfaces/{if_name}/periods/day`.
async fn get_interface_period_day(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Vec<DayRecord>>>, ApiError> {
    let data = state.vnstat.get_interface(&if_name).await?;
    Ok(Json(JsendResponse::success_with_data(
        data.traffic.day.into_iter().map(Into::into).collect(),
    )))
}

/// Handler for `GET /interfaces/{if_name}/periods/hour`.
async fn get_interface_period_hour(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Vec<HourRecord>>>, ApiError> {
    let data = state.vnstat.get_interface(&if_name).await?;
    Ok(Json(JsendResponse::success_with_data(
        data.traffic.hour.into_iter().map(Into::into).collect(),
    )))
}

/// Handler for `GET /interfaces/{if_name}/periods/month`.
async fn get_interface_period_month(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Vec<MonthRecord>>>, ApiError> {
    let data = state.vnstat.get_interface(&if_name).await?;
    Ok(Json(JsendResponse::success_with_data(
        data.traffic.month.into_iter().map(Into::into).collect(),
    )))
}

/// Handler for `GET /interfaces/{if_name}/periods/year`.
async fn get_interface_period_year(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Vec<YearRecord>>>, ApiError> {
    let data = state.vnstat.get_interface(&if_name).await?;
    Ok(Json(JsendResponse::success_with_data(
        data.traffic.year.into_iter().map(Into::into).collect(),
    )))
}

/// Handler for `GET /interfaces/{if_name}/periods/fiveminute`.
async fn get_interface_period_fiveminute(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Vec<FiveMinuteRecord>>>, ApiError> {
    let data = state.vnstat.get_interface(&if_name).await?;
    Ok(Json(JsendResponse::success_with_data(
        data.traffic
            .fiveminute
            .into_iter()
            .map(Into::into)
            .collect(),
    )))
}

/// Handler for `GET /interfaces/{if_name}/periods/top`.
async fn get_interface_period_top(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Vec<TopRecord>>>, ApiError> {
    let data = state.vnstat.get_interface(&if_name).await?;
    Ok(Json(JsendResponse::success_with_data(
        data.traffic.top.into_iter().map(Into::into).collect(),
    )))
}

/// Handler for `GET /interfaces/{if_name}/periods/total`.
async fn get_interface_period_total(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<JsendResponse<Total>>, ApiError> {
    let data = state.vnstat.get_interface(&if_name).await?;
    Ok(Json(JsendResponse::success_with_data(
        data.traffic.total.into(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::vnstat::{Total as VnstatTotal, Traffic};
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
                Arc::new(crate::task_registry::TaskRegistry::new(4)),
            )),
            link_speed: Default::default(),
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
                Arc::new(crate::task_registry::TaskRegistry::new(4)),
            )),
            link_speed: Default::default(),
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
    async fn link_speed_defaults_to_1000() {
        let json = json_of(
            get_interface_link_speed(Path("eth0".into()), state())
                .await
                .unwrap(),
        );
        let data = json["data"].clone();
        assert_eq!(data["interface"], "eth0");
        assert_eq!(data["rx"], 1000);
        assert_eq!(data["tx"], 1000);
    }

    #[tokio::test]
    async fn link_speed_returns_configured_values() {
        use crate::config::link_speed::LinkSpeed as ConfigLinkSpeed;
        use crate::config::link_speed::LinkSpeedConfig as ConfigLinkSpeedConfig;

        let app_state = AppState {
            link_speed: ConfigLinkSpeedConfig {
                interfaces: std::collections::HashMap::from([(
                    "eth0".to_string(),
                    ConfigLinkSpeed { rx: 500, tx: 2000 },
                )]),
            },
            ..app_state()
        };
        let json = json_of(
            get_interface_link_speed(Path("eth0".into()), State(app_state))
                .await
                .unwrap(),
        );
        let data = json["data"].clone();
        assert_eq!(data["interface"], "eth0");
        assert_eq!(data["rx"], 500);
        assert_eq!(data["tx"], 2000);
    }

    #[tokio::test]
    async fn link_speed_is_per_interface() {
        use crate::config::link_speed::LinkSpeed as ConfigLinkSpeed;
        use crate::config::link_speed::LinkSpeedConfig as ConfigLinkSpeedConfig;

        // Only eth0 is configured; wlan0 must fall back to the default.
        let app_state = AppState {
            link_speed: ConfigLinkSpeedConfig {
                interfaces: std::collections::HashMap::from([(
                    "eth0".to_string(),
                    ConfigLinkSpeed { rx: 500, tx: 2000 },
                )]),
            },
            ..app_state()
        };

        let eth0 = json_of(
            get_interface_link_speed(Path("eth0".into()), State(app_state.clone()))
                .await
                .unwrap(),
        );
        assert_eq!(eth0["data"]["rx"], 500);
        assert_eq!(eth0["data"]["tx"], 2000);

        let wlan0 = json_of(
            get_interface_link_speed(Path("wlan0".into()), State(app_state))
                .await
                .unwrap(),
        );
        assert_eq!(wlan0["data"]["rx"], 1000);
        assert_eq!(wlan0["data"]["tx"], 1000);
    }

    #[tokio::test]
    async fn link_speed_unknown_interface_returns_404() {
        let result: Result<Json<JsendResponse<LinkSpeed>>, ApiError> =
            get_interface_link_speed(Path("eth9".into()), state()).await;
        let err = result.expect_err("expected a 404 error");
        assert_error::<LinkSpeed>(Err(err), StatusCode::NOT_FOUND, 10001, "fail").await;
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
    async fn unknown_interface_live_sse_returns_jsend_404() {
        let response = get_interface_live_sse(Path("eth9".into()), state()).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(
            response
                .headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("application/json")
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "fail");
        assert_eq!(json["code"], 10001);
        assert_eq!(json["message"], "no such interface: eth9");
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
                    if let Ok(bytes) = frame.into_data()
                        && String::from_utf8_lossy(&bytes).contains("shutting down")
                    {
                        farewell = true;
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
        assert_eq!(traffic.total, VnstatTotal { rx: 0, tx: 0 });
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
            VnstatTotal {
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
            VnstatTotal {
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
