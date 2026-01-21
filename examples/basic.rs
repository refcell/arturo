//! Basic Arturo Example
//!
//! Demonstrates minimal arturo conductor usage with ed25519 cryptography:
//! - Implementing `Payload` with sha256 digests
//! - Implementing `EpochManager` for single-sequencer mode
//! - Creating and running a `Conductor`
//!
//! Run with: `cargo run --example basic`

use arturo::{Conductor, ConductorConfig, EpochManager, EpochStream, Payload, TransferError};
use commonware_cryptography::{Hasher, Signer, ed25519::PrivateKey, sha256};
use futures::stream;

/// A minimal payload type.
#[derive(Clone, Debug)]
struct SimplePayload {
    height: u64,
    data: Vec<u8>,
}

impl Payload for SimplePayload {
    type Digest = sha256::Digest;

    fn digest(&self) -> Self::Digest {
        let mut h = sha256::Sha256::new();
        h.update(&self.height.to_le_bytes());
        h.update(&self.data);
        h.finalize()
    }

    fn height(&self) -> u64 {
        self.height
    }

    fn encode(&self) -> Vec<u8> {
        let mut buf = self.height.to_le_bytes().to_vec();
        buf.extend(&self.data);
        buf
    }

    fn decode(bytes: &[u8]) -> Option<Self> {
        (bytes.len() >= 8).then(|| Self {
            height: u64::from_le_bytes(bytes[..8].try_into().unwrap()),
            data: bytes[8..].to_vec(),
        })
    }
}

/// A static single-sequencer epoch manager.
#[derive(Clone)]
struct StaticEpochManager {
    sequencer: <PrivateKey as Signer>::PublicKey,
}

impl EpochManager for StaticEpochManager {
    type PublicKey = <PrivateKey as Signer>::PublicKey;

    fn current_epoch(&self) -> u64 {
        0
    }
    fn sequencer(&self, _: u64) -> Option<Self::PublicKey> {
        Some(self.sequencer.clone())
    }
    fn is_sequencer(&self, key: &Self::PublicKey) -> bool {
        key == &self.sequencer
    }
    async fn transfer_leader(&self) -> Result<(), TransferError> {
        Err(TransferError::NotSupported)
    }
    fn subscribe(&self) -> EpochStream<Self::PublicKey> {
        Box::pin(stream::empty())
    }
    fn validators(&self, _: u64) -> Option<Vec<Self::PublicKey>> {
        Some(vec![self.sequencer.clone()])
    }
    fn quorum_threshold(&self, _: u64) -> Option<usize> {
        Some(1)
    }
}

#[tokio::main]
async fn main() {
    // Create ed25519 signer and epoch manager
    let signer = PrivateKey::from_seed(42);
    let epoch_manager = StaticEpochManager { sequencer: signer.public_key() };

    // Create and start the conductor
    let conductor: Conductor<SimplePayload, StaticEpochManager, PrivateKey> =
        Conductor::new(ConductorConfig::default(), epoch_manager, signer);
    conductor.start().await;
    println!("Conductor started (leader: {})", conductor.leader().await);

    // Commit a payload and certify via acknowledgment
    let payload = SimplePayload { height: 0, data: b"hello arturo".to_vec() };
    conductor.commit(payload).await.expect("commit failed");
    let certified = conductor.acknowledge().await;
    println!("Certified payload: {:?}", certified.map(|p| p.data));
}
