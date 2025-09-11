use crate::config::server::ServerConfig;
use crate::config::traits::ConfigEntity;
use crate::config::vnstat::VnstatConfig;
use anyhow::{bail, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;

mod server;
mod traits;
mod vnstat;

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,

    #[serde(default)]
    pub vnstat: VnstatConfig,
}

impl ConfigEntity for AppConfig {
    fn finalize(&mut self) -> Result<()> {
        self.server.finalize()?;
        self.vnstat.finalize()?;

        Ok(())
    }

    fn validate(&self) -> Result<()> {
        self.server.validate()?;
        self.vnstat.validate()?;

        Ok(())
    }
}
pub fn load_config(file_path: &str) -> Result<AppConfig> {
    // Check if the file exists.
    if !Path::new(file_path).exists() {
        bail!("File `{}` does not exist.", file_path);
    }

    // Read the file content into a string.
    let config_content = fs::read_to_string(file_path)?;

    // Parse the TOML file into an AppConfig object.
    let mut config: AppConfig = toml::from_str(&config_content)?;

    // Finalize the configuration to ensure all values are initialized.
    config.finalize()?;

    // Validate the configuration to ensure all values are correct.
    config.validate()?;

    // Return the fully validated configuration.
    Ok(config)
}
