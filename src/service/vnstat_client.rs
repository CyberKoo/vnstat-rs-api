use crate::model::vnstat::VnstatData;
use anyhow::{Context, Result};
use cached::cached;
use std::time::Duration;

pub(crate) struct VnstatClient {
    executable: String,
    timeout: Duration,
}

impl VnstatClient {
    pub(crate) fn new(executable: impl Into<String>, timeout_secs: u64) -> Self {
        Self {
            executable: executable.into(),
            timeout: Duration::from_secs(timeout_secs),
        }
    }

    pub(crate) async fn fetch_data(&self) -> Result<VnstatData> {
        fetch_vnstat_data_cached(self.executable.clone(), self.timeout).await
    }

    pub(crate) fn build_live_stream_command(
        &self,
        if_name: impl AsRef<str>,
    ) -> Result<Vec<String>> {
        Ok(vec![
            self.executable.clone(),
            "-i".to_string(),
            if_name.as_ref().to_string(),
            "--json".to_string(),
            "-l".to_string(),
        ])
    }
}

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

    if !output.status.success() {
        anyhow::bail!(
            "vnStat exited with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let json_str =
        String::from_utf8(output.stdout).context("failed to parse vnStat response as UTF-8")?;
    serde_json::from_str(&json_str).context("failed to deserialize vnStat JSON response")
}
