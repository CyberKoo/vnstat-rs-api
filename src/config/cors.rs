use super::traits::ConfigEntity;
use serde::Deserialize;
use tower_http::cors::{
    AllowCredentials, AllowHeaders, AllowMethods, AllowOrigin, CorsLayer, ExposeHeaders,
};

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

impl ConfigEntity for CorsConfig {}

impl CorsConfig {
    /// Builds a [`CorsLayer`] from this configuration.
    ///
    /// Translates the values in this config into the corresponding
    /// `tower-http` CORS middleware. When `allowed_origins` is empty the
    /// layer uses `AllowOrigin::any()` (or `AllowOrigin::mirror_request()` if
    /// credentials are enabled, since the CORS spec forbids a wildcard origin
    /// with credentials). Invalid origin/method/header strings are skipped.
    pub fn to_layer(&self) -> CorsLayer {
        let mut layer = CorsLayer::new();

        // --- allowed_origins ---
        if self.allowed_origins.is_empty() {
            // No specific origins → allow all.
            // NOTE: If credentials are also enabled, wildcard won't work per CORS spec;
            // in that case we use mirror_request which echoes back the request's Origin.
            if self.allow_credentials {
                layer = layer.allow_origin(AllowOrigin::mirror_request());
            } else {
                layer = layer.allow_origin(AllowOrigin::any());
            }
        } else {
            let origins: Vec<_> = self
                .allowed_origins
                .iter()
                .filter_map(|o| axum::http::HeaderValue::from_str(o).ok())
                .collect();
            layer = layer.allow_origin(AllowOrigin::list(origins));
        }

        // --- allowed_methods ---
        if self.allowed_methods.is_empty() {
            // Wildcards are invalid with credentials; mirror the request
            // method instead (tower-http panics on the combination).
            if self.allow_credentials {
                layer = layer.allow_methods(AllowMethods::mirror_request());
            } else {
                layer = layer.allow_methods(AllowMethods::any());
            }
        } else {
            let methods: Vec<_> = self
                .allowed_methods
                .iter()
                .filter_map(|m: &String| axum::http::Method::from_bytes(m.as_bytes()).ok())
                .collect();
            layer = layer.allow_methods(AllowMethods::list(methods));
        }

        // --- allowed_headers ---
        if self.allowed_headers.is_empty() {
            // Same credential constraint as methods above.
            if self.allow_credentials {
                layer = layer.allow_headers(AllowHeaders::mirror_request());
            } else {
                layer = layer.allow_headers(AllowHeaders::any());
            }
        } else {
            let headers: Vec<_> = self
                .allowed_headers
                .iter()
                .filter_map(|h: &String| axum::http::HeaderName::from_bytes(h.as_bytes()).ok())
                .collect();
            layer = layer.allow_headers(AllowHeaders::list(headers));
        }

        // --- expose_headers ---
        if !self.expose_headers.is_empty() {
            let headers: Vec<_> = self
                .expose_headers
                .iter()
                .filter_map(|h: &String| axum::http::HeaderName::from_bytes(h.as_bytes()).ok())
                .collect();
            layer = layer.expose_headers(ExposeHeaders::list(headers));
        }

        // --- allow_credentials ---
        if self.allow_credentials {
            layer = layer.allow_credentials(AllowCredentials::yes());
        }

        // --- max_age ---
        if let Some(max_age) = self.max_age {
            layer = layer.max_age(std::time::Duration::from_secs(max_age));
        }

        layer
    }
}

/// Returns the default value for the `enabled` field (`true`).
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
        Router::new().layer(cfg.to_layer())
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
