//! Custom Payload Example
//!
//! Demonstrates implementing `arturo::Payload` for a block-like type with
//! timestamp, parent hash, and data fields.
//!
//! Run with: `cargo run --example custom_payload`

use arturo::Payload;
use commonware_cryptography::{Hasher as _, sha256};

/// A block-like payload with timestamp, parent reference, and data.
#[derive(Clone, Debug, PartialEq)]
struct BlockPayload {
    height: u64,
    timestamp: u64,
    parent: Option<sha256::Digest>,
    data: Vec<u8>,
}

impl Payload for BlockPayload {
    type Digest = sha256::Digest;

    fn digest(&self) -> Self::Digest {
        let mut hasher = sha256::Sha256::new();
        hasher.update(&self.height.to_le_bytes());
        hasher.update(&self.timestamp.to_le_bytes());
        if let Some(ref p) = self.parent {
            hasher.update(p.as_ref());
        }
        hasher.update(&self.data);
        hasher.finalize()
    }

    fn height(&self) -> u64 {
        self.height
    }
    fn parent(&self) -> Option<Self::Digest> {
        self.parent
    }

    fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(17 + self.parent.map_or(0, |_| 32) + self.data.len());
        buf.extend_from_slice(&self.height.to_le_bytes());
        buf.extend_from_slice(&self.timestamp.to_le_bytes());
        match &self.parent {
            Some(p) => {
                buf.push(1);
                buf.extend_from_slice(p.as_ref());
            }
            None => buf.push(0),
        }
        buf.extend_from_slice(&self.data);
        buf
    }

    fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 17 {
            return None;
        }
        let height = u64::from_le_bytes(bytes[0..8].try_into().ok()?);
        let timestamp = u64::from_le_bytes(bytes[8..16].try_into().ok()?);
        let (parent, off) = if bytes[16] == 1 {
            let arr: [u8; 32] = bytes.get(17..49)?.try_into().ok()?;
            (Some(sha256::Digest::from(arr)), 49)
        } else {
            (None, 17)
        };
        Some(Self { height, timestamp, parent, data: bytes.get(off..)?.to_vec() })
    }
}

fn main() {
    // Genesis block (no parent)
    let genesis =
        BlockPayload { height: 0, timestamp: 1700000000, parent: None, data: b"genesis".to_vec() };
    let genesis_digest = genesis.digest();
    println!("Genesis digest: {genesis_digest:?}");

    // Child block referencing genesis
    let block1 = BlockPayload {
        height: 1,
        timestamp: 1700000012,
        parent: Some(genesis_digest),
        data: b"block 1".to_vec(),
    };
    println!("Block 1 digest: {:?}", block1.digest());
    println!("Block 1 parent: {:?}", block1.parent());

    // Verify encode/decode roundtrip
    assert_eq!(block1, BlockPayload::decode(&block1.encode()).unwrap());
    println!("Encode/decode roundtrip: OK");
}
