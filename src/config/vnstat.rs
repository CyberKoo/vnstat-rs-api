use crate::config::traits::ConfigEntity;
use anyhow::bail;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct VnstatConfig {
    #[serde(default = "default_executable")]
    pub executable: String,
}

impl ConfigEntity for VnstatConfig {
    fn finalize(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

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
    fn default() -> Self {
        VnstatConfig {
            executable: default_executable(),
        }
    }
}

fn default_executable() -> String {
    "/usr/bin/vnstat".to_string()
}
