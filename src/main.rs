use crate::args::Args;
use anyhow::Context;
use axum::Router;
use clap::Parser;
use log::info;
use std::net::SocketAddr;

mod args;
mod config;
mod error_code;
mod logging;
mod model;
mod router;
mod service;
mod task_manager;
mod task_handle;
mod utils;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Initialize the logging system with the specified debug level
    logging::init(args.debug)?;
    log::debug!("debug mode enabled");

    let config = config::load_config(&args.config).context("Failed to load configuration")?;
    info!("Configuration loaded successfully");

    // set vnstat service executable
    service::vnstat_service::set_vnstat_executable(&config.vnstat.executable)?;

    // initialize tracing
    // tracing_subscriber::fmt::init();

    // build our application with a route
    let app = Router::new().nest_service("/api/v1", router::get_router());

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind(config.server.to_socket_addr()?)
        .await
        .context("Failed to bind socket")?;

    // server
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .context("server failed to start")?;

    Ok(())
}
