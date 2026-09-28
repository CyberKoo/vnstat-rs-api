use crate::api::error::ApiError;
use crate::model::vnstat::{Total, Traffic};
use axum::extract::{FromRequest, Query};
use serde::Deserialize;
use serde::de::DeserializeOwned;

/// Optional query parameters accepted by `GET /interfaces/{if_name}`.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct InterfaceQuery {
    /// Comma-separated list of time periods to include.
    /// When absent all periods are returned.
    pub(super) periods: Option<String>,

    /// Maximum number of records to return per period.
    pub(super) limit: Option<u32>,
}

/// Query extractor that converts malformed parameters to the API error format.
#[derive(Debug)]
pub(super) struct ApiQuery<T>(pub(super) T);

impl<S, T> FromRequest<S> for ApiQuery<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(req: axum::extract::Request, state: &S) -> Result<Self, Self::Rejection> {
        let Query(value) = Query::<T>::from_request(req, state)
            .await
            .map_err(ApiError::from)?;
        Ok(Self(value))
    }
}

/// Applies the endpoint's period selection and per-period record limit.
pub(super) fn apply_traffic_filter(traffic: &mut Traffic, query: &InterfaceQuery) {
    if let Some(ref periods_str) = query.periods {
        let requested: Vec<&str> = periods_str.split(',').map(str::trim).collect();

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
