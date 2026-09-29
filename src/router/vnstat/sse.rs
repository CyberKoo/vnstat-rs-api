use crate::api::error::ApiError;
use crate::service::vnstat_service::LiveStatsMessage;
use crate::utils::sse::sse_with_default_headers;
use async_stream::stream;
use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive};
use axum::response::{IntoResponse, Response, Sse};
use futures_util::StreamExt;
use tracing::trace;

use super::AppState;

/// Handler for `GET /interfaces/{if_name}/live`.
pub async fn get_interface_live_sse(
    Path(if_name): Path<String>,
    State(state): State<AppState>,
) -> Response {
    if let Err(error) = state.vnstat.get_interface(&if_name).await {
        return ApiError::from(error).into_response();
    }

    trace!("SSE stream for interface `{}` connected.", if_name);

    let stream = state.vnstat.stream_interface_live_stats(if_name).await;

    // End the stream when graceful shutdown begins so the server can drain
    // long-lived connections instead of waiting for clients to disconnect.
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
                    Some(Ok(LiveStatsMessage::Data(data))) => yield Ok(Event::default().data(data).id(crate::utils::timestamp::get_in_ms().to_string())),
                    Some(Err(error)) => yield Err(error),
                    None => break,
                },
            }
        }
    };

    sse_with_default_headers(Sse::new(stream).keep_alive(KeepAlive::default()))
}
