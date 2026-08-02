use clap::Parser;

/// Full version string reported by `--version`.
///
/// Includes the git branch and short commit hash when built inside a git
/// checkout (e.g. `1.2.0-dev@9bfa843`), falling back to the plain package
/// version for release builds outside one.
const VERSION: &str = if env!("GIT_BRANCH").is_empty() || env!("GIT_COMMIT").is_empty() {
    env!("CARGO_PKG_VERSION")
} else {
    concat!(
        env!("CARGO_PKG_VERSION"),
        "-",
        env!("GIT_BRANCH"),
        "@",
        env!("GIT_COMMIT")
    )
};

/// Command-line arguments for the vnstat-rs-api application.
///
/// Parsed from the command line using [`clap::Parser`]. Both `--config <FILE>` and
/// `--debug` / `-d` are optional; sensible defaults are provided for each.
#[derive(Parser, Debug)]
#[command(name = env!("CARGO_PKG_DESCRIPTION"))]
#[command(author = env!("CARGO_PKG_AUTHORS"))]
#[command(version = VERSION)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_defaults() {
        let args = Args::try_parse_from(["test"]).unwrap();
        assert_eq!(args.config, "config.toml");
        assert!(!args.debug);
    }

    #[test]
    fn parses_custom_args() {
        let args = Args::try_parse_from(["test", "-c", "/tmp/x.toml", "-d"]).unwrap();
        assert_eq!(args.config, "/tmp/x.toml");
        assert!(args.debug);
    }

    #[test]
    fn rejects_unknown_args() {
        assert!(Args::try_parse_from(["test", "--bogus"]).is_err());
    }
}
