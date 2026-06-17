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
    let filter = if debug {
        EnvFilter::builder()
            .with_default_directive(tracing::Level::DEBUG.into())
            .from_env_lossy()
    } else {
        EnvFilter::builder()
            .with_default_directive(tracing::Level::INFO.into())
            .from_env_lossy()
    };

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
        .init();

    Ok(())
}
