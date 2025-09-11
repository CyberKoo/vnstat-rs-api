use axum::http::HeaderValue;
use axum::response::{IntoResponse, Response, Sse};
use axum::response::sse::Event;
use futures_util::Stream;

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