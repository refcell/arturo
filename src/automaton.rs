//! Generic automaton implementation for payload ordering.
//!
//! This module provides [`PayloadAutomaton`], which implements the
//! commonware [`Automaton`] trait for generic payload types.

use std::{collections::BTreeMap, sync::Arc};

use commonware_consensus::{Automaton, types::Epoch as ConsensusEpoch};
use commonware_cryptography::Digest as DigestTrait;
use futures_channel::oneshot as fc_oneshot;
use tokio::sync::{RwLock, oneshot};

use crate::{
    traits::Payload,
    types::{Height, PendingPayload},
};

/// Context provided to the automaton for proposal and verification.
///
/// This wraps the sequencer identity and height information.
#[derive(Debug, Clone)]
pub struct PayloadContext<K> {
    /// The sequencer proposing the payload.
    pub sequencer: K,
    /// The height being proposed.
    pub height: Height,
}

/// Internal state of the payload automaton.
struct PayloadState<P: Payload> {
    /// The latest certified payload.
    latest_certified: Option<P>,
    /// Pending payload awaiting certification.
    pending: Option<PendingPayload<P>>,
    /// Certified payloads indexed by height.
    by_height: BTreeMap<Height, P>,
    /// Pending proposal channel.
    pending_proposal: Option<oneshot::Sender<P::Digest>>,
}

impl<P: Payload> Default for PayloadState<P> {
    fn default() -> Self {
        Self {
            latest_certified: None,
            pending: None,
            by_height: BTreeMap::new(),
            pending_proposal: None,
        }
    }
}

/// Generic automaton over any [`Payload`] type.
///
/// The `PayloadAutomaton` implements the commonware [`Automaton`] trait,
/// bridging user-defined payload types with the ordered broadcast consensus.
///
/// # Type Parameters
///
/// * `P` - The payload type, must implement [`Payload`]
/// * `K` - The public key type for identifying sequencers
///
/// # Example
///
/// ```ignore
/// use arturo::automaton::PayloadAutomaton;
///
/// let automaton: PayloadAutomaton<MyPayload, PublicKey> = PayloadAutomaton::new();
/// ```
pub struct PayloadAutomaton<P: Payload, K> {
    state: Arc<RwLock<PayloadState<P>>>,
    _key: std::marker::PhantomData<K>,
}

impl<P: Payload, K> Clone for PayloadAutomaton<P, K> {
    fn clone(&self) -> Self {
        Self { state: Arc::clone(&self.state), _key: std::marker::PhantomData }
    }
}

impl<P: Payload, K> std::fmt::Debug for PayloadAutomaton<P, K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PayloadAutomaton").field("state", &"...").finish()
    }
}

