use std::time::{SystemTime, UNIX_EPOCH};

/// Returns the current system time as a millisecond timestamp since the Unix
/// epoch.
///
/// The value is calculated as `SystemTime::now().duration_since(UNIX_EPOCH)`
/// and falls back to `0` when the system clock is set before the Unix epoch.
///
/// * Returns: The number of whole milliseconds since `1970-01-01 00:00:00
///   UTC`.
pub fn get_in_ms() -> u128 {
    let start = SystemTime::now();
    let since_the_epoch = start.duration_since(UNIX_EPOCH).unwrap_or_default();

    since_the_epoch.as_millis()
}
