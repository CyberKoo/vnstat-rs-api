use crate::args::Args;
use crate::router::AppState;
use anyhow::Context;
use axum::Router;
use axum::routing::get;
use clap::Parser;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::signal;
use tokio_util::sync::CancellationToken;
use tower_http::trace::TraceLayer;
use tracing::info;

mod api_error;
mod args;
mod config;
mod error_code;
mod logging;
mod model;
mod router;
mod service;
mod task_registry;
mod utils;

#[cfg(test)]
mod test_support;

/// Entry point for the vnstat-rs API server.
///
/// Parses command-line arguments, initialises logging, loads configuration,
/// Entry point for the vnstat-rs API server.
///
/// Parses command-line arguments and delegates to [`run`].
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    run(args).await
}

/// Starts the HTTP server with the given arguments.
///
/// Initialises logging, loads configuration, and starts the Axum HTTP
/// server with graceful shutdown support.  The server binds to the address
/// and port specified in the configuration file and exposes all routes
/// under `/api/v1`.
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
async fn run(args: Args) -> anyhow::Result<()> {
    logging::init(args.debug)?;
    tracing::debug!("debug mode enabled");

    let config = config::load_config(&args.config).context("Failed to load configuration")?;
    info!("Configuration loaded successfully");

    let vnstat = Arc::new(service::vnstat_service::VnstatService::new(
        config.vnstat.executable,
        config.vnstat.query_timeout_secs,
    ));
    let task_registry = Arc::new(task_registry::TaskRegistry::new(
        config.sse.subscriber_buffer,
    ));

    // Cancelled when a shutdown signal arrives; long-lived SSE streams end
    // when it fires so graceful shutdown can drain in-flight connections.
    let shutdown_token = CancellationToken::new();

    let app_state = AppState {
        vnstat,
        task_registry,
        shutdown_token: shutdown_token.clone(),
    };

    let app = Router::new()
        .route("/", get(router::home))
        .nest("/api/v1", router::get_router())
        .fallback(router::not_found)
        .method_not_allowed_fallback(router::method_not_allowed)
        .layer(TraceLayer::new_for_http());

    // Apply CORS layer based on configuration.
    let app = if config.cors.enabled {
        app.layer(config.cors.to_layer())
    } else {
        app
    };

    let app = app.with_state(app_state);

    let listener = tokio::net::TcpListener::bind(config.server.to_socket_addr()?)
        .await
        .context("Failed to bind socket")?;

    info!("Server listening on {}", listener.local_addr().unwrap());

    let serve_result = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(shutdown_token))
    .await;

    serve_result.context("server failed to start")?;

    info!("Server shut down gracefully");

    Ok(())
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
///
/// When a signal is received the token is cancelled *before* this future
/// completes, so handlers that observe the token (e.g. SSE streams) start
/// ending while axum stops accepting new connections and drains the rest.
async fn shutdown_signal(token: CancellationToken) {
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

    // End long-lived SSE streams so their connections can drain instead of
    // blocking graceful shutdown indefinitely.
    token.cancel();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;

    fn free_port() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    }

    /// Serialises the end-to-end `run()` tests: they send SIGTERM to the
    /// whole test process, so they must not overlap each other.
    static RUN_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    fn temp_config(port: u16, vnstat_path: &str) -> String {
        format!(
            "[server]\nlisten = \"127.0.0.1\"\nport = {}\n\n[vnstat]\nexecutable = \"{}\"\nquery_timeout_secs = 5\n",
            port, vnstat_path
        )
    }

    fn temp_config_with_cors(port: u16, vnstat_path: &str) -> String {
        format!(
            "[server]\nlisten = \"127.0.0.1\"\nport = {}\n\n[vnstat]\nexecutable = \"{}\"\nquery_timeout_secs = 5\n\n[cors]\nenabled = true\nallowed_origins = [\"http://localhost:5173\"]\n",
            port, vnstat_path
        )
    }

    #[tokio::test]
    async fn run_with_cors_enabled_starts_and_shuts_down() {
        let _guard = RUN_LOCK.lock().await;
        let port = free_port();
        let script = test_support::fake_vnstat_script();
        let config_path = test_support::write_temp_file(
            "run-config-cors",
            &temp_config_with_cors(port, script.to_str().unwrap()),
        );

        let pid = std::process::id();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let _ = std::process::Command::new("/bin/kill")
                .args(["-TERM", &pid.to_string()])
                .status();
        });

        let args = Args {
            config: config_path.to_str().unwrap().to_string(),
            debug: false,
        };
        let result = run(args).await;
        assert!(
            result.is_ok(),
            "server with CORS enabled should shut down gracefully, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn run_starts_server_and_shuts_down_gracefully_on_sigterm() {
        let _guard = RUN_LOCK.lock().await;
        let port = free_port();
        let script = test_support::fake_vnstat_script();
        let config_path = test_support::write_temp_file(
            "run-config",
            &temp_config(port, script.to_str().unwrap()),
        );

        // Send SIGTERM after the server has had time to start.
        let pid = std::process::id();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let _ = std::process::Command::new("/bin/kill")
                .args(["-TERM", &pid.to_string()])
                .status();
        });

        let args = Args {
            config: config_path.to_str().unwrap().to_string(),
            debug: false,
        };
        let result = run(args).await;
        assert!(
            result.is_ok(),
            "server should shut down gracefully, got: {:?}",
            result
        );
    }

    /// Regression test: graceful shutdown must complete even when an SSE
    /// connection is active. Before the fix, `axum::serve` waited forever
    /// for the long-lived stream to drain and the server never stopped.
    #[tokio::test]
    async fn run_shuts_down_gracefully_with_active_sse_connection() {
        let _guard = RUN_LOCK.lock().await;
        let port = free_port();
        let script = test_support::fake_vnstat_script();
        let config_path = test_support::write_temp_file(
            "run-config-sse",
            &temp_config(port, script.to_str().unwrap()),
        );

        // Start the server first so the SIGTERM handler is installed before
        // the signal is sent (the test runtime is current-thread, so we must
        // `tokio::time::sleep` later rather than block the thread).
        let args = Args {
            config: config_path.to_str().unwrap().to_string(),
            debug: false,
        };
        let server = tokio::spawn(run(args));

        // A tiny HTTP/1.1 SSE client on a separate thread: it opens the live
        // stream, reads events, and waits for the server to close the
        // connection.
        let client = std::thread::spawn(move || {
            use std::io::{BufRead, BufReader, Write};
            use std::net::TcpStream;
            use std::time::{Duration, Instant};

            let deadline = Instant::now() + Duration::from_secs(10);
            let mut tcp = loop {
                match TcpStream::connect(("127.0.0.1", port)) {
                    Ok(tcp) => break tcp,
                    Err(_) if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    Err(e) => panic!("failed to connect to server: {e}"),
                }
            };
            tcp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            write!(
                tcp,
                "GET /api/v1/interfaces/eth0/live HTTP/1.1\r\n\
                 Host: localhost\r\n\
                 Accept: text/event-stream\r\n\r\n"
            )
            .unwrap();

            let mut reader = BufReader::new(tcp);
            let mut line = String::new();
            // Drain the response headers.
            loop {
                line.clear();
                let n = reader.read_line(&mut line).unwrap();
                assert!(n > 0, "server closed the connection before sending headers");
                if line == "\r\n" {
                    break;
                }
            }
            // Read SSE lines until the server closes the stream (EOF).
            let (mut saw_event, mut saw_farewell) = (false, false);
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break, // EOF: server closed the connection
                    Ok(_) => {
                        if line.contains("jsonversion") {
                            saw_event = true;
                        }
                        if line.contains("shutting down") {
                            saw_farewell = true;
                        }
                    }
                    Err(e) => panic!("error reading SSE stream: {e}"),
                }
            }
            (saw_event, saw_farewell)
        });

        // Let the stream establish and deliver a few events, then shut down.
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let pid = std::process::id();
        std::process::Command::new("/bin/kill")
            .args(["-TERM", &pid.to_string()])
            .status()
            .unwrap();

        let result = tokio::time::timeout(std::time::Duration::from_secs(20), server)
            .await
            .expect("server should shut down gracefully despite the active SSE connection")
            .expect("server task failed");
        assert!(
            result.is_ok(),
            "server should shut down gracefully, got: {:?}",
            result
        );

        let (saw_event, saw_farewell) = client.join().unwrap();
        assert!(saw_event, "SSE client should have received live events");
        assert!(
            saw_farewell,
            "SSE client should have received the shutdown farewell event"
        );
    }
}
