//! Sidecar service that generates payloads and drives consensus.
//!
//! Runs on a configurable interval, commits payloads from the leader,
//! and triggers acknowledgments on all participants.

use std::{sync::Arc, time::Duration};

use tokio::time::interval;

use crate::{
    participant::SharedParticipant,
    payload::DemoPayload,
    status::{SidecarStatus, StatusSender},
};

/// Configuration for the sidecar.
#[derive(Debug, Clone)]
pub struct SidecarConfig {
    /// Interval between payload commits.
    pub commit_interval: Duration,
    /// Number of commits before advancing epoch.
    pub commits_per_epoch: u64,
}

impl Default for SidecarConfig {
    fn default() -> Self {
        Self { commit_interval: Duration::from_millis(245), commits_per_epoch: 3 }
    }
}

/// Sidecar that drives the demo consensus.
pub struct Sidecar {
    /// All participants.
    participants: Vec<SharedParticipant>,
    /// Configuration.
    config: SidecarConfig,
    /// Status sender for TUI updates.
    status_tx: StatusSender,
}

impl Sidecar {
    /// Creates a new sidecar.
    pub fn new(
        participants: Vec<SharedParticipant>,
        config: SidecarConfig,
        status_tx: StatusSender,
    ) -> Self {
        Self { participants, config, status_tx }
    }

    /// Spawns the sidecar as a background task.
    pub fn spawn(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            self.run().await;
        })
    }

    /// Sends a status update to the TUI.
    fn update_status(&self, action: &str, epoch: u64, certified_blocks: u64) {
        let _ = self.status_tx.send(SidecarStatus {
            action: action.to_string(),
            epoch,
            certified_blocks,
        });
    }

    /// Runs the sidecar loop.
    async fn run(&self) {
        let mut ticker = interval(self.config.commit_interval);
        let mut commit_count: u64 = 0;
        let mut current_epoch: u64 = 0;
        let mut certified_blocks: u64 = 0;

        self.update_status("Initializing...", 0, 0);

        loop {
            ticker.tick().await;

            // Find the current leader
            let leader = self.find_leader().await;

            if let Some(leader) = leader {
                // Use global block number for consistency across leader rotations
                let block_num = certified_blocks + 1;

                // Get the conductor's expected next height (for the payload)
                let conductor_height = leader.conductor().next_height().await;

                // Update status: proposing
                self.update_status(
                    &format!("P{} proposing block {}", leader.id(), block_num),
                    current_epoch,
                    certified_blocks,
                );

                // Create payload with conductor's expected height (required for validation)
                let payload =
                    DemoPayload::new(conductor_height, format!("block-{block_num}").into_bytes());

                // Commit from the leader
                if leader.commit(payload).await.is_err() {
                    self.update_status(
                        "Commit failed, retrying...",
                        current_epoch,
                        certified_blocks,
                    );
                    continue;
                }

                // Update status: collecting acks
                self.update_status(
                    &format!("Collecting acks for block {}", block_num),
                    current_epoch,
                    certified_blocks,
                );

                // Trigger acknowledgments on all participants
                for participant in &self.participants {
                    let _ = participant.acknowledge().await;
                }

                // Block is now certified
                certified_blocks += 1;

                // Update status: certified
                self.update_status(
                    &format!("Block {} certified!", block_num),
                    current_epoch,
                    certified_blocks,
                );

                commit_count += 1;

                // Advance epoch periodically
                if commit_count >= self.config.commits_per_epoch {
                    commit_count = 0;
                    current_epoch += 1;
                    self.update_status(
                        &format!("Rotating leader to epoch {}", current_epoch),
                        current_epoch,
                        certified_blocks,
                    );
                    self.advance_epoch().await;
                }
            } else {
                self.update_status("Waiting for leader...", current_epoch, certified_blocks);
            }
        }
    }

    /// Finds the current leader participant.
    async fn find_leader(&self) -> Option<SharedParticipant> {
        for participant in &self.participants {
            if participant.conductor().leader().await {
                return Some(Arc::clone(participant));
            }
        }
        None
    }

    /// Advances the epoch and notifies all participants.
    async fn advance_epoch(&self) {
        // Use the first participant's epoch manager to advance
        if let Some(first) = self.participants.first() {
            let change = first.epoch_manager().advance_epoch().await;

            // Notify all participants of the epoch change
            for participant in &self.participants {
                // Create a participant-specific change with correct is_self flag
                let is_self = participant.epoch_manager().self_index()
                    == (change.epoch as usize) % participant.epoch_manager().participant_count();

                let participant_change = arturo::EpochChange {
                    epoch: change.epoch,
                    sequencer: change.sequencer.clone(),
                    is_self,
                };

                participant.handle_epoch_change(participant_change).await;
            }
        }
    }
}
