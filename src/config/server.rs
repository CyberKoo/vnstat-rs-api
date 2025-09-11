use crate::config::traits::ConfigEntity;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_listen")]
    pub listen: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            listen: default_listen(),
            port: default_port(),
        }
    }
}

impl ConfigEntity for ServerConfig {
    fn finalize(&mut self) -> Result<()> {
        if self.listen.eq_ignore_ascii_case("localhost") {
            self.listen = "127.0.0.1".to_string();
        }
        
        Ok(())
    }

    fn validate(&self) -> Result<()> {
        if !std::net::IpAddr::from_str(&self.listen).is_ok() {
            bail!("Invalid listen address: {}", self.listen);
        }
        
        Ok(())
    }
}

impl ServerConfig {
    pub fn to_socket_addr(&self) -> Result<SocketAddr> {
        let ip: IpAddr = self
            .listen
            .parse()
            .context("Failed to parse listen address")?;

        Ok(SocketAddr::new(ip, self.port))
    }
}

fn default_listen() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    3000
}
