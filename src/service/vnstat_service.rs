use crate::model::vnstat::{Interface, VnstatData};
use crate::task_registry::{TaskMessage, TaskRegistry};
use crate::utils::timestamp;
use anyhow::{Context, Result};
use async_stream::stream;
use axum::response::sse::Event;
use cached::cached;
use futures_util::Stream;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tracing::warn;

/// A typed vnstat query failure.
///
/// Distinguishes server-side failures (subprocess, timeout, parsing) from
/// client-side failures (unknown interface) so handlers can map them to
/// the correct HTTP status code.
#[derive(Debug)]
pub enum VnstatError {
    /// The vnstat subprocess failed, timed out, or produced unparseable output.
    FetchFailed(anyhow::Error),
    /// The requested interface is not tracked by vnstat.
    InterfaceNotFound(String),
}

impl std::fmt::Display for VnstatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VnstatError::FetchFailed(e) => write!(f, "vnstat data fetch failed: {}", e),
            VnstatError::InterfaceNotFound(name) => write!(f, "no such interface: {}", name),
        }
    }
}

impl std::error::Error for VnstatError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            VnstatError::FetchFailed(e) => Some(e.as_ref()),
            VnstatError::InterfaceNotFound(_) => None,
        }
    }
}

/// Service for interacting with the `vnstat` command-line tool.
///
/// Encapsulates vnStat data fetching, interface queries, health checks, and
/// live-traffic streaming.  Uses a single-entry, 60-second TTL cache for
/// periodic data queries to avoid redundant subprocess invocations.
pub struct VnstatService {
    executable: String,
    timeout: Duration,
}

