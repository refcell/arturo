//! Simple demo payload type implementing the arturo `Payload` trait.

use arturo::{Height, Payload};
use commonware_cryptography::{Hasher as _, sha256};
use serde::{Deserialize, Serialize};

/// A simple payload for the demo.
///
/// Contains a height, timestamp, and arbitrary data bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DemoPayload {
    /// The height/sequence number of this payload.
    pub height: Height,
    /// Unix timestamp when the payload was created.
    pub timestamp: u64,
    /// Arbitrary data bytes.
    pub data: Vec<u8>,
}

impl DemoPayload {
    /// Creates a new demo payload.
    pub fn new(height: Height, data: Vec<u8>) -> Self {
        let timestamp =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        Self { height, timestamp, data }
    }
}

impl Payload for DemoPayload {
    type Digest = sha256::Digest;

    fn digest(&self) -> Self::Digest {
        let mut hasher = sha256::Sha256::new();
        hasher.update(&self.height.to_le_bytes());
        hasher.update(&self.timestamp.to_le_bytes());
        hasher.update(&self.data);
        hasher.finalize()
    }

    fn height(&self) -> Height {
        self.height
    }

    fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    fn decode(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }
}
