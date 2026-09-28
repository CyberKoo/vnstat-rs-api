use super::traits::ConfigEntity;
use anyhow::bail;
use serde::Deserialize;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
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
    /// Validates that the timeout is positive and the path names a regular file.
    /// On Unix, the file must also have an execute permission bit set.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The `executable` path is empty.
    /// - The timeout is zero.
    /// - The `executable` path is not a regular file.
    /// - On Unix, the file has no execute permission bits.
    fn validate(&self) -> anyhow::Result<()> {
        if self.executable.is_empty() {
            bail!("Vnstat executable is empty");
        }

        if self.query_timeout_secs == 0 {
            bail!("Vnstat query timeout must be greater than zero");
        }

        let path = Path::new(&self.executable);
        if !path.is_file() {
            bail!("Vnstat executable is not an existing regular file");
        }

        #[cfg(unix)]
        if path.metadata()?.permissions().mode() & 0o111 == 0 {
            bail!("Vnstat executable is not executable");
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
    fn validate_rejects_zero_timeout() {
        let c = VnstatConfig {
            executable: std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            query_timeout_secs: 0,
        };
        assert!(c.validate().unwrap_err().to_string().contains("timeout"));
    }

    #[test]
    fn validate_rejects_directory() {
        let c = VnstatConfig {
            executable: std::env::temp_dir().to_string_lossy().into_owned(),
            query_timeout_secs: 5,
        };
        assert!(c.validate().unwrap_err().to_string().contains("executable"));
    }

    #[cfg(unix)]
    #[test]
    fn validate_rejects_file_without_execute_permission() {
        let path = crate::test_support::write_temp_file("no-exec-vnstat", "#!/bin/sh\n");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let c = VnstatConfig {
            executable: path.to_string_lossy().into_owned(),
            query_timeout_secs: 5,
        };
        assert!(
            c.validate()
                .unwrap_err()
                .to_string()
                .contains("not executable")
        );
        std::fs::remove_file(path).unwrap();
    }

    #[cfg(unix)]
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
