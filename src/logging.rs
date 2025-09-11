use anyhow::{Context, Result};
use log::LevelFilter;
use simplelog::{format_description, Config, ConfigBuilder, SimpleLogger};
use time::format_description::FormatItem;
use time::UtcOffset;

pub fn init(debug: bool) -> Result<()> {
    SimpleLogger::init(
        if debug {
            LevelFilter::Debug
        } else {
            LevelFilter::Info
        },
        get_config()?, // Retrieve the logger configuration.
    )
    .context("Failed to initialize logger")?;

    Ok(())
}

fn get_config() -> Result<Config> {
    Ok(ConfigBuilder::new()
        .set_time_offset(get_local_offset()?) // Set local time offset.
        .set_time_format_custom(get_time_format()) // Use a custom time format.
        .set_level_padding(simplelog::LevelPadding::Right) // Right-align log levels.
        .set_thread_level(LevelFilter::Off) // Disable thread information.
        .set_target_level(LevelFilter::Debug) // Show module path for Debug level and above.
        .set_write_log_enable_colors(true) // Enable colored log output.
        .build())
}

fn get_local_offset() -> Result<UtcOffset> {
    UtcOffset::current_local_offset().context("Failed to get current local offset")
}

fn get_time_format() -> &'static [FormatItem<'static>] {
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second].[subsecond digits:3]")
}
