//! Configuration for the op-conductor binary.
//!
//! Supports loading configuration from TOML files, environment variables,
//! or CLI arguments.

use std::{net::SocketAddr, path::Path};

use clap::Parser;
use serde::{Deserialize, Serialize};

/// CLI arguments for op-conductor.
#[derive(Debug, Parser)]
#[command(name = "op-conductor")]
#[command(about = "Minimal OP Stack conductor using arturo")]
pub struct Cli {
    /// Path to the configuration file.
    #[arg(short, long, env = "OP_CONDUCTOR_CONFIG")]
    pub config: Option<String>,

    /// Address to bind the HTTP server to.
    #[arg(long, env = "OP_CONDUCTOR_BIND_ADDR", default_value = "127.0.0.1:8080")]
    pub bind_addr: SocketAddr,

    /// This node's identity seed (used for key derivation).
    #[arg(long, env = "OP_CONDUCTOR_IDENTITY")]
    pub identity: Option<u64>,

    /// Comma-separated list of peer URLs.
    #[arg(long, env = "OP_CONDUCTOR_PEERS", value_delimiter = ',')]
    pub peers: Vec<String>,

    /// Health check interval in milliseconds.
    #[arg(long, env = "OP_CONDUCTOR_HEALTH_INTERVAL_MS", default_value = "1000")]
    pub health_interval_ms: u64,

    /// Quorum threshold for certification.
    #[arg(long, env = "OP_CONDUCTOR_QUORUM_THRESHOLD", default_value = "1")]
    pub quorum_threshold: usize,
}

/// Configuration for the op-conductor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Address to bind the HTTP server to.
    pub bind_addr: SocketAddr,

    /// List of peer URLs for health checking and communication.
    pub peers: Vec<String>,

    /// This node's identity seed for key derivation.
    pub identity: u64,

    /// Health check interval in milliseconds.
    pub health_interval_ms: u64,

    /// Quorum threshold for certification.
    pub quorum_threshold: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:8080".parse().unwrap(),
            peers: Vec::new(),
            identity: 0,
            health_interval_ms: 1000,
            quorum_threshold: 1,
        }
    }
}

impl Config {
    /// Load configuration from a TOML file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let contents = std::fs::read_to_string(path)?;
        toml::from_str(&contents).map_err(ConfigError::Parse)
    }

    /// Load configuration from CLI arguments, optionally overriding with a config file.
    pub fn load() -> Result<Self, ConfigError> {
        let cli = Cli::parse();

        // Start with config file if provided
        let mut config =
            if let Some(ref path) = cli.config { Self::from_file(path)? } else { Self::default() };

        // CLI args override config file values
        config.bind_addr = cli.bind_addr;

        if let Some(identity) = cli.identity {
            config.identity = identity;
        }

        if !cli.peers.is_empty() {
            config.peers = cli.peers;
        }

        config.health_interval_ms = cli.health_interval_ms;
        config.quorum_threshold = cli.quorum_threshold;

        Ok(config)
    }
}

/// Configuration loading errors.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Failed to read configuration file.
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),

    /// Failed to parse configuration file.
    #[error("failed to parse config: {0}")]
    Parse(toml::de::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.bind_addr.to_string(), "127.0.0.1:8080");
        assert!(config.peers.is_empty());
        assert_eq!(config.health_interval_ms, 1000);
        assert_eq!(config.quorum_threshold, 1);
    }

    #[test]
    fn test_config_serde() {
        let config = Config {
            bind_addr: "0.0.0.0:9000".parse().unwrap(),
            peers: vec!["http://peer1:8080".to_string(), "http://peer2:8080".to_string()],
            identity: 42,
            health_interval_ms: 500,
            quorum_threshold: 2,
        };

        let toml_str = toml::to_string(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();

        assert_eq!(parsed.bind_addr, config.bind_addr);
        assert_eq!(parsed.peers, config.peers);
        assert_eq!(parsed.identity, config.identity);
    }
}
