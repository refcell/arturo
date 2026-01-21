//! Core types for the arturo conductor.
//!
//! This module contains error types, epoch-related structures, and other
//! shared types used throughout the crate.

use std::fmt;

use thiserror::Error;

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
#[derive(Debug, Clone, PartialEq, Eq, Error)]
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
#[derive(Debug, Clone, PartialEq, Eq, Error)]
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
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::below_threshold(2, 3, false)]
    #[case::at_threshold(3, 3, true)]
    #[case::above_threshold(4, 3, true)]
    fn pending_payload_certification(
        #[case] acks: usize,
        #[case] threshold: usize,
        #[case] expected_certified: bool,
    ) {
        let mut pending = PendingPayload::new("test", threshold);
        for _ in 0..acks {
            pending.acknowledge();
        }
        assert_eq!(pending.is_certified(), expected_certified);
    }

    #[test]
    fn epoch_change_display() {
        let change = EpochChange { epoch: 42, sequencer: "node1", is_self: true };
        let display = format!("{change}");
        assert!(display.contains("42"));
        assert!(display.contains("node1"));
        assert!(display.contains("true"));
    }

    #[rstest]
    #[case::not_sequencer(ConductorError::NotSequencer, "not the current sequencer")]
    #[case::not_initialized(ConductorError::NotInitialized, "conductor not initialized")]
    #[case::channel_closed(ConductorError::ChannelClosed, "internal channel closed")]
    #[case::validation_failed(ConductorError::ValidationFailed("bad".to_string()), "payload validation failed: bad")]
    fn conductor_error_display(#[case] error: ConductorError, #[case] expected: &str) {
        assert_eq!(format!("{error}"), expected);
    }

    #[rstest]
    #[case::not_supported(TransferError::NotSupported, "transfer not supported")]
    #[case::no_successor(TransferError::NoSuccessor, "no successor available")]
    #[case::in_progress(TransferError::InProgress, "transfer in progress")]
    #[case::timeout(TransferError::Timeout, "transfer timeout")]
    #[case::failed(TransferError::Failed("reason".to_string()), "transfer failed: reason")]
    fn transfer_error_display(#[case] error: TransferError, #[case] expected: &str) {
        assert_eq!(format!("{error}"), expected);
    }
}
