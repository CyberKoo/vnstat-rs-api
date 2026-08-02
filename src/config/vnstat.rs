use super::traits::ConfigEntity;
use anyhow::bail;
use serde::Deserialize;
use std::path::Path;

/// Configuration for the vnStat statistics backend.
///
/// Controls the path to the vnStat binary and the query timeout.
#[derive(Debug, Deserialize)]
pub struct VnstatConfig {
    #[serde(default = "default_executable")]
    pub executable: String,

    /// Timeout in seconds for each vnstat query subprocess.
    #[serde(default = "default_timeout")]
    pub query_timeout_secs: u64,
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
    /// (`/usr/bin/vnstat`) and default timeout (5s).
    fn default() -> Self {
        VnstatConfig {
            executable: default_executable(),
            query_timeout_secs: default_timeout(),
        }
    }
}

/// Returns the default path to the vnStat executable (`/usr/bin/vnstat`).
fn default_executable() -> String {
    "/usr/bin/vnstat".to_string()
}

/// Returns the default query timeout in seconds.
fn default_timeout() -> u64 {
    5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let c = VnstatConfig::default();
        assert_eq!(c.executable, "/usr/bin/vnstat");
        assert_eq!(c.query_timeout_secs, 5);
    }

    #[test]
    fn validate_rejects_empty_executable() {
        let c = VnstatConfig {
            executable: String::new(),
            query_timeout_secs: 5,
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn validate_rejects_nonexistent_executable() {
        let c = VnstatConfig {
            executable: "/nonexistent/vnstat".into(),
            query_timeout_secs: 5,
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn validate_accepts_existing_file() {
        let tmp = crate::test_support::write_script("#!/bin/sh\nexit 0\n");
        let c = VnstatConfig {
            executable: tmp.to_str().unwrap().into(),
            query_timeout_secs: 5,
        };
        assert!(c.validate().is_ok());
    }
}
