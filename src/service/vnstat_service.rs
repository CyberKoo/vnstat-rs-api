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
use tokio::sync::broadcast::error::RecvError;
use tracing::warn;

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
    pub async fn fetch_vnstat_data(&self) -> Result<VnstatData> {
        fetch_vnstat_data_cached(self.executable.clone(), self.timeout).await
    }

    /// Returns a list of all network interface names tracked by vnStat.
    pub async fn list_interfaces(&self) -> Result<Vec<String>> {
        let data = self.fetch_vnstat_data().await?;
        Ok(data.interfaces.iter().map(|i| i.name.clone()).collect())
    }

    /// Returns traffic statistics for a single interface.
    ///
    /// # Errors
    ///
    /// Returns an error if the interface is not found or the data fetch fails.
    pub async fn get_interface(&self, if_name: &str) -> Result<Interface> {
        let data = self.fetch_vnstat_data().await?;
        data.interfaces
            .into_iter()
            .find(|i| i.name == if_name)
            .ok_or_else(|| anyhow::anyhow!("interface not found: {}", if_name))
    }

    /// Returns the vnStat version string.
    pub async fn get_vnstat_version(&self) -> Result<String> {
        let data = self.fetch_vnstat_data().await?;
        Ok(data.vnstatversion)
    }

    /// Performs a quick health check by running `vnstat --json`.
    ///
    /// # Returns
    ///
    /// `Ok(())` if the vnStat executable responds successfully within the
    /// configured timeout.
    ///
    /// # Errors
    ///
    /// Returns an error if the vnStat subprocess fails, exits with a non-zero
    /// status code, or does not complete within the configured timeout.
    pub async fn check_health(&self) -> Result<()> {
        tokio::time::timeout(self.timeout, async {
            let output = tokio::process::Command::new(&self.executable)
                .arg("--json")
                .output()
                .await
                .context("failed to execute vnStat for health check")?;

            if !output.status.success() {
                anyhow::bail!(
                    "vnstat returned non-zero exit code: {:?}",
                    output.status.code()
                );
            }

            Ok(())
        })
        .await
        .context("vnstat health check timed out")?
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
        let stream_name = if_name.clone();

        Box::pin(stream! {
            let (mut receiver, _guard) = registry.subscribe(if_name.clone(), cmd).await;

            loop {
                match receiver.recv().await {
                    Ok(message) => match message {
                        TaskMessage::Data(data) => yield Ok(Event::default().data(data).id(timestamp::get_in_ms().to_string())),
                        TaskMessage::Error(error) => yield Err(error),
                        TaskMessage::Eof => break
                    },
                    Err(RecvError::Closed) => break,
                    Err(RecvError::Lagged(n)) => {
                        warn!("SSE [{}] message lagged: dropped {} messages", stream_name, n);
                        yield Ok(Event::default().comment("Message dropped (lag)"));
                    }
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
