use self::traits::ConfigEntity;
use crate::config::cors::CorsConfig;
use crate::config::link_speed::LinkSpeedConfig;
use crate::config::server::ServerConfig;
use crate::config::sse::SseConfig;
use crate::config::vnstat::VnstatConfig;
use anyhow::{Result, bail};
use serde::Deserialize;
use std::fs;
use std::path::Path;

pub mod cors;
pub mod link_speed;
pub mod server;
pub mod sse;
pub mod traits;
pub mod vnstat;

/// Top-level application configuration deserialized from a TOML file.
///
/// Contains five subsections:
/// - `server`: required, controls the HTTP listener settings.
/// - `vnstat`: optional with defaults, configures the vnStat binary path.
/// - `cors`:  optional with defaults, configures CORS behaviour.
/// - `sse`:   optional with defaults, configures SSE live-stream behaviour.
/// - `link_speed`: optional with defaults, configures the reported link speed.
#[derive(Debug, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,

    #[serde(default)]
    pub vnstat: VnstatConfig,

    #[serde(default)]
    pub cors: CorsConfig,

    #[serde(default)]
    pub sse: SseConfig,

    #[serde(default)]
    pub link_speed: LinkSpeedConfig,
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
        self.sse.validate()?;
        self.link_speed.validate()?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_config_rejects_missing_file() {
        let err = load_config("/nonexistent/config.toml");
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn load_config_rejects_invalid_toml() {
        let p = crate::test_support::write_temp_file("cfg", "not = [valid");
        assert!(load_config(p.to_str().unwrap()).is_err());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn load_config_requires_server_section() {
        let p = crate::test_support::write_temp_file("cfg", "[vnstat]\nexecutable = \"/bin/sh\"\n");
        assert!(load_config(p.to_str().unwrap()).is_err());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn load_config_finalizes_and_validates() {
        let script = crate::test_support::write_script("#!/bin/sh\nexit 0\n");
        let p = crate::test_support::write_temp_file(
            "cfg",
            &format!(
                "[server]\nlisten = \"localhost\"\nport = 8080\n\n[vnstat]\nexecutable = \"{}\"\nquery_timeout_secs = 3\n\n[sse]\nsubscriber_buffer = 8\n\n[link_speed.eth0]\nrx = 500\ntx = 2500\n",
                script.display()
            ),
        );
        let cfg = load_config(p.to_str().unwrap()).unwrap();
        assert_eq!(cfg.server.listen, "127.0.0.1");
        assert_eq!(cfg.server.port, 8080);
        assert_eq!(cfg.vnstat.query_timeout_secs, 3);
        assert_eq!(cfg.sse.subscriber_buffer, 8);
        assert_eq!(cfg.link_speed.get("eth0").rx, 500);
        assert_eq!(cfg.link_speed.get("eth0").tx, 2500);
        assert_eq!(cfg.link_speed.get("wlan0").rx, 1000);
        assert!(!cfg.cors.enabled);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn load_config_applies_defaults_for_missing_sections() {
        let p = crate::test_support::write_temp_file("cfg", "[server]\n");
        let cfg = load_config(p.to_str().unwrap()).unwrap();
        assert_eq!(cfg.server.port, 3000);
        assert_eq!(cfg.vnstat.executable, "/usr/bin/vnstat");
        assert_eq!(cfg.sse.subscriber_buffer, 4);
        assert_eq!(cfg.link_speed.get("eth0").rx, 1000);
        assert_eq!(cfg.link_speed.get("eth0").tx, 1000);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn load_config_rejects_invalid_listen_address() {
        let p = crate::test_support::write_temp_file("cfg", "[server]\nlisten = \"bogus\"\n");
        assert!(load_config(p.to_str().unwrap()).is_err());
        let _ = fs::remove_file(&p);
    }
}
