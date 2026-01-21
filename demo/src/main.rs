//! Arturo demo binary with TUI visualization.
#![allow(unreachable_pub, dead_code, clippy::missing_const_for_fn)]
//!
//! This demo runs multiple participants with round-robin leader election
//! and visualizes the consensus process in a terminal UI.

mod config;
mod epoch;
mod participant;
mod payload;
mod sidecar;
mod status;
mod tui;

use std::sync::Arc;

use clap::Parser;
use commonware_cryptography::{Signer as _, ed25519};

use crate::{
    config::DemoConfig,
    participant::{Participant, SharedParticipant},
    sidecar::{Sidecar, SidecarConfig},
    tui::App,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse configuration
    let config = DemoConfig::parse();

    // Create participant keys from seeds 1..=N
    let all_keys: Vec<ed25519::PublicKey> = (1..=config.participants)
        .map(|seed| ed25519::PrivateKey::from_seed(seed as u64).public_key())
        .collect();

    // Create participants
    let mut participants: Vec<SharedParticipant> = Vec::with_capacity(config.participants);
    for (i, seed) in (1..=config.participants).enumerate() {
        let participant = Participant::new(seed as u64, i + 1, all_keys.clone());
        participants.push(Arc::new(participant));
    }

    // Start all conductors
    for participant in &participants {
        participant.start().await;
    }

    // Set initial leader (participant 1 at epoch 0)
    if let Some(first) = participants.first() {
        let change = arturo::EpochChange {
            epoch: 0,
            sequencer: all_keys[0].clone(),
            is_self: first.epoch_manager().self_index() == 0,
        };
        for (i, participant) in participants.iter().enumerate() {
            let participant_change = arturo::EpochChange {
                epoch: 0,
                sequencer: change.sequencer.clone(),
                is_self: i == 0,
            };
            participant.handle_epoch_change(participant_change).await;
        }
    }

    // Create status channel for sidecar -> TUI communication
    let (status_tx, status_rx) = status::channel();

    // Spawn sidecar
    let sidecar_config = SidecarConfig {
        commit_interval: config.commit_interval(),
        commits_per_epoch: config.commits_per_epoch,
    };
    let sidecar = Arc::new(Sidecar::new(participants.clone(), sidecar_config, status_tx));
    let _sidecar_handle = sidecar.spawn();

    // Run TUI
    let app = App::new(participants.clone(), status_rx);
    tui::run(app).await?;

    // Stop all conductors
    for participant in &participants {
        participant.stop().await;
    }

    Ok(())
}
