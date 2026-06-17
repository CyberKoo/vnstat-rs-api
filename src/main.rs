use crate::args::Args;
use crate::router::AppState;
use anyhow::Context;
use axum::Router;
use clap::Parser;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::signal;
use tower_http::cors::{
    AllowCredentials, AllowHeaders, AllowMethods, AllowOrigin, CorsLayer, ExposeHeaders,
};
use tower_http::trace::TraceLayer;
use tracing::info;

mod args;
mod config;
mod error_code;
mod logging;
mod model;
mod router;
mod service;
mod task_handle;
mod task_manager;
mod utils;

/// Entry point for the vnstat-rs API server.
///
/// Parses command-line arguments, initialises logging, loads configuration,
/// and starts the Axum HTTP server with graceful shutdown support.  The
/// server binds to the address and port specified in the configuration file
/// and exposes all routes under `/api/v1`.
///
/// # Returns
///
/// * `Ok(())` — The server shut down gracefully (e.g. via Ctrl+C or SIGTERM).
///
/// # Errors
///
/// * Returns an error if logging initialisation fails.
/// * Returns an error if the configuration file cannot be loaded or parsed.
/// * Returns an error if the server socket cannot be bound.
/// * Returns an error if the server encounters a fatal runtime failure.
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    logging::init(args.debug)?;
    tracing::debug!("debug mode enabled");

    let config = config::load_config(&args.config).context("Failed to load configuration")?;
    info!("Configuration loaded successfully");

    let vnstat = Arc::new(service::vnstat_service::VnstatService::new(
        config.vnstat.executable,
    ));
    let task_manager = Arc::new(task_manager::TaskManager::new());

    let app_state = AppState {
        vnstat,
        task_manager,
    };

    let app = Router::new()
        .nest("/api/v1", router::get_router())
        .layer(TraceLayer::new_for_http());

    // Apply CORS layer based on configuration.
    let app = if config.cors.enabled {
        let cors = build_cors_layer(&config.cors);
        app.layer(cors)
    } else {
        app
    };

    let app = app.with_state(app_state);

    let listener = tokio::net::TcpListener::bind(config.server.to_socket_addr()?)
        .await
        .context("Failed to bind socket")?;

    info!("Server listening on {}", listener.local_addr().unwrap());

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("server failed to start")?;

    info!("Server shut down gracefully");

    Ok(())
}

/// Build a [`CorsLayer`] from the application's CORS configuration.
///
/// Translates the values in [`CorsConfig`] into the corresponding
/// `tower-http` CORS middleware.  When `allowed_origins` is empty the
/// layer uses `AllowOrigin::any()` (or `AllowOrigin::mirror_request()` if
/// credentials are enabled, since the CORS spec forbids a wildcard origin
/// with credentials).
///
/// # Arguments
///
/// * `cors_config` - A reference to the deserialised CORS configuration
///   containing allowed origins, methods, headers, exposed headers,
///   credentials, and an optional max-age.
///
/// # Returns
///
/// A fully configured [`CorsLayer`] ready to be applied to the Axum router.
fn build_cors_layer(cors_config: &config::cors::CorsConfig) -> CorsLayer {
    let mut layer = CorsLayer::new();

    // --- allowed_origins ---
    if cors_config.allowed_origins.is_empty() {
        // No specific origins → allow all.
        // NOTE: If credentials are also enabled, wildcard won't work per CORS spec;
        // in that case we use mirror_request which echoes back the request's Origin.
        if cors_config.allow_credentials {
            layer = layer.allow_origin(AllowOrigin::mirror_request());
        } else {
            layer = layer.allow_origin(AllowOrigin::any());
        }
    } else {
        let origins: Vec<_> = cors_config
            .allowed_origins
            .iter()
            .filter_map(|o| axum::http::HeaderValue::from_str(o).ok())
            .collect();
        layer = layer.allow_origin(AllowOrigin::list(origins));
    }

    // --- allowed_methods ---
    if cors_config.allowed_methods.is_empty() {
        layer = layer.allow_methods(AllowMethods::any());
    } else {
        let methods: Vec<_> = cors_config
            .allowed_methods
            .iter()
            .filter_map(|m| axum::http::Method::from_bytes(m.as_bytes()).ok())
            .collect();
        layer = layer.allow_methods(AllowMethods::list(methods));
    }

    // --- allowed_headers ---
    if cors_config.allowed_headers.is_empty() {
        layer = layer.allow_headers(AllowHeaders::any());
    } else {
        let headers: Vec<_> = cors_config
            .allowed_headers
            .iter()
            .filter_map(|h| axum::http::HeaderName::from_bytes(h.as_bytes()).ok())
            .collect();
        layer = layer.allow_headers(AllowHeaders::list(headers));
    }

    // --- expose_headers ---
    if !cors_config.expose_headers.is_empty() {
        let headers: Vec<_> = cors_config
            .expose_headers
            .iter()
            .filter_map(|h| axum::http::HeaderName::from_bytes(h.as_bytes()).ok())
            .collect();
        layer = layer.expose_headers(ExposeHeaders::list(headers));
    }

    // --- allow_credentials ---
    if cors_config.allow_credentials {
        layer = layer.allow_credentials(AllowCredentials::yes());
    }

    // --- max_age ---
    if let Some(max_age) = cors_config.max_age {
        layer = layer.max_age(std::time::Duration::from_secs(max_age));
    }

    layer
}

/// Wait for a shutdown signal (Ctrl+C or SIGTERM) and initiate graceful
/// server shutdown.
///
/// On Unix platforms this function listens for both `SIGINT` (Ctrl+C) and
/// `SIGTERM`.  On non-Unix platforms only Ctrl+C is handled.  If either
/// signal handler cannot be installed a warning is logged and the future
/// never completes, effectively falling back to the other signal.
///
/// This function is designed to be used with
/// [`axum::serve::with_graceful_shutdown`].
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = signal::ctrl_c().await {
            tracing::warn!("Failed to install Ctrl+C handler: {}", e);
            std::future::pending::<()>().await
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => sig.recv().await,
            Err(e) => {
                tracing::warn!("Failed to install SIGTERM handler: {}", e);
                let _: () = std::future::pending::<()>().await;
                Some(())
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Shutdown signal received, gracefully stopping...");
}
