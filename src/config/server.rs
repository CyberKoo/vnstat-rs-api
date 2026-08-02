use super::traits::ConfigEntity;
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

/// Configuration for the HTTP server listener.
///
/// Controls the IP address and port the server binds to. The `listen` field
/// supports common hostname aliases (`localhost`, `localhost6`, `ip6-localhost`)
/// which are resolved to their canonical IP addresses during finalization.
#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_listen")]
    pub listen: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

impl Default for ServerConfig {
    /// Returns a `ServerConfig` with the default listen address (`0.0.0.0`)
    /// and port (`3000`).
    fn default() -> Self {
        ServerConfig {
            listen: default_listen(),
            port: default_port(),
        }
    }
}

impl ConfigEntity for ServerConfig {
    /// Finalizes the listen address by resolving common hostname aliases.
    ///
    /// Performs the following normalizations:
    /// - `"localhost"`        → `"127.0.0.1"`
    /// - `"localhost6"`       → `"::1"`
    /// - `"ip6-localhost"`    → `"::1"`
    ///
    /// # Errors
    ///
    /// This method currently never fails.
    fn finalize(&mut self) -> Result<()> {
        if self.listen.eq_ignore_ascii_case("localhost") {
            self.listen = "127.0.0.1".to_string();
        } else if self.listen.eq_ignore_ascii_case("localhost6")
            || self.listen.eq_ignore_ascii_case("ip6-localhost")
        {
            self.listen = "::1".to_string();
        }

        Ok(())
    }

    /// Validates that the listen address is a valid IP address.
    ///
    /// # Errors
    ///
    /// Returns an error if `listen` cannot be parsed as an `IpAddr`.
    fn validate(&self) -> Result<()> {
        if std::net::IpAddr::from_str(&self.listen).is_err() {
            bail!("Invalid listen address: {}", self.listen);
        }

        Ok(())
    }
}

impl ServerConfig {
    /// Converts the configuration into a `SocketAddr`.
    ///
    /// Parses the `listen` field as an `IpAddr` and pairs it with the
    /// configured port.
    ///
    /// # Returns
    ///
    /// A `SocketAddr` suitable for binding a TCP listener.
    ///
    /// # Errors
    ///
    /// Returns an error if the `listen` field cannot be parsed as a valid
    /// IP address.
    pub fn to_socket_addr(&self) -> Result<SocketAddr> {
        let ip: IpAddr = self
            .listen
            .parse()
            .context("Failed to parse listen address")?;

        Ok(SocketAddr::new(ip, self.port))
    }
}

/// Returns the default listen address (`0.0.0.0`).
fn default_listen() -> String {
    "0.0.0.0".to_string()
}

/// Returns the default HTTP port (`3000`).
fn default_port() -> u16 {
    3000
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(listen: &str, port: u16) -> ServerConfig {
        ServerConfig {
            listen: listen.to_string(),
            port,
        }
    }

    #[test]
    fn defaults() {
        let c = ServerConfig::default();
        assert_eq!(c.listen, "0.0.0.0");
        assert_eq!(c.port, 3000);
    }

    #[test]
    fn finalize_resolves_localhost_aliases() {
        for (input, expected) in [
            ("localhost", "127.0.0.1"),
            ("LOCALHOST", "127.0.0.1"),
            ("localhost6", "::1"),
            ("ip6-localhost", "::1"),
        ] {
            let mut c = config(input, 3000);
            c.finalize().unwrap();
            assert_eq!(c.listen, expected, "input: {}", input);
        }
    }

    #[test]
    fn finalize_keeps_ip_addresses_untouched() {
        let mut c = config("192.168.1.5", 3000);
        c.finalize().unwrap();
        assert_eq!(c.listen, "192.168.1.5");
    }

    #[test]
    fn validate_accepts_valid_ip() {
        assert!(config("127.0.0.1", 3000).validate().is_ok());
    }

    #[test]
    fn validate_rejects_invalid_ip() {
        assert!(config("not-an-ip", 3000).validate().is_err());
    }

    #[test]
    fn to_socket_addr_combines_ip_and_port() {
        let c = config("127.0.0.1", 8080);
        assert_eq!(
            c.to_socket_addr().unwrap(),
            SocketAddr::from(([127, 0, 0, 1], 8080))
        );
    }

    #[test]
    fn to_socket_addr_rejects_invalid_ip() {
        assert!(config("nope", 8080).to_socket_addr().is_err());
    }
}
