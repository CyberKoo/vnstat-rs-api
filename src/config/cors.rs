use super::traits::ConfigEntity;
use serde::Deserialize;

/// Configuration for Cross-Origin Resource Sharing (CORS).
///
/// Controls which origins, methods, headers, and credentials are allowed
/// when the API is accessed from a web browser.  All fields have sensible
/// defaults so that the layer can be enabled with minimal configuration.
#[derive(Debug, Deserialize)]
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

/// Returns the default value for the `enabled` field (`true`).
fn default_enabled() -> bool {
    false
}
