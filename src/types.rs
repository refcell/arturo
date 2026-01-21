//! Core types for the arturo conductor.
//!
//! This module contains error types, epoch-related structures, and other
//! shared types used throughout the crate.

use std::fmt;

/// Epoch identifier type.
///
/// Epochs are sequential periods during which a single sequencer has
/// exclusive authority to propose payloads.
pub type Epoch = u64;

/// Height/sequence number type.
///
/// Heights are monotonically increasing identifiers for payloads within
/// a chain.
pub type Height = u64;

/// Epoch change notification.
///
/// Emitted when the epoch transitions, notifying subscribers of the
/// new sequencer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpochChange<K> {
    /// The new epoch number.
    pub epoch: Epoch,
    /// The public key of the sequencer for this epoch.
    pub sequencer: K,
    /// Whether the local node is the new sequencer.
    pub is_self: bool,
}

impl<K: fmt::Display> fmt::Display for EpochChange<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "EpochChange {{ epoch: {}, sequencer: {}, is_self: {} }}",
            self.epoch, self.sequencer, self.is_self
        )
    }
}

/// Errors that can occur during conductor operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConductorError {
    /// Attempted to commit a payload when not the current sequencer.
    #[error("not the current sequencer")]
    NotSequencer,

    /// The payload failed validation.
    #[error("payload validation failed: {0}")]
    ValidationFailed(String),

    /// The payload height does not follow the expected sequence.
    #[error("invalid height: expected {expected}, got {got}")]
    InvalidHeight {
        /// The expected height.
        expected: Height,
        /// The actual height received.
        got: Height,
    },

    /// The payload's parent digest does not match.
    #[error("parent mismatch: expected {expected}, got {got}")]
    ParentMismatch {
        /// The expected parent digest.
        expected: String,
        /// The actual parent digest received.
        got: String,
    },

    /// The conductor is not yet initialized.
    #[error("conductor not initialized")]
    NotInitialized,

    /// Internal channel was closed unexpectedly.
    #[error("internal channel closed")]
    ChannelClosed,
}

/// Errors that can occur during leader transfer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransferError {
    /// Transfer is not supported by the epoch manager.
    #[error("transfer not supported")]
    NotSupported,

    /// No suitable successor is available.
    #[error("no successor available")]
    NoSuccessor,

    /// Transfer is already in progress.
    #[error("transfer in progress")]
    InProgress,

    /// Transfer failed for an implementation-specific reason.
    #[error("transfer failed: {0}")]
    Failed(String),

    /// Timeout waiting for transfer to complete.
    #[error("transfer timeout")]
    Timeout,
}

/// State of a pending payload awaiting certification.
#[derive(Debug, Clone)]
pub struct PendingPayload<P> {
    /// The payload awaiting certification.
    pub payload: P,
    /// Number of acknowledgments received.
    pub acks: usize,
    /// Required acknowledgments for certification.
    pub threshold: usize,
}

impl<P> PendingPayload<P> {
    /// Creates a new pending payload.
    pub const fn new(payload: P, threshold: usize) -> Self {
        Self { payload, acks: 0, threshold }
    }

    /// Returns true if the payload has reached quorum.
    pub const fn is_certified(&self) -> bool {
        self.acks >= self.threshold
    }

    /// Records an acknowledgment.
    pub const fn acknowledge(&mut self) {
        self.acks += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pending_payload_certification() {
        let mut pending = PendingPayload::new("test", 3);
        assert!(!pending.is_certified());

        pending.acknowledge();
        pending.acknowledge();
        assert!(!pending.is_certified());

        pending.acknowledge();
        assert!(pending.is_certified());
    }

    #[test]
    fn test_epoch_change_display() {
        let change = EpochChange { epoch: 42, sequencer: "node1", is_self: true };
        let display = format!("{}", change);
        assert!(display.contains("42"));
        assert!(display.contains("node1"));
        assert!(display.contains("true"));
    }

    #[test]
    fn test_conductor_error_display() {
        let err = ConductorError::NotSequencer;
        assert_eq!(format!("{}", err), "not the current sequencer");

        let err = ConductorError::InvalidHeight { expected: 5, got: 3 };
        assert!(format!("{}", err).contains("expected 5"));
    }
}
