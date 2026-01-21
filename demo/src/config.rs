//! CLI configuration for the demo binary.

use std::time::Duration;

use clap::Parser;

/// CLI arguments for the demo.
#[derive(Debug, Parser)]
#[command(name = "demo")]
#[command(about = "Arturo demo with TUI visualization")]
pub struct DemoConfig {
    /// Number of participants in the demo.
    #[arg(short, long, default_value = "8")]
    pub participants: usize,

    /// Interval between payload commits in milliseconds.
    #[arg(short, long, default_value = "245")]
    pub interval_ms: u64,

    /// Number of commits before advancing epoch.
    #[arg(short, long, default_value = "3")]
    pub commits_per_epoch: u64,
}

impl DemoConfig {
    /// Returns the commit interval as a Duration.
    pub const fn commit_interval(&self) -> Duration {
        Duration::from_millis(self.interval_ms)
    }
}

impl Default for DemoConfig {
    fn default() -> Self {
        Self { participants: 8, interval_ms: 245, commits_per_epoch: 3 }
    }
}
