use crate::config::AppConfig;
use crate::config::cors::CorsConfig;
use crate::router::{self, AppState};
use axum::Router;
use axum::routing::get;
use tower_http::cors::{
    AllowCredentials, AllowHeaders, AllowMethods, AllowOrigin, CorsLayer, ExposeHeaders,
};
use tower_http::trace::TraceLayer;

/// Builds the HTTP CORS middleware from validated application settings.
///
/// This adapter belongs to the application/Web boundary rather than the
/// configuration model, which remains framework-agnostic.
pub(crate) fn cors_layer(config: &CorsConfig) -> CorsLayer {
    let mut layer = CorsLayer::new();

    if config.allowed_origins.is_empty() {
        layer = if config.allow_credentials {
            layer.allow_origin(AllowOrigin::mirror_request())
        } else {
            layer.allow_origin(AllowOrigin::any())
        };
    } else {
        let origins: Vec<_> = config
            .allowed_origins
            .iter()
            .filter_map(|origin| axum::http::HeaderValue::from_str(origin).ok())
            .collect();
        layer = layer.allow_origin(AllowOrigin::list(origins));
    }

    if config.allowed_methods.is_empty() {
        layer = if config.allow_credentials {
            layer.allow_methods(AllowMethods::mirror_request())
        } else {
            layer.allow_methods(AllowMethods::any())
        };
    } else {
        let methods: Vec<_> = config
            .allowed_methods
            .iter()
            .filter_map(|method| axum::http::Method::from_bytes(method.as_bytes()).ok())
            .collect();
        layer = layer.allow_methods(AllowMethods::list(methods));
    }

    if config.allowed_headers.is_empty() {
        layer = if config.allow_credentials {
            layer.allow_headers(AllowHeaders::mirror_request())
        } else {
            layer.allow_headers(AllowHeaders::any())
        };
    } else {
        let headers: Vec<_> = config
            .allowed_headers
            .iter()
            .filter_map(|header| axum::http::HeaderName::from_bytes(header.as_bytes()).ok())
            .collect();
        layer = layer.allow_headers(AllowHeaders::list(headers));
    }

    if !config.expose_headers.is_empty() {
        let headers: Vec<_> = config
            .expose_headers
            .iter()
            .filter_map(|header| axum::http::HeaderName::from_bytes(header.as_bytes()).ok())
            .collect();
        layer = layer.expose_headers(ExposeHeaders::list(headers));
    }

    if config.allow_credentials {
        layer = layer.allow_credentials(AllowCredentials::yes());
    }
    if let Some(max_age) = config.max_age {
        layer = layer.max_age(std::time::Duration::from_secs(max_age));
    }

    layer
}

/// Builds the HTTP application from validated configuration and shared state.
pub fn build_app(config: &AppConfig, state: AppState) -> Router {
    let app = Router::new()
        .route("/", get(router::home))
        .nest("/api/v1", router::get_router())
        .fallback(router::not_found)
        .method_not_allowed_fallback(router::method_not_allowed)
        .layer(TraceLayer::new_for_http());

    let app = if config.cors.enabled {
        app.layer(cors_layer(&config.cors))
    } else {
        app
    };

    app.with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::vnstat_service::VnstatService;
    use crate::task_registry::TaskRegistry;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    #[test]
    fn builds_application_with_default_configuration() {
        let config: AppConfig = toml::from_str("[server]\n").unwrap();
        let state = AppState {
            vnstat: Arc::new(VnstatService::new(
                "/bin/false",
                1,
                Arc::new(TaskRegistry::new(4)),
            )),
            link_speed: Default::default(),
            shutdown_token: CancellationToken::new(),
        };

        let _ = build_app(&config, state);
    }
}