impl<P: Payload, K> Default for PayloadAutomaton<P, K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P: Payload, K> PayloadAutomaton<P, K> {
    /// Creates a new payload automaton.
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(PayloadState::default())),
            _key: std::marker::PhantomData,
        }
    }

    /// Creates a new automaton initialized with a genesis payload.
    pub fn with_genesis(genesis: P) -> Self {
        let height = genesis.height();
        let mut by_height = BTreeMap::new();
        by_height.insert(height, genesis.clone());

        Self {
            state: Arc::new(RwLock::new(PayloadState {
                latest_certified: Some(genesis),
                pending: None,
                by_height,
                pending_proposal: None,
            })),
            _key: std::marker::PhantomData,
        }
    }

    /// Returns the latest certified payload.
    pub async fn latest(&self) -> Option<P> {
        self.state.read().await.latest_certified.clone()
    }

    /// Returns the expected next height.
    pub async fn next_height(&self) -> Height {
        self.state.read().await.latest_certified.as_ref().map(|p| p.height() + 1).unwrap_or(0)
    }

    /// Returns a payload by height.
    pub async fn get_by_height(&self, height: Height) -> Option<P> {
        self.state.read().await.by_height.get(&height).cloned()
    }

    /// Submits a payload for proposal.
    ///
    /// This is called by the conductor when acting as sequencer.
    /// Returns a receiver that will yield the digest once the proposal
    /// is ready to be broadcast.
    pub async fn submit_proposal(
        &self,
        payload: P,
        threshold: usize,
    ) -> oneshot::Receiver<P::Digest> {
        let (tx, rx) = oneshot::channel();
        let digest = payload.digest();

        let mut state = self.state.write().await;
        state.pending = Some(PendingPayload::new(payload, threshold));
        state.pending_proposal = Some(tx);

        // Immediately send the digest - the payload is ready for broadcast
        if let Some(sender) = state.pending_proposal.take() {
            let _ = sender.send(digest);
        }

        rx
    }

    /// Records an acknowledgment for the pending payload.
    ///
    /// Returns the certified payload if quorum is reached.
    pub async fn acknowledge(&self) -> Option<P> {
        let mut state = self.state.write().await;

        if let Some(ref mut pending) = state.pending {
            pending.acknowledge();

            if pending.is_certified() {
                let payload = pending.payload.clone();
                let height = payload.height();

                state.by_height.insert(height, payload.clone());
                state.latest_certified = Some(payload.clone());
                state.pending = None;

                return Some(payload);
            }
        }

        None
    }

    /// Certifies a payload directly (for validators receiving certified payloads).
    pub async fn certify(&self, payload: P) {
        let mut state = self.state.write().await;
        let height = payload.height();

        state.by_height.insert(height, payload.clone());

        // Update latest if this is newer
        let should_update =
            state.latest_certified.as_ref().map(|p| payload.height() > p.height()).unwrap_or(true);

        if should_update {
            state.latest_certified = Some(payload);
        }
    }

    /// Validates a payload for correctness.
    ///
    /// Checks:
    /// - Height is sequential
    /// - Parent digest matches (if provided)
    pub async fn validate(&self, payload: &P) -> bool {
        let state = self.state.read().await;

        // Check height is sequential
        let expected_height = state.latest_certified.as_ref().map(|p| p.height() + 1).unwrap_or(0);

        if payload.height() != expected_height {
            return false;
        }

        // Check parent if provided
        if let Some(parent_digest) = payload.parent() {
            let expected_parent = state.latest_certified.as_ref().map(|p| p.digest());
            if Some(parent_digest) != expected_parent {
                return false;
            }
        }

        true
    }
}

/// Implementation of commonware's Automaton trait.
///
/// This bridges our generic payload abstraction with commonware's
/// consensus engine.
impl<P, K> Automaton for PayloadAutomaton<P, K>
where
    P: Payload,
    K: Clone + Send + Sync + 'static,
{
    type Context = PayloadContext<K>;
    type Digest = P::Digest;

    async fn genesis(&mut self, _epoch: ConsensusEpoch) -> Self::Digest {
        self.state
            .read()
            .await
            .latest_certified
            .as_ref()
            .map(|p| p.digest())
            .unwrap_or(<P::Digest as DigestTrait>::EMPTY)
    }

    async fn propose(&mut self, _ctx: Self::Context) -> fc_oneshot::Receiver<Self::Digest> {
        let (tx, rx) = fc_oneshot::channel();

        // Check if we have a pending payload to propose
        let state = self.state.read().await;
        if let Some(ref pending) = state.pending {
            let digest = pending.payload.digest();
            let _ = tx.send(digest);
        }
        // If no pending payload, the receiver will be dropped,
        // signaling that no proposal is ready

        rx
    }

    async fn verify(
        &mut self,
        _ctx: Self::Context,
        digest: Self::Digest,
    ) -> fc_oneshot::Receiver<bool> {
        let (tx, rx) = fc_oneshot::channel();

        // Verify the digest corresponds to a known or valid payload
        let state = self.state.read().await;

        // Check pending payload, or fall back to checking certified payloads
        let valid = state.pending.as_ref().map_or_else(
            || state.by_height.values().any(|p| p.digest() == digest),
            |pending| pending.payload.digest() == digest,
        );

        let _ = tx.send(valid);
        rx
    }
}

#[cfg(test)]
mod tests {
    use commonware_cryptography::{Hasher as _, sha256};

    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct TestPayload {
        data: Vec<u8>,
        height: Height,
        parent: Option<sha256::Digest>,
    }

    impl Payload for TestPayload {
        type Digest = sha256::Digest;

