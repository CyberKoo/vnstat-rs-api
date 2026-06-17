use self::traits::ConfigEntity;
use crate::config::cors::CorsConfig;
use crate::config::server::ServerConfig;
use crate::config::vnstat::VnstatConfig;
use anyhow::{Result, bail};
use serde::Deserialize;
use std::fs;
use std::path::Path;

pub mod cors;
pub mod server;
pub mod traits;
pub mod vnstat;

/// Top-level application configuration deserialized from a TOML file.
///
/// Contains three subsections:
/// - `server`: required, controls the HTTP listener settings.
/// - `vnstat`: optional with defaults, configures the vnStat binary path.
/// - `cors`:  optional with defaults, configures CORS behaviour.
#[derive(Debug, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,

    #[serde(default)]
    pub vnstat: VnstatConfig,

    #[serde(default)]
    pub cors: CorsConfig,
}

impl ConfigEntity for AppConfig {
    /// Finalizes all sub-configurations after deserialization.
    ///
    /// Delegates to each child's `finalize()` in turn so that
    /// string aliases (e.g. `"localhost"` → `"127.0.0.1"`) are
    /// resolved before validation.
    ///
    /// # Errors
    ///
    /// Propagates any error from a child `finalize()` call.
    fn finalize(&mut self) -> Result<()> {
        self.server.finalize()?;
        self.vnstat.finalize()?;

        Ok(())
    }

    /// Validates all sub-configurations after finalization.
    ///
    /// Delegates to each child's `validate()` in turn.
    ///
    /// # Errors
    ///
    /// Returns the first validation error encountered from any child
    /// configuration.
    fn validate(&self) -> Result<()> {
        self.server.validate()?;
        self.vnstat.validate()?;

        Ok(())
    }
}

/// Loads, finalizes, and validates the application configuration from a TOML
/// file.
///
/// # Arguments
///
/// * `file_path` — Path to the TOML configuration file.
///
/// # Returns
///
/// A fully finalized and validated `AppConfig` instance.
///
/// # Errors
///
/// Returns an error if:
/// - The file does not exist.
/// - The file cannot be read.
/// - The file content is not valid TOML.
/// - Any `finalize()` or `validate()` step fails.
pub fn load_config(file_path: &str) -> Result<AppConfig> {
    if !Path::new(file_path).exists() {
        bail!("File `{}` does not exist.", file_path);
    }

    let config_content = fs::read_to_string(file_path)?;

    let mut config: AppConfig = toml::from_str(&config_content)?;

    config.finalize()?;

    config.validate()?;

    Ok(config)
}
