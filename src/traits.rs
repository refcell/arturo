//! Core trait abstractions for the arturo conductor.
//!
//! This module defines the primary traits that users must implement to use
//! the conductor:
//!
//! - [`Payload`]: Abstraction over payload types
//! - [`EpochManager`]: Abstraction over epoch/leader management

use std::{future::Future, pin::Pin};

use commonware_cryptography::Digest;
use thiserror::Error;

use crate::types::{Epoch, EpochChange, Height, TransferError};

/// Abstraction over payload types.
///
/// Users implement this trait for their own payload types. The conductor
/// is generic over payloads, allowing any data structure that can be
/// digested and ordered.
///
/// # Example
///
/// ```ignore
/// use arturo::Payload;
///
/// #[derive(Clone)]
/// struct MyPayload {
///     hash: [u8; 32],
///     height: u64,
///     data: Vec<u8>,
/// }
///
/// impl Payload for MyPayload {
///     type Digest = [u8; 32];
///
///     fn digest(&self) -> Self::Digest {
///         self.hash
///     }
///
///     fn height(&self) -> u64 {
///         self.height
///     }
/// }
/// ```
pub trait Payload: Clone + Send + Sync + 'static {
    /// The digest type for this payload.
    ///
    /// Must implement commonware's [`Digest`] trait for cryptographic
    /// integrity.
    type Digest: Digest;

    /// Compute the digest of this payload.
    ///
    /// The digest must be deterministic - calling this method on the same
    /// payload must always return the same digest.
    fn digest(&self) -> Self::Digest;

    /// Returns the height/sequence number of this payload.
    ///
    /// Heights must be monotonically increasing within a chain.
    fn height(&self) -> Height;

    /// Returns the parent digest for chain validation.
    ///
    /// Returns `None` for genesis payloads or when parent tracking is
    /// not needed.
    fn parent(&self) -> Option<Self::Digest> {
        None
    }

    /// Serialize the payload to bytes.
    ///
    /// Used for network transmission and storage.
    fn encode(&self) -> Vec<u8>;

    /// Deserialize a payload from bytes.
    ///
    /// Returns `None` if the bytes are invalid.
    fn decode(bytes: &[u8]) -> Option<Self>;
}

/// A stream of epoch changes.
///
/// This is a boxed stream to allow for different implementations.
pub type EpochStream<K> = Pin<Box<dyn futures::Stream<Item = EpochChange<K>> + Send>>;

/// Abstraction over epoch/leader management.
///
/// This trait allows pluggable leader election and epoch management
/// strategies. Implementations can range from static configuration
/// to complex distributed protocols.
///
/// # Single Sequencer per Epoch
///
/// The conductor assumes a single sequencer per epoch. The epoch manager
/// is responsible for determining who the sequencer is for each epoch.
///
/// # Example
///
/// ```ignore
/// use arturo::{EpochManager, Epoch, EpochChange, TransferError};
///
/// #[derive(Clone)]
/// struct StaticEpochManager {
///     sequencer: PublicKey,
/// }
///
/// impl EpochManager for StaticEpochManager {
///     type PublicKey = PublicKey;
///
///     fn current_epoch(&self) -> Epoch { 0 }
///     fn sequencer(&self, _: Epoch) -> Option<Self::PublicKey> {
///         Some(self.sequencer.clone())
///     }
///     // ...
/// }
/// ```
pub trait EpochManager: Clone + Send + Sync + 'static {
    /// The public key type used to identify participants.
    type PublicKey: Clone + Send + Sync + Eq + std::hash::Hash + std::fmt::Debug;

    /// Returns the current epoch number.
    fn current_epoch(&self) -> Epoch;

    /// Returns the sequencer (leader) for a given epoch.
    ///
    /// Returns `None` if the epoch is unknown or has no assigned sequencer.
    fn sequencer(&self, epoch: Epoch) -> Option<Self::PublicKey>;

    /// Checks if a public key is the current sequencer.
    fn is_sequencer(&self, key: &Self::PublicKey) -> bool {
        self.sequencer(self.current_epoch()).map(|s| &s == key).unwrap_or(false)
    }

    /// Requests a leadership transfer.
    ///
    /// The implementation defines what "transfer" means - it could be
    /// stepping down, nominating a successor, or triggering an election.
    ///
    /// Returns an error if transfer is not supported or fails.
    fn transfer_leader(&self) -> impl Future<Output = Result<(), TransferError>> + Send;

    /// Subscribes to epoch/leader changes.
    ///
    /// Returns a stream that emits [`EpochChange`] events whenever the
    /// epoch transitions or the sequencer changes.
    fn subscribe(&self) -> EpochStream<Self::PublicKey>;

    /// Returns the set of validators for a given epoch.
    ///
    /// Validators are nodes that acknowledge and sign chunks from the
    /// sequencer.
    fn validators(&self, epoch: Epoch) -> Option<Vec<Self::PublicKey>>;

    /// Returns the quorum threshold for a given epoch.
    ///
    /// This is typically `2f + 1` where `f` is the maximum number of
    /// Byzantine failures tolerated.
    fn quorum_threshold(&self, epoch: Epoch) -> Option<usize>;
}

