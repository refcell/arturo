//! Status channel for communicating sidecar activity to the TUI.

use std::sync::Arc;

use tokio::sync::watch;

/// Status message from the sidecar.
#[derive(Debug, Clone, Default)]
pub struct SidecarStatus {
    /// Current action description.
    pub action: String,
    /// Current epoch.
    pub epoch: u64,
    /// Total number of certified blocks (shared across all participants).
    pub certified_blocks: u64,
}

/// Sender for sidecar status updates.
pub type StatusSender = watch::Sender<SidecarStatus>;

/// Receiver for sidecar status updates.
pub type StatusReceiver = watch::Receiver<SidecarStatus>;

/// Creates a new status channel.
pub fn channel() -> (StatusSender, StatusReceiver) {
    watch::channel(SidecarStatus::default())
}

/// Shared status receiver for the TUI.
pub type SharedStatusReceiver = Arc<tokio::sync::Mutex<StatusReceiver>>;
