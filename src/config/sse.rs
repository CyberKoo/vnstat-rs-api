use super::traits::ConfigEntity;
use anyhow::bail;
use serde::Deserialize;

/// Configuration for Server-Sent Events (SSE) live streams.
///
/// Controls the per-subscriber output buffer used while streaming live
/// traffic to clients.
#[derive(Debug, Deserialize)]
pub struct SseConfig {
    /// Buffer capacity (in messages) of each SSE subscriber's channel.
    ///
    /// A subscriber that cannot keep up simply loses messages (the reader
    /// uses `try_send`) instead of applying backpressure to the vnstat
    /// live-stream pipeline.
    #[serde(default = "default_subscriber_buffer")]
    pub subscriber_buffer: usize,
}

impl Default for SseConfig {
    /// Returns an `SseConfig` with the default subscriber buffer size (4).
    fn default() -> Self {
        SseConfig {
            subscriber_buffer: default_subscriber_buffer(),
        }
    }
}

impl ConfigEntity for SseConfig {
    /// Validates the subscriber buffer size.
    ///
    /// # Errors
    ///
    /// Returns an error if `subscriber_buffer` is zero (a zero-capacity
    /// channel would drop every message).
    fn validate(&self) -> anyhow::Result<()> {
        if self.subscriber_buffer == 0 {
            bail!("sse.subscriber_buffer must be greater than 0");
        }

        Ok(())
    }
}

/// Returns the default per-subscriber SSE buffer size (4 messages).
fn default_subscriber_buffer() -> usize {
    4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        assert_eq!(SseConfig::default().subscriber_buffer, 4);
    }

    #[test]
    fn validate_rejects_zero_buffer() {
        let c = SseConfig {
            subscriber_buffer: 0,
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn validate_accepts_positive_buffer() {
        let c = SseConfig {
            subscriber_buffer: 1,
        };
        assert!(c.validate().is_ok());
    }
}
