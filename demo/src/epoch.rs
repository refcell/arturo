//! Round-robin epoch manager for the demo.
//!
//! Rotates the sequencer role among participants based on epoch number.

use std::sync::Arc;

use arturo::{Epoch, EpochChange, EpochManager, EpochStream, TransferError};
use commonware_cryptography::ed25519;
use futures::stream;
use tokio::sync::{RwLock, broadcast};

/// State for the round-robin epoch manager.
struct EpochState {
    /// Current epoch number.
    epoch: Epoch,
    /// Index of current sequencer in the participants list.
    sequencer_idx: usize,
}

/// Round-robin epoch manager that rotates sequencer based on epoch.
///
/// The sequencer for epoch N is `participants[N % len]`.
#[derive(Clone)]
pub struct RoundRobinEpochManager {
    /// List of all participant public keys in order.
    participants: Arc<Vec<ed25519::PublicKey>>,
    /// This node's public key.
    self_key: ed25519::PublicKey,
    /// This node's index in the participants list.
    self_idx: usize,
    /// Internal state.
    state: Arc<RwLock<EpochState>>,
    /// Broadcast channel for epoch changes.
    epoch_tx: broadcast::Sender<EpochChange<ed25519::PublicKey>>,
}

impl std::fmt::Debug for RoundRobinEpochManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoundRobinEpochManager")
            .field("participants", &self.participants.len())
            .field("self_idx", &self.self_idx)
            .finish_non_exhaustive()
    }
}

impl RoundRobinEpochManager {
    /// Creates a new round-robin epoch manager.
    ///
    /// # Arguments
    ///
    /// * `participants` - Ordered list of all participant public keys
    /// * `self_key` - This node's public key
    pub fn new(participants: Vec<ed25519::PublicKey>, self_key: ed25519::PublicKey) -> Self {
        let self_idx = participants.iter().position(|k| k == &self_key).unwrap_or(0);
        let (epoch_tx, _) = broadcast::channel(16);

        Self {
            participants: Arc::new(participants),
            self_key,
            self_idx,
            state: Arc::new(RwLock::new(EpochState { epoch: 0, sequencer_idx: 0 })),
            epoch_tx,
        }
    }

    /// Advances to the next epoch.
    ///
    /// Rotates the sequencer to the next participant.
    pub async fn advance_epoch(&self) -> EpochChange<ed25519::PublicKey> {
        let mut state = self.state.write().await;
        state.epoch += 1;
        state.sequencer_idx = (state.epoch as usize) % self.participants.len();

        let sequencer = self.participants[state.sequencer_idx].clone();
        let is_self = state.sequencer_idx == self.self_idx;

        let change = EpochChange { epoch: state.epoch, sequencer, is_self };

        let _ = self.epoch_tx.send(change.clone());

        change
    }

    /// Returns the current sequencer index.
    pub async fn current_sequencer_idx(&self) -> usize {
        self.state.read().await.sequencer_idx
    }

    /// Returns the number of participants.
    pub fn participant_count(&self) -> usize {
        self.participants.len()
    }

    /// Returns this node's index.
    pub const fn self_index(&self) -> usize {
        self.self_idx
    }
}

impl EpochManager for RoundRobinEpochManager {
    type PublicKey = ed25519::PublicKey;

    fn current_epoch(&self) -> Epoch {
        self.state.try_read().map(|s| s.epoch).unwrap_or(0)
    }

    fn sequencer(&self, epoch: Epoch) -> Option<Self::PublicKey> {
        let idx = (epoch as usize) % self.participants.len();
        self.participants.get(idx).cloned()
    }

    fn is_sequencer(&self, key: &Self::PublicKey) -> bool {
        let idx = self.state.try_read().map(|s| s.sequencer_idx).unwrap_or(0);
        self.participants.get(idx).is_some_and(|k| k == key)
    }

    async fn transfer_leader(&self) -> Result<(), TransferError> {
        // Round-robin doesn't support manual transfer; epochs advance automatically.
        Err(TransferError::NotSupported)
    }

    fn subscribe(&self) -> EpochStream<Self::PublicKey> {
        let mut rx = self.epoch_tx.subscribe();
        Box::pin(stream::poll_fn(move |cx| {
            use std::task::Poll;
            match rx.try_recv() {
                Ok(change) => Poll::Ready(Some(change)),
                Err(broadcast::error::TryRecvError::Empty) => {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                Err(_) => Poll::Ready(None),
            }
        }))
    }

    fn validators(&self, _epoch: Epoch) -> Option<Vec<Self::PublicKey>> {
        Some(self.participants.as_ref().clone())
    }

    fn quorum_threshold(&self, _epoch: Epoch) -> Option<usize> {
        // Simple majority: floor(n/2) + 1
        Some(self.participants.len() / 2 + 1)
    }
}
