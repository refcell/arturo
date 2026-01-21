//! Participant wrapper around the Conductor for the demo.

use std::sync::Arc;

use arturo::{Conductor, ConductorConfig};
use commonware_cryptography::{Signer as _, ed25519};

use crate::{epoch::RoundRobinEpochManager, payload::DemoPayload};

/// Type alias for the demo conductor.
pub type DemoConductor = Conductor<DemoPayload, RoundRobinEpochManager, ed25519::PrivateKey>;

/// View of a participant's state for the TUI.
#[derive(Debug, Clone)]
pub struct ParticipantView {
    /// Participant ID (1-indexed for display).
    pub id: usize,
    /// Current epoch.
    pub epoch: u64,
    /// Whether this participant is the current leader.
    pub is_leader: bool,
    /// Next expected payload height.
    pub next_height: u64,
    /// Number of certified payloads.
    pub certified_count: u64,
}

/// A demo participant wrapping a conductor.
pub struct Participant {
    /// Participant ID (1-indexed).
    id: usize,
    /// The underlying conductor.
    conductor: DemoConductor,
    /// The epoch manager (shared reference for epoch advancement).
    epoch_manager: RoundRobinEpochManager,
}

impl Participant {
    /// Creates a new participant from a seed.
    ///
    /// # Arguments
    ///
    /// * `seed` - The seed for key derivation
    /// * `id` - The 1-indexed participant ID
    /// * `all_keys` - List of all participant public keys
    pub fn new(seed: u64, id: usize, all_keys: Vec<ed25519::PublicKey>) -> Self {
        let signer = ed25519::PrivateKey::from_seed(seed);
        let public_key = signer.public_key();

        let epoch_manager = RoundRobinEpochManager::new(all_keys, public_key);

        let config =
            ConductorConfig { quorum_threshold: epoch_manager.participant_count() / 2 + 1 };

        let conductor = Conductor::new(config, epoch_manager.clone(), signer);

        Self { id, conductor, epoch_manager }
    }

    /// Returns the participant ID.
    pub const fn id(&self) -> usize {
        self.id
    }

    /// Returns a reference to the conductor.
    pub const fn conductor(&self) -> &DemoConductor {
        &self.conductor
    }

    /// Returns a reference to the epoch manager.
    pub const fn epoch_manager(&self) -> &RoundRobinEpochManager {
        &self.epoch_manager
    }

    /// Starts the conductor.
    pub async fn start(&self) {
        self.conductor.start().await;
    }

    /// Stops the conductor.
    pub async fn stop(&self) {
        self.conductor.stop().await;
    }

    /// Gets a view of this participant's current state.
    pub async fn get_view(&self) -> ParticipantView {
        let epoch = self.conductor.current_epoch().await;
        let is_leader = self.conductor.leader().await;
        let next_height = self.conductor.next_height().await;
        let certified_count = if next_height > 0 { next_height } else { 0 };

        ParticipantView { id: self.id, epoch, is_leader, next_height, certified_count }
    }

    /// Commits a payload (if this participant is the leader).
    pub async fn commit(&self, payload: DemoPayload) -> Result<(), arturo::ConductorError> {
        self.conductor.commit(payload).await
    }

    /// Records an acknowledgment and returns the certified payload if quorum reached.
    pub async fn acknowledge(&self) -> Option<DemoPayload> {
        self.conductor.acknowledge().await
    }

    /// Handles an epoch change.
    pub async fn handle_epoch_change(&self, change: arturo::EpochChange<ed25519::PublicKey>) {
        self.conductor.handle_epoch_change(change).await;
    }
}

/// Shared participant reference for concurrent access.
pub type SharedParticipant = Arc<Participant>;
