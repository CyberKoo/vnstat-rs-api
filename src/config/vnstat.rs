use super::traits::ConfigEntity;
use anyhow::bail;
use serde::Deserialize;
use std::path::Path;

/// Configuration for the vnStat statistics backend.
///
/// Controls the path to the vnStat binary that the application invokes to
/// query network traffic statistics.
#[derive(Debug, Deserialize)]
pub struct VnstatConfig {
    #[serde(default = "default_executable")]
    pub executable: String,
}

impl ConfigEntity for VnstatConfig {
    /// Validates that the vnStat executable path is non-empty and points to an
    /// existing file on disk.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The `executable` path is empty.
    /// - The `executable` path does not exist on the filesystem.
    fn validate(&self) -> anyhow::Result<()> {
        if self.executable.is_empty() {
            bail!("Vnstat executable is empty");
        }

        if !Path::new(&self.executable).exists() {
            bail!("Vnstat executable does not exist");
        }

        Ok(())
    }
}

impl Default for VnstatConfig {
    /// Returns a `VnstatConfig` with the default executable path
    /// (`/usr/bin/vnstat`).
    fn default() -> Self {
        VnstatConfig {
            executable: default_executable(),
        }
    }
}

/// Returns the default path to the vnStat executable (`/usr/bin/vnstat`).
fn default_executable() -> String {
    "/usr/bin/vnstat".to_string()
}
