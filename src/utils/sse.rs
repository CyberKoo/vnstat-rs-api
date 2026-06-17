use axum::http::HeaderValue;
use axum::response::{IntoResponse, Response, Sse};
use axum::response::sse::Event;
use futures_util::Stream;

/// Wraps an SSE stream with common HTTP response headers for long-lived SSE
/// connections.
///
/// The headers set are:
/// * `Cache-Control: no-cache, no-transform` — prevents proxies from caching
///   the event stream or transforming its data.
/// * `Connection: keep-alive` — hints that the underlying TCP connection
///   should be kept open.
/// * `X-Accel-Buffering: no` — disables nginx buffering when the
///   application runs behind nginx.
///
/// * `sse` — The SSE streaming response to decorate.
///
/// * Returns: An [`axum::response::Response`] with the above headers
///   attached.
pub fn sse_with_default_headers<T>(sse: Sse<T>) -> Response
where
    T: Stream<Item = Result<Event, String>> + Send + 'static,
{
    let mut res = sse.into_response();

    let headers = res.headers_mut();
    headers.insert("Cache-Control", HeaderValue::from_static("no-cache, no-transform"));
    headers.insert("Connection", HeaderValue::from_static("keep-alive"));
    headers.insert("X-Accel-Buffering", HeaderValue::from_static("no"));

    res
}
