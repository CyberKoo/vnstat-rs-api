use super::traits::ConfigEntity;
use anyhow::bail;
use axum::http::{HeaderName, Method, Uri};
use serde::Deserialize;
use std::net::{Ipv4Addr, Ipv6Addr};

/// Configuration for Cross-Origin Resource Sharing (CORS).
///
/// Controls which origins, methods, headers, and credentials are allowed
/// when the API is accessed from a web browser.  All fields have sensible
/// defaults so that the layer can be enabled with minimal configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct CorsConfig {
    /// Whether the CORS layer is enabled at all.
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Explicitly allowed origins.
    ///
    /// If empty the behaviour is controlled by the `allow_credentials` flag:
    /// * With credentials — `mirror_request()` (echoes the request origin).
    /// * Without credentials — `any()` (wildcard `*`).
    ///
    /// The explicit list accepts HTTP(S) origins and the opaque origin `null`.
    /// Use an empty list, not `*`, for the wildcard/mirror behaviour.
    #[serde(default)]
    pub allowed_origins: Vec<String>,

    /// Explicitly allowed HTTP methods.
    ///
    /// When empty, all methods are permitted.
    #[serde(default)]
    pub allowed_methods: Vec<String>,

    /// Explicitly allowed request headers.
    ///
    /// When empty, all headers are permitted.
    #[serde(default)]
    pub allowed_headers: Vec<String>,

    /// Response headers exposed to the client.
    ///
    /// When empty, no additional headers are exposed beyond the CORS-safe
    /// list.
    #[serde(default)]
    pub expose_headers: Vec<String>,

    /// Whether the browser may include credentials (cookies, authorization
    /// headers) with cross-origin requests.
    #[serde(default)]
    pub allow_credentials: bool,

    /// The maximum time (in seconds) the preflight response can be cached
    /// by the browser.
    ///
    /// When `None` the browser's default is used.
    #[serde(default)]
    pub max_age: Option<u64>,
}

impl Default for CorsConfig {
    /// Returns a `CorsConfig` with sensible defaults: enabled with wide-open
    /// access (all origins, all methods, all headers, no exposed headers, no
    /// credentials, no explicit max-age).
    fn default() -> Self {
        CorsConfig {
            enabled: default_enabled(),
            allowed_origins: Vec::new(),
            allowed_methods: Vec::new(),
            allowed_headers: Vec::new(),
            expose_headers: Vec::new(),
            allow_credentials: false,
            max_age: None,
        }
    }
}

impl ConfigEntity for CorsConfig {
    fn validate(&self) -> anyhow::Result<()> {
        for (index, origin) in self.allowed_origins.iter().enumerate() {
            if !is_valid_origin(origin) {
                bail!("cors.allowed_origins[{index}] is not a valid HTTP(S) origin: {origin:?}");
            }
        }
        for (index, method) in self.allowed_methods.iter().enumerate() {
            if Method::from_bytes(method.as_bytes()).is_err() {
                bail!("cors.allowed_methods[{index}] is not a valid HTTP method: {method:?}");
            }
        }
        for (index, header) in self.allowed_headers.iter().enumerate() {
            if HeaderName::from_bytes(header.as_bytes()).is_err() {
                bail!("cors.allowed_headers[{index}] is not a valid header name: {header:?}");
            }
        }
        for (index, header) in self.expose_headers.iter().enumerate() {
            if HeaderName::from_bytes(header.as_bytes()).is_err() {
                bail!("cors.expose_headers[{index}] is not a valid header name: {header:?}");
            }
        }
        Ok(())
    }
}

// An Origin is a serialized scheme + authority, not an arbitrary HTTP header
// value or a full URL (which could include a path, query, or fragment).
fn is_valid_origin(origin: &str) -> bool {
    if origin == "null" {
        return true;
    }

    let Some((scheme, authority)) = origin.split_once("://") else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return false;
    }
    if authority.is_empty()
        || authority.contains(['/', '?', '#', '@', '\\'])
        || origin.parse::<Uri>().is_err()
    {
        return false;
    }

    let (host, port) = if let Some(ipv6) = authority.strip_prefix('[') {
        let Some((address, suffix)) = ipv6.split_once(']') else {
            return false;
        };
        if address.parse::<Ipv6Addr>().is_err() {
            return false;
        }
        if suffix.is_empty() {
            return true;
        }
        let Some(port) = suffix.strip_prefix(':') else {
            return false;
        };
        return valid_port(port);
    } else if let Some((host, port)) = authority.split_once(':') {
        (host, Some(port))
    } else {
        (authority, None)
    };

    let host = host.strip_suffix('.').unwrap_or(host);
    !host.is_empty()
        && (!host.bytes().all(|c| c.is_ascii_digit() || c == b'.')
            || host.parse::<Ipv4Addr>().is_ok())
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
        && port.is_none_or(valid_port)
}

