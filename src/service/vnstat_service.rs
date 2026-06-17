use crate::model::vnstat::{Interface, VnstatData};
use crate::task_handle::TaskMessage;
use crate::task_manager::TaskManager;
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

/// Timeout for vnstat command execution.
const VNSTAT_TIMEOUT: Duration = Duration::from_secs(15);

/// Service for interacting with the `vnstat` command-line tool.
///
/// Encapsulates vnStat data fetching, interface listing, health checks, and
/// live-traffic streaming. Uses a cached (1-entry, 60-second TTL) backend for
/// periodic data queries to avoid redundant subprocess invocations.
pub struct VnstatService {
    executable: String,
}

impl VnstatService {
    /// Creates a new `VnstatService`.
    ///
    /// # Arguments
    ///
    /// * `executable` - Path or name of the `vnstat` binary (e.g. `"vnstat"`).
    ///
    /// # Returns
    ///
    /// A new `VnstatService` instance that will delegate all vnStat calls to
    /// the given executable.
    pub fn new(executable: impl Into<String>) -> Self {
        Self {
            executable: executable.into(),
        }
    }

    /// Fetches the full vnStat data JSON.
    ///
    /// This is a public convenience wrapper around the cached free function
    /// [`fetch_vnstat_data_cached`], using the service's configured
    /// executable path as the cache key.
    ///
    /// # Returns
    ///
    /// A [`VnstatData`] struct deserialized from the `vnstat --json` output.
    ///
    /// # Errors
    ///
    /// Returns an error if the vnStat subprocess fails, times out, produces
    /// invalid UTF-8, or returns JSON that cannot be deserialized.
    pub async fn fetch_vnstat_data(&self) -> Result<VnstatData> {
        fetch_vnstat_data_cached(self.executable.clone()).await
    }

    /// Lists all network interfaces tracked by vnStat.
    ///
    /// # Returns
    ///
    /// A `Vec<String>` containing the names of every interface present in
    /// the vnStat database.
    ///
    /// # Errors
    ///
    /// Propagates any error from [`fetch_vnstat_data`](Self::fetch_vnstat_data).
    pub async fn list_vnstat_interfaces(&self) -> Result<Vec<String>> {
        let data = self.fetch_vnstat_data().await?;
        let interfaces: Vec<_> = data.interfaces.iter().map(|i| i.name.clone()).collect();

        Ok(interfaces)
    }

    /// Retrieves detailed statistics for a specific network interface.
    ///
    /// # Arguments
    ///
    /// * `if_name` - Name of the interface to look up (e.g. `"eth0"`).
    ///
    /// # Returns
    ///
    /// The [`Interface`] struct matching the requested interface name.
    ///
    /// # Errors
    ///
    /// Returns an error if the interface is not found in the vnStat data, or
    /// if the underlying data fetch fails.
    pub async fn fetch_interface_stats(&self, if_name: impl AsRef<str>) -> Result<Interface> {
        let data = self.fetch_vnstat_data().await?;

        data.interfaces
            .iter()
            .find(|i| i.name == if_name.as_ref())
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("interface not found"))
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
    /// status code, or does not complete within [`VNSTAT_TIMEOUT`].
    pub async fn check_health(&self) -> Result<()> {
        tokio::time::timeout(VNSTAT_TIMEOUT, async {
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
    ///
    /// The returned `Vec<String>` is suitable for spawning a long-running
    /// `vnstat` process that emits JSON traffic updates.
    ///
    /// # Arguments
    ///
    /// * `if_name` - The network interface to monitor in real time.
    ///
    /// # Returns
    ///
    /// A vector of command tokens: `[executable, "-i", if_name, "--json", "-l"]`.
    ///
    /// # Errors
    ///
    /// This function currently never returns `Err`; the `Result` return type
    /// is reserved for future validation.
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
    /// The stream subscribes to a [`TaskManager`] channel that wraps a
    /// long-running `vnstat -l` process. Each line of JSON output is emitted
    /// as an SSE event with a millisecond-precision timestamp as the event ID.
    ///
    /// # Arguments
    ///
    /// * `manager` - Shared [`TaskManager`] responsible for managing the
    ///   underlying vnStat subprocess.
    /// * `if_name` - The network interface to stream live data for.
    ///
    /// # Returns
    ///
    /// A pinned, boxed [`Stream`] producing `Result<Event, String>` items:
    /// * `Ok(Event)` — a live-traffic data point (or a comment on message lag).
    /// * `Err(String)` — an unrecoverable error from the vnStat subprocess.
    ///
    /// The stream ends when the subprocess terminates or the broadcast channel
    /// is closed.
    pub async fn stream_interface_live_stats(
        &self,
        manager: Arc<TaskManager>,
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
            let (mut receiver, _guard) = manager.subscribe(if_name.clone(), cmd).await;

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
/// The cache key is the executable path (constant at runtime), so repeated
/// calls within the 60-second window return the cached result without
/// re-invoking the subprocess. The TTL refreshes on every cache hit.
///
/// # Arguments
///
/// * `executable` — Path or name of the `vnstat` binary.
///
/// # Returns
///
/// A [`VnstatData`] struct deserialized from the `vnstat --json` output.
///
/// # Errors
///
/// Returns an error if:
/// * The vnStat subprocess fails to start or returns a non-zero exit code.
/// * The command times out after [`VNSTAT_TIMEOUT`].
/// * The stdout is not valid UTF-8.
/// * The JSON payload cannot be deserialized into [`VnstatData`].
#[cached(max_size = 1, ttl = 60, refresh = true)]
async fn fetch_vnstat_data_cached(executable: String) -> Result<VnstatData> {
    let output = tokio::time::timeout(VNSTAT_TIMEOUT, async {
        tokio::process::Command::new(&executable)
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
