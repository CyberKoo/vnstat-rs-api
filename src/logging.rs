use anyhow::{Context, Result};
use time::UtcOffset;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;

/// Initialises the global [`tracing`] subscriber with opinionated defaults.
///
/// The subscriber writes structured, human-readable log lines to stderr and
/// includes a local-timestamp, the target module, the log level, and ANSI
/// colouring.  When `debug` is `true` the default filter is set to `DEBUG`;
/// otherwise it is set to `INFO`.
///
/// # Arguments
///
/// * `debug` — If `true`, the log level defaults to [`tracing::Level::DEBUG`];
///   otherwise it defaults to [`tracing::Level::INFO`].  In either case the
///   effective filter can still be overridden via the `RUST_LOG` environment
///   variable.
///
/// # Errors
///
/// Returns an error if:
/// * The timestamp format string cannot be parsed by the `time` crate.
/// * The current local UTC offset cannot be determined (e.g. on systems where
///   local time information is unavailable).
///
/// Once the subscriber is installed this function cannot be called again
/// (a subsequent call will panic).
pub fn init(debug: bool) -> Result<()> {
    let filter = build_env_filter(debug);

    #[allow(deprecated)]
    let format = time::format_description::parse(
        "[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:3]",
    )
    .context("Failed to parse time format")?;
    let offset = UtcOffset::current_local_offset().context("Failed to get current local offset")?;
    let timer = fmt::time::OffsetTime::new(offset, format);

    fmt()
        .with_env_filter(filter)
        .with_timer(timer)
        .with_target(true)
        .with_level(true)
        .with_ansi(true)
        .try_init()
        .unwrap_or_else(|e| {
            // A subscriber is already installed (e.g. tests running multiple
            // server instances in one process): keep the existing one and
            // continue instead of panicking.
            tracing::warn!("tracing subscriber already set: {}", e);
        });

    Ok(())
}

/// Builds the log filter for the given debug flag.
///
/// The effective filter can be overridden via the `RUST_LOG` environment
/// variable; the `debug` flag only sets the fallback default level.
fn build_env_filter(debug: bool) -> EnvFilter {
    let default_level = if debug {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };

    EnvFilter::builder()
        .with_default_directive(default_level.into())
        .from_env_lossy()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_defaults_to_debug_level() {
        let filter = build_env_filter(true);
        let directive = filter.to_string();
        assert!(directive.contains("debug"));
    }

    #[test]
    fn release_defaults_to_info_level() {
        let filter = build_env_filter(false);
        assert!(filter.to_string().contains("info"));
    }
}