/// Provider for payload storage and retrieval.
///
/// This trait abstracts over how payloads are stored and retrieved,
/// allowing for different backend implementations.
pub trait PayloadStore<P: Payload>: Clone + Send + Sync + 'static {
    /// Stores a certified payload.
    fn store(&self, payload: &P) -> impl Future<Output = Result<(), StoreError>> + Send;

    /// Retrieves a payload by its digest.
    fn get(&self, digest: &P::Digest) -> impl Future<Output = Option<P>> + Send;

    /// Retrieves a payload by height.
    fn get_by_height(&self, height: Height) -> impl Future<Output = Option<P>> + Send;

    /// Returns the latest certified payload.
    fn latest(&self) -> impl Future<Output = Option<P>> + Send;
}

/// Errors that can occur during storage operations.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StoreError {
    /// The payload already exists.
    #[error("payload already exists")]
    AlreadyExists,

    /// Storage backend error.
    #[error("storage error: {0}")]
    Backend(String),
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{Arc, RwLock},
    };

    use commonware_cryptography::sha256;
    use rstest::rstest;

    use super::*;

    // Test payload using commonware's sha256::Digest
    #[derive(Clone, Debug, PartialEq)]
    struct TestPayload {
        data: Vec<u8>,
        height: Height,
    }

    impl Payload for TestPayload {
        type Digest = sha256::Digest;

        fn digest(&self) -> Self::Digest {
            use commonware_cryptography::Hasher as _;
            let mut hasher = commonware_cryptography::sha256::Sha256::new();
            hasher.update(&self.height.to_le_bytes());
            hasher.update(&self.data);
            hasher.finalize()
        }

        fn height(&self) -> Height {
            self.height
        }

        fn encode(&self) -> Vec<u8> {
            let mut buf = Vec::new();
            buf.extend_from_slice(&self.height.to_le_bytes());
            buf.extend_from_slice(&self.data);
            buf
        }

        fn decode(bytes: &[u8]) -> Option<Self> {
            if bytes.len() < 8 {
                return None;
            }
            let height = u64::from_le_bytes(bytes[..8].try_into().ok()?);
            let data = bytes[8..].to_vec();
            Some(Self { data, height })
        }
    }

    #[test]
    fn payload_digest_determinism() {
        let payload = TestPayload { data: vec![1, 2, 3, 4], height: 1 };
        let d1 = payload.digest();
        let d2 = payload.digest();
        assert_eq!(d1, d2);
    }

    #[rstest]
    #[case::simple(vec![1, 2, 3, 4], 42)]
    #[case::empty_data(vec![], 0)]
    #[case::large_height(vec![255], u64::MAX)]
    fn payload_encode_decode(#[case] data: Vec<u8>, #[case] height: Height) {
        let payload = TestPayload { data, height };
        let encoded = payload.encode();
        let decoded = TestPayload::decode(&encoded).unwrap();
        assert_eq!(payload, decoded);
    }

    #[rstest]
    #[case::already_exists(StoreError::AlreadyExists, "payload already exists")]
    #[case::backend(StoreError::Backend("db error".to_string()), "storage error: db error")]
    fn store_error_display(#[case] error: StoreError, #[case] expected: &str) {
        assert_eq!(format!("{error}"), expected);
    }

    // Test a simple in-memory store
    #[derive(Clone)]
    struct InMemoryStore<P: Payload> {
        payloads: Arc<RwLock<HashMap<Height, P>>>,
    }

    impl<P: Payload> InMemoryStore<P> {
        fn new() -> Self {
            Self { payloads: Arc::new(RwLock::new(HashMap::new())) }
        }
    }

    impl<P: Payload> PayloadStore<P> for InMemoryStore<P> {
        async fn store(&self, payload: &P) -> Result<(), StoreError> {
            let mut payloads = self.payloads.write().unwrap();
            payloads.insert(payload.height(), payload.clone());
            Ok(())
        }

        async fn get(&self, digest: &P::Digest) -> Option<P> {
            let payloads = self.payloads.read().unwrap();
            payloads.values().find(|p| &p.digest() == digest).cloned()
        }

        async fn get_by_height(&self, height: Height) -> Option<P> {
            let payloads = self.payloads.read().unwrap();
            payloads.get(&height).cloned()
        }

        async fn latest(&self) -> Option<P> {
            let payloads = self.payloads.read().unwrap();
            payloads.values().max_by_key(|p| p.height()).cloned()
        }
    }

    #[tokio::test]
    async fn in_memory_store() {
        let store = InMemoryStore::<TestPayload>::new();

        let payload = TestPayload { data: vec![1, 2, 3], height: 1 };
        store.store(&payload).await.unwrap();

        let retrieved = store.get_by_height(1).await.unwrap();
        assert_eq!(retrieved, payload);

        let latest = store.latest().await.unwrap();
        assert_eq!(latest, payload);
    }
}
