use clap::Parser;

/// Command-line arguments for the vnstat-rs-api application.
///
/// Parsed from the command line using [`clap::Parser`]. Both `--config <FILE>` and
/// `--debug` / `-d` are optional; sensible defaults are provided for each.
#[derive(Parser, Debug)]
#[command(name = env!("CARGO_PKG_DESCRIPTION"))]
#[command(author = env!("CARGO_PKG_AUTHORS"))]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(
    about = "A RESTful Web API wrapper for vnstat's network traffic monitoring.",
    long_about = "vnstat-rs-api: A Rust-based tool that converts vnstat's CLI into a RESTful API. \
    Provides endpoints to query network interfaces, traffic statistics (e.g., daily/monthly usage), \
    and updates via HTTP requests. Ideal for integration into monitoring systems, dashboards, \
    or automated scripts. Built for performance, safety, and extensibility in Rust."
)]
pub struct Args {
    #[arg(short, long, default_value = "config.toml", help = "Config file")]
    pub config: String,

    #[arg(short, long, default_value = "false", help = "Enable debug mode")]
    pub debug: bool,
}