        fn digest(&self) -> Self::Digest {
            let mut hasher = sha256::Sha256::new();
            hasher.update(&self.height.to_le_bytes());
            hasher.update(&self.data);
            hasher.finalize()
        }

        fn height(&self) -> Height {
            self.height
        }

        fn parent(&self) -> Option<Self::Digest> {
            self.parent
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
            Some(Self { data, height, parent: None })
        }
    }

    #[tokio::test]
    async fn test_automaton_genesis() {
        let genesis = TestPayload { data: vec![1, 2, 3], height: 0, parent: None };
        let genesis_digest = genesis.digest();

        let mut automaton: PayloadAutomaton<TestPayload, String> =
            PayloadAutomaton::with_genesis(genesis);

        let result = automaton.genesis(ConsensusEpoch::new(0)).await;
        assert_eq!(result, genesis_digest);
    }

    #[tokio::test]
    async fn test_automaton_empty_genesis() {
        let mut automaton: PayloadAutomaton<TestPayload, String> = PayloadAutomaton::new();

        let result = automaton.genesis(ConsensusEpoch::new(0)).await;
        assert_eq!(result, sha256::Digest::EMPTY);
    }

    #[tokio::test]
    async fn test_submit_and_acknowledge() {
        let automaton: PayloadAutomaton<TestPayload, String> = PayloadAutomaton::new();

        let payload = TestPayload { data: vec![1, 2, 3], height: 0, parent: None };

        // Submit with threshold of 2
        let rx = automaton.submit_proposal(payload.clone(), 2).await;
        let digest = rx.await.unwrap();
        assert_eq!(digest, payload.digest());

        // First ack - not certified yet
        assert!(automaton.acknowledge().await.is_none());

        // Second ack - certified
        let certified = automaton.acknowledge().await;
        assert!(certified.is_some());
        assert_eq!(certified.unwrap(), payload);
    }

    #[tokio::test]
    async fn test_validate_sequential_heights() {
        let genesis = TestPayload { data: vec![1], height: 0, parent: None };
        let automaton: PayloadAutomaton<TestPayload, String> =
            PayloadAutomaton::with_genesis(genesis);

        // Valid next payload
        let valid = TestPayload { data: vec![2], height: 1, parent: None };
        assert!(automaton.validate(&valid).await);

        // Invalid height (skipped)
        let invalid = TestPayload { data: vec![3], height: 5, parent: None };
        assert!(!automaton.validate(&invalid).await);
    }

    #[tokio::test]
    async fn test_validate_parent_digest() {
        let genesis = TestPayload { data: vec![1], height: 0, parent: None };
        let genesis_digest = genesis.digest();
        let automaton: PayloadAutomaton<TestPayload, String> =
            PayloadAutomaton::with_genesis(genesis);

        // Valid parent
        let valid = TestPayload { data: vec![2], height: 1, parent: Some(genesis_digest) };
        assert!(automaton.validate(&valid).await);

        // Invalid parent - create a different digest
        let invalid_parent = {
            let mut hasher = sha256::Sha256::new();
            hasher.update(b"invalid");
            hasher.finalize()
        };
        let invalid = TestPayload { data: vec![3], height: 1, parent: Some(invalid_parent) };
        assert!(!automaton.validate(&invalid).await);
    }

    #[tokio::test]
    async fn test_certify_directly() {
        let automaton: PayloadAutomaton<TestPayload, String> = PayloadAutomaton::new();

        let payload = TestPayload { data: vec![1, 2, 3], height: 0, parent: None };

        automaton.certify(payload.clone()).await;

        let latest = automaton.latest().await;
        assert_eq!(latest, Some(payload));
    }

    #[tokio::test]
    async fn test_get_by_height() {
        let automaton: PayloadAutomaton<TestPayload, String> = PayloadAutomaton::new();

        let p0 = TestPayload { data: vec![1], height: 0, parent: None };
        let p1 = TestPayload { data: vec![2], height: 1, parent: None };

        automaton.certify(p0.clone()).await;
        automaton.certify(p1.clone()).await;

        assert_eq!(automaton.get_by_height(0).await, Some(p0));
        assert_eq!(automaton.get_by_height(1).await, Some(p1));
        assert_eq!(automaton.get_by_height(2).await, None);
    }
}
