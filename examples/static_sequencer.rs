//! Static Sequencer Configuration Example
//!
//! Demonstrates using `StaticSequencersProvider` with a fixed validator set.
//! This pattern is useful for testing or private networks with known participants.
//!
//! Run with: `cargo run --example static_sequencer`

use std::collections::HashMap;

use arturo::StaticSequencersProvider;
use commonware_consensus::{ordered_broadcast::types::SequencersProvider, types::Epoch};
use commonware_cryptography::{Signer, ed25519};

fn main() {
    // Create three validators from deterministic seeds
    let validator_keys: Vec<_> = (0..3).map(ed25519::PrivateKey::from_seed).collect();
    let validators: Vec<_> = validator_keys.iter().map(|k| k.public_key()).collect();

    // Configure epochs: rotate sequencer each epoch
    let epochs: HashMap<u64, Vec<ed25519::PublicKey>> =
        (0..3).map(|epoch| (epoch, vec![validators[epoch as usize].clone()])).collect();

    let provider = StaticSequencersProvider::new(epochs);

    // Check sequencer for each epoch
    for epoch in 0..3 {
        let sequencers = provider.sequencers(Epoch::new(epoch)).unwrap();
        let is_v0_sequencer = sequencers.iter().any(|k| k == &validators[0]);
        println!("Epoch {}: validator_0 is sequencer = {}", epoch, is_v0_sequencer);
    }

    // Single sequencer mode: same sequencer for all epochs
    let single_provider = StaticSequencersProvider::single(validators[0].clone());
    assert!(
        single_provider.sequencers(Epoch::new(500)).unwrap().iter().any(|k| k == &validators[0])
    );
    println!("Single mode: validator_0 is sequencer for epoch 500");
}
