use super::traits::ConfigEntity;
use anyhow::bail;
use serde::Deserialize;
use std::collections::HashMap;

/// Configuration for the reported link speed of network interfaces.
///
/// Speeds are configured **per interface** via `[link_speed.<if_name>]`
/// tables in the configuration file. Interfaces without an entry fall back
/// to the default of 1000 Mbps on both directions:
///
/// ```toml
/// [link_speed.eth0]
/// rx = 1000  # RX link speed in Mbps (default: 1000)
/// tx = 1000  # TX link speed in Mbps (default: 1000)
/// ```
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(transparent)]
pub struct LinkSpeedConfig {
    /// Per-interface link speeds, keyed by interface name.
    pub interfaces: HashMap<String, LinkSpeed>,
}

impl LinkSpeedConfig {
    /// Returns the configured link speed for `if_name`, or the default
    /// (1000 Mbps both directions) when the interface has no entry.
    pub fn get(&self, if_name: &str) -> LinkSpeed {
        self.interfaces.get(if_name).copied().unwrap_or_default()
    }
}

impl ConfigEntity for LinkSpeedConfig {
    /// Validates that every configured link speed is positive.
    ///
    /// # Errors
    ///
    /// Returns an error if any interface's `rx` or `tx` is zero.
    fn validate(&self) -> anyhow::Result<()> {
        for (if_name, speed) in &self.interfaces {
            if speed.rx == 0 {
                bail!("link_speed.{}.rx must be positive", if_name);
            }
            if speed.tx == 0 {
                bail!("link_speed.{}.tx must be positive", if_name);
            }
        }
        Ok(())
    }
}

/// Link speed of a single interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct LinkSpeed {
    /// RX link speed in Mbps.
    #[serde(default = "default_link_speed")]
    pub rx: u64,

    /// TX link speed in Mbps.
    #[serde(default = "default_link_speed")]
    pub tx: u64,
}

impl Default for LinkSpeed {
    /// Returns a `LinkSpeed` with both directions at the default of
    /// 1000 Mbps.
    fn default() -> Self {
        LinkSpeed {
            rx: default_link_speed(),
            tx: default_link_speed(),
        }
    }
}

/// Returns the default link speed in Mbps (1000).
fn default_link_speed() -> u64 {
    1000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let c = LinkSpeedConfig::default();
        assert!(c.interfaces.is_empty());
        assert_eq!(c.get("eth0"), LinkSpeed { rx: 1000, tx: 1000 });
    }

    #[test]
    fn validate_rejects_zero_rx() {
        let c = LinkSpeedConfig {
            interfaces: HashMap::from([("eth0".to_string(), LinkSpeed { rx: 0, tx: 1000 })]),
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn validate_rejects_zero_tx() {
        let c = LinkSpeedConfig {
            interfaces: HashMap::from([("eth0".to_string(), LinkSpeed { rx: 1000, tx: 0 })]),
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn validate_accepts_positive_speeds() {
        let c = LinkSpeedConfig {
            interfaces: HashMap::from([("eth0".to_string(), LinkSpeed { rx: 500, tx: 2000 })]),
        };
        assert!(c.validate().is_ok());
    }

    #[test]
    fn deserializes_per_interface_values() {
        let c: LinkSpeedConfig = toml::from_str("[eth0]\nrx = 2500\ntx = 2500\n").unwrap();
        assert_eq!(c.get("eth0"), LinkSpeed { rx: 2500, tx: 2500 });
        assert_eq!(c.get("wlan0"), LinkSpeed { rx: 1000, tx: 1000 });
    }

    #[test]
    fn missing_field_defaults_to_1000() {
        let c: LinkSpeedConfig = toml::from_str("[eth0]\nrx = 500\n").unwrap();
        assert_eq!(c.get("eth0").rx, 500);
        assert_eq!(c.get("eth0").tx, 1000);
    }

    #[test]
    fn empty_table_gives_no_entries() {
        let c: LinkSpeedConfig = toml::from_str("").unwrap();
        assert!(c.interfaces.is_empty());
        assert_eq!(c.get("eth0"), LinkSpeed { rx: 1000, tx: 1000 });
    }
}