fn valid_port(port: &str) -> bool {
    !port.is_empty() && port.bytes().all(|c| c.is_ascii_digit()) && port.parse::<u16>().is_ok()
}

/// Returns the default value for the `enabled` field (`false`).
fn default_enabled() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::Request;
    use axum::response::Response;
    use tower::ServiceExt;

    fn router_for(cfg: CorsConfig) -> Router {
        Router::new().layer(crate::app::cors_layer(&cfg))
    }

    fn simple_request(origin: Option<&str>) -> Request<Body> {
        let mut builder = Request::builder().uri("/");
        if let Some(o) = origin {
            builder = builder.header("origin", o);
        }
        builder.body(Body::empty()).unwrap()
    }

    async fn send(router: Router, req: Request<Body>) -> Response {
        router.oneshot(req).await.unwrap()
    }

    #[test]
    fn defaults_are_disabled_and_wide_open() {
        let cfg = CorsConfig::default();
        assert!(!cfg.enabled);
        assert!(cfg.allowed_origins.is_empty());
        assert!(!cfg.allow_credentials);
        assert_eq!(cfg.max_age, None);
        // Default validate() is a no-op and must succeed.
        assert!(cfg.validate().is_ok());
    }

    #[tokio::test]
    async fn wildcard_origin_when_empty_and_no_credentials() {
        let cfg = CorsConfig {
            allowed_origins: vec![],
            allow_credentials: false,
            ..Default::default()
        };
        let res = send(router_for(cfg), simple_request(Some("http://example.com"))).await;
        assert_eq!(
            res.headers().get("access-control-allow-origin").unwrap(),
            "*"
        );
    }

    #[tokio::test]
    async fn mirror_request_when_credentials_without_origin_list() {
        let cfg = CorsConfig {
            allowed_origins: vec![],
            allow_credentials: true,
            ..Default::default()
        };
        let res = send(router_for(cfg), simple_request(Some("http://example.com"))).await;
        assert_eq!(
            res.headers().get("access-control-allow-origin").unwrap(),
            "http://example.com"
        );
    }

    #[tokio::test]
    async fn explicit_origin_list_matches() {
        let cfg = CorsConfig {
            allowed_origins: vec!["http://a.com".into()],
            ..Default::default()
        };
        let res = send(router_for(cfg), simple_request(Some("http://a.com"))).await;
        assert_eq!(
            res.headers().get("access-control-allow-origin").unwrap(),
            "http://a.com"
        );
    }

    #[tokio::test]
    async fn invalid_origin_entries_are_skipped() {
        let cfg = CorsConfig {
            allowed_origins: vec!["not a valid header value \n".into()],
            ..Default::default()
        };
        let res = send(router_for(cfg), simple_request(Some("http://a.com"))).await;
        assert!(res.headers().get("access-control-allow-origin").is_none());
    }

    #[tokio::test]
    async fn explicit_methods_headers_and_exposed_headers() {
        let cfg = CorsConfig {
            allowed_methods: vec!["GET".into()],
            allowed_headers: vec!["x-custom".into()],
            expose_headers: vec!["X-Exposed".into()],
            ..Default::default()
        };
        let req = Request::builder()
            .uri("/")
            .method("OPTIONS")
            .header("origin", "http://a.com")
            .header("access-control-request-method", "GET")
            .header("access-control-request-headers", "x-custom")
            .body(Body::empty())
            .unwrap();
        let res = send(router_for(cfg.clone()), req).await;
        let h = res.headers();
        assert_eq!(h.get("access-control-allow-methods").unwrap(), "GET");
        assert_eq!(h.get("access-control-allow-headers").unwrap(), "x-custom");

        // Exposed headers only appear on actual (non-preflight) responses.
        let res = send(router_for(cfg), simple_request(Some("http://a.com"))).await;
        let exposed = res.headers().get("access-control-expose-headers").unwrap();
        assert_eq!(exposed.to_str().unwrap().to_lowercase(), "x-exposed");
    }

    #[tokio::test]
    async fn credentials_and_max_age_on_preflight() {
        let cfg = CorsConfig {
            allowed_origins: vec!["http://a.com".into()],
            allow_credentials: true,
            max_age: Some(3600),
            ..Default::default()
        };
        let req = Request::builder()
            .uri("/")
            .method("OPTIONS")
            .header("origin", "http://a.com")
            .header("access-control-request-method", "GET")
            .body(Body::empty())
            .unwrap();
        let res = send(router_for(cfg), req).await;
        let h = res.headers();
        assert_eq!(h.get("access-control-allow-credentials").unwrap(), "true");
        assert_eq!(h.get("access-control-max-age").unwrap(), "3600");
    }
}