impl VnstatService {
    /// Creates a new `VnstatService`.
    ///
    /// # Arguments
    ///
    /// * `executable` - Path or name of the `vnstat` binary.
    /// * `timeout_secs` - Timeout in seconds for vnstat subprocess execution.
    pub fn new(executable: impl Into<String>, timeout_secs: u64) -> Self {
        Self {
            executable: executable.into(),
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    /// Fetches the full vnStat data JSON (cached for 60 seconds).
    ///
    /// Delegates to the cached free function [`fetch_vnstat_data_cached`].
    pub async fn fetch_vnstat_data(&self) -> Result<VnstatData, VnstatError> {
        fetch_vnstat_data_cached(self.executable.clone(), self.timeout)
            .await
            .map_err(VnstatError::FetchFailed)
    }

    /// Returns a list of all network interface names tracked by vnStat.
    pub async fn list_interfaces(&self) -> Result<Vec<String>, VnstatError> {
        let data = self.fetch_vnstat_data().await?;
        Ok(data.interfaces.iter().map(|i| i.name.clone()).collect())
    }

    /// Returns traffic statistics for a single interface.
    ///
    /// # Errors
    ///
    /// Returns an error if the interface is not found or the data fetch fails.
    pub async fn get_interface(&self, if_name: &str) -> Result<Interface, VnstatError> {
        let data = self.fetch_vnstat_data().await?;
        data.interfaces
            .into_iter()
            .find(|i| i.name == if_name)
            .ok_or_else(|| VnstatError::InterfaceNotFound(if_name.to_string()))
    }

    /// Returns the vnStat version string.
    pub async fn get_vnstat_version(&self) -> Result<String, VnstatError> {
        let data = self.fetch_vnstat_data().await?;
        Ok(data.vnstatversion)
    }

    /// Builds the command-line arguments required to start a live-traffic
    /// stream for a given interface.
    pub fn build_live_stream_command(&self, if_name: impl AsRef<str>) -> Result<Vec<String>> {
        Ok(vec![
            self.executable.clone(),
            "-i".to_string(),
            if_name.as_ref().to_string(),
            "--json".to_string(),
            "-l".to_string(),
        ])
    }

    /// Returns a streaming response of Server-Sent Events (SSE) that yields
    /// live traffic statistics for the requested interface.
    ///
    /// If the client cannot keep up with the stream, messages are dropped for
    /// that client instead of applying backpressure to the vnstat subprocess.
    pub async fn stream_interface_live_stats(
        &self,
        registry: Arc<TaskRegistry>,
        if_name: String,
    ) -> Pin<Box<dyn Stream<Item = Result<Event, String>> + Send>> {
        let cmd = match self.build_live_stream_command(&if_name) {
            Ok(cmd) => cmd,
            Err(e) => {
                warn!("Failed to build live stream command: {}", e);
                return Box::pin(futures_util::stream::once(async move {
                    Err(format!("Failed to start live stream: {}", e))
                }));
            }
        };

        Box::pin(stream! {
            let (mut receiver, _guard) = registry.subscribe(if_name.clone(), cmd).await;

            // The channel closes when the process ends or all other
            // subscribers leave; slow consumers have their messages dropped
            // upstream instead of blocking the pipeline.
            while let Some(message) = receiver.recv().await {
                match message {
                    TaskMessage::Data(data) => yield Ok(Event::default().data(data).id(timestamp::get_in_ms().to_string())),
                    TaskMessage::Error(error) => yield Err(error),
                    TaskMessage::Eof => break,
                }
            }
        })
    }
}

/// Fetches vnStat data with a single-entry, 60-second in-memory cache.
///
/// The cache key is the executable path and timeout (both constant at runtime).
/// Repeated calls within the 60-second window return the cached result without
/// re-invoking the subprocess. The TTL refreshes on every cache hit.
///
/// # Errors
///
/// Returns an error if:
/// * The vnStat subprocess fails to start or returns a non-zero exit code.
/// * The command times out.
/// * The stdout is not valid UTF-8.
/// * The JSON payload cannot be deserialized into [`VnstatData`].
#[cached(max_size = 1, ttl = 60, refresh = true)]
async fn fetch_vnstat_data_cached(executable: String, timeout: Duration) -> Result<VnstatData> {
    let output = tokio::time::timeout(timeout, async {
        tokio::process::Command::new(executable)
            .arg("--json")
            .output()
            .await
            .context("failed to execute vnStat")
    })
    .await
    .context("vnstat command timed out")?
    .context("failed to execute vnStat")?;

    let json_str =
        String::from_utf8(output.stdout).context("failed to parse vnStat response as UTF-8")?;

    serde_json::from_str(&json_str).context("failed to deserialize vnStat JSON response")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;
    use futures_util::StreamExt;
    use std::error::Error;
    use std::path::Path;

    fn service(script: &Path, timeout: u64) -> VnstatService {
        VnstatService::new(script.to_str().unwrap().to_string(), timeout)
    }

    #[tokio::test]
    async fn fetches_and_parses_vnstat_data() {
        let s = service(&test_support::fake_vnstat_script(), 5);
        let data = s.fetch_vnstat_data().await.unwrap();
        assert_eq!(data.vnstatversion, "2.13");
        assert_eq!(data.interfaces.len(), 2);
    }

    #[tokio::test]
    async fn fetch_is_cached_between_calls() {
        let s = service(&test_support::fake_vnstat_script(), 5);
        let first = s.fetch_vnstat_data().await.unwrap();
        let second = s.fetch_vnstat_data().await.unwrap();
        assert_eq!(first.interfaces[0].name, second.interfaces[0].name);
    }

    #[tokio::test]
    async fn fetch_failure_when_output_is_garbage() {
        let s = service(&test_support::garbage_vnstat_script(), 5);
        let err = s.fetch_vnstat_data().await.unwrap_err();
        assert!(matches!(err, VnstatError::FetchFailed(_)));
    }

    #[tokio::test]
    async fn fetch_failure_when_command_times_out() {
        let s = service(&test_support::slow_vnstat_script(), 1);
        let err = s.fetch_vnstat_data().await.unwrap_err();
        assert!(matches!(err, VnstatError::FetchFailed(_)));
    }

    #[tokio::test]
    async fn lists_interfaces_and_versions() {
        let s = service(&test_support::fake_vnstat_script(), 5);
        assert_eq!(s.list_interfaces().await.unwrap(), vec!["eth0", "wlan0"]);
        assert_eq!(s.get_vnstat_version().await.unwrap(), "2.13");
    }

    #[tokio::test]
    async fn get_interface_returns_matching_interface() {
        let s = service(&test_support::fake_vnstat_script(), 5);
        let iface = s.get_interface("eth0").await.unwrap();
        assert_eq!(iface.name, "eth0");
        assert_eq!(iface.traffic.total.rx, 123456789);
    }

    #[tokio::test]
    async fn get_interface_returns_typed_not_found() {
        let s = service(&test_support::fake_vnstat_script(), 5);
        let err = s.get_interface("eth9").await.unwrap_err();
        assert!(matches!(err, VnstatError::InterfaceNotFound(name) if name == "eth9"));
    }

    #[tokio::test]
    async fn build_live_stream_command_orders_args() {
        let script = test_support::fake_vnstat_script();
        let s = service(&script, 5);
        let cmd = s.build_live_stream_command("eth0").unwrap();
        assert_eq!(cmd[0], script.to_str().unwrap());
        assert_eq!(&cmd[1..], &["-i", "eth0", "--json", "-l"]);
    }

    #[tokio::test]
    async fn streams_live_events() {
        let s = service(&test_support::fake_vnstat_script(), 5);
        let registry = Arc::new(TaskRegistry::new(4));
        let mut stream = s.stream_interface_live_stats(registry, "eth0".into()).await;

        // Collect three live events; each must arrive quickly and be an Ok event.
        for _ in 0..3 {
            let item = tokio::time::timeout(Duration::from_secs(3), stream.next())
                .await
                .expect("stream should yield within timeout")
                .expect("stream should not end");
            let _ = item.expect("no stream error");
        }
    }

    #[test]
    fn error_display_and_source() {
        let err = VnstatError::FetchFailed(anyhow::anyhow!("boom"));
        assert!(err.to_string().contains("vnstat data fetch failed"));
        assert!(err.source().is_some());

        let nf = VnstatError::InterfaceNotFound("eth0".into());
        assert_eq!(nf.to_string(), "no such interface: eth0");
        assert!(nf.source().is_none());
    }
}
