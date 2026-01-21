//! Generic conductor orchestrator.
//!
//! The [`Conductor`] is the main entry point for the arturo consensus layer.
//! It orchestrates payload ordering, certification, and epoch management.

use std::{marker::PhantomData, sync::Arc};

use commonware_cryptography::Signer;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::{
    automaton::PayloadAutomaton,
    traits::{EpochManager, EpochStream, Payload},
    types::{ConductorError, EpochChange, TransferError},
};

/// Configuration for the conductor.
#[derive(Debug, Clone)]
pub struct ConductorConfig {
    /// Number of acknowledgments required for certification.
    ///
    /// Typically `2f + 1` where `f` is the Byzantine fault tolerance.
    pub quorum_threshold: usize,
}

impl Default for ConductorConfig {
    fn default() -> Self {
        Self { quorum_threshold: 1 }
    }
}

/// Internal state of the conductor.
#[derive(Default)]
struct ConductorState {
    /// Whether the conductor is running.
    running: bool,
    /// Current epoch.
    current_epoch: u64,
    /// Whether we are currently the sequencer.
    is_sequencer: bool,
}

/// Generic conductor over Payload, EpochManager, and Crypto scheme.
///
/// The conductor is the main orchestrator that:
/// - Manages payload proposal and certification
/// - Tracks epoch transitions and sequencer identity
/// - Provides the primary API for committing payloads
///
/// # Type Parameters
///
/// * `P` - The payload type, must implement [`Payload`]
/// * `E` - The epoch manager, must implement [`EpochManager`]
/// * `S` - The cryptographic signer, must implement [`Signer`]
///
/// # Example
///
/// ```ignore
/// use arturo::{Conductor, ConductorConfig};
///
/// let conductor: Conductor<MyPayload, MyEpochManager, Ed25519Signer> =
///     Conductor::new(config, epoch_manager, signer);
///
/// // Check if we're the leader
/// if conductor.leader().await {
///     conductor.commit(payload).await?;
/// }
///
/// // Get the latest certified payload
/// let latest = conductor.latest().await;
/// ```
pub struct Conductor<P, E, S>
where
    P: Payload,
    E: EpochManager,
    S: Signer,
{
    /// Configuration.
    config: ConductorConfig,
    /// The payload automaton.
    automaton: PayloadAutomaton<P, E::PublicKey>,
    /// The epoch manager.
    epoch_manager: E,
    /// Our signer.
    signer: S,
    /// Internal state.
    state: Arc<RwLock<ConductorState>>,
    /// Marker for the signer's public key type.
    _crypto: PhantomData<S>,
}

impl<P, E, S> Clone for Conductor<P, E, S>
where
    P: Payload,
    E: EpochManager,
    S: Signer + Clone,
{
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            automaton: self.automaton.clone(),
            epoch_manager: self.epoch_manager.clone(),
            signer: self.signer.clone(),
            state: Arc::clone(&self.state),
            _crypto: PhantomData,
        }
    }
}

impl<P, E, S> std::fmt::Debug for Conductor<P, E, S>
where
    P: Payload,
    E: EpochManager,
    S: Signer,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Conductor").field("config", &self.config).finish_non_exhaustive()
    }
}

impl<P, E, S> Conductor<P, E, S>
where
    P: Payload,
    E: EpochManager,
    S: Signer,
{
    /// Creates a new conductor.
    pub fn new(config: ConductorConfig, epoch_manager: E, signer: S) -> Self {
        Self {
            config,
            automaton: PayloadAutomaton::new(),
            epoch_manager,
            signer,
            state: Arc::new(RwLock::new(ConductorState::default())),
            _crypto: PhantomData,
        }
    }

    /// Creates a new conductor initialized with a genesis payload.
    pub fn with_genesis(config: ConductorConfig, epoch_manager: E, signer: S, genesis: P) -> Self {
        Self {
            config,
            automaton: PayloadAutomaton::with_genesis(genesis),
            epoch_manager,
            signer,
            state: Arc::new(RwLock::new(ConductorState::default())),
            _crypto: PhantomData,
        }
    }

    /// Returns whether the local node is currently the sequencer (leader).
    pub async fn leader(&self) -> bool {
        self.state.read().await.is_sequencer
    }

    /// Returns a stream of epoch/leader changes.
    ///
    /// Use this to be notified when leadership changes.
    pub fn leader_channel(&self) -> EpochStream<E::PublicKey> {
        self.epoch_manager.subscribe()
    }

    /// Commits a payload.
    ///
    /// This is the primary method for proposing new payloads. It will:
    /// 1. Verify the caller is the current sequencer
    /// 2. Validate the payload
    /// 3. Submit it for certification
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The caller is not the current sequencer
    /// - The payload fails validation
    /// - The internal channel is closed
    pub async fn commit(&self, payload: P) -> Result<(), ConductorError> {
        // Check if we're the sequencer
        if !self.state.read().await.is_sequencer {
            warn!("commit called but not sequencer");
            return Err(ConductorError::NotSequencer);
        }

        // Validate the payload
        if !self.automaton.validate(&payload).await {
            let expected_height = self.automaton.next_height().await;
            let got_height = payload.height();

            if expected_height != got_height {
                return Err(ConductorError::InvalidHeight {
                    expected: expected_height,
                    got: got_height,
                });
            }

            // Must be a parent mismatch
            return Err(ConductorError::ValidationFailed("parent digest mismatch".to_string()));
        }

        // Get the quorum threshold
        let epoch = self.state.read().await.current_epoch;
        let threshold =
            self.epoch_manager.quorum_threshold(epoch).unwrap_or(self.config.quorum_threshold);

        debug!(
            height = payload.height(),
            threshold = threshold,
            "submitting payload for certification"
        );

        // Submit for certification
        let rx = self.automaton.submit_proposal(payload, threshold).await;

        // Wait for the digest (proposal accepted)
        rx.await.map_or(Err(ConductorError::ChannelClosed), |digest| {
            debug!(?digest, "payload proposal accepted");
            Ok(())
        })
    }

    /// Returns the latest certified payload.
    pub async fn latest(&self) -> Option<P> {
        self.automaton.latest().await
    }

    /// Returns a certified payload by height.
    pub async fn get_by_height(&self, height: u64) -> Option<P> {
        self.automaton.get_by_height(height).await
    }

    /// Returns the expected next height.
    pub async fn next_height(&self) -> u64 {
        self.automaton.next_height().await
    }

    /// Requests a leadership transfer.
    ///
    /// Delegates to the epoch manager's transfer mechanism.
    pub async fn transfer_leader(&self) -> Result<(), TransferError> {
        info!("requesting leadership transfer");
        self.epoch_manager.transfer_leader().await
    }

    /// Starts the conductor.
    ///
    /// This initializes the conductor and begins processing epoch changes.
    /// Should be called before any other operations.
    pub async fn start(&self) {
        let mut state = self.state.write().await;
        state.running = true;
        state.current_epoch = self.epoch_manager.current_epoch();

        // Check if we're the sequencer for the current epoch
        if let Some(sequencer) = self.epoch_manager.sequencer(state.current_epoch) {
            // We need to compare with our identity
            // This requires knowing our public key from the signer
            state.is_sequencer = self.epoch_manager.is_sequencer(&sequencer);
        }

        info!(epoch = state.current_epoch, is_sequencer = state.is_sequencer, "conductor started");
    }

    /// Stops the conductor.
    pub async fn stop(&self) {
        let mut state = self.state.write().await;
        state.running = false;
        info!("conductor stopped");
    }

    /// Returns whether the conductor is running.
    pub async fn is_running(&self) -> bool {
        self.state.read().await.running
    }

    /// Returns the current epoch.
    pub async fn current_epoch(&self) -> u64 {
        self.state.read().await.current_epoch
    }

    /// Handles an epoch change notification.
    ///
    /// Updates internal state when the epoch transitions.
    pub async fn handle_epoch_change(&self, change: EpochChange<E::PublicKey>) {
        let mut state = self.state.write().await;
        state.current_epoch = change.epoch;
        state.is_sequencer = change.is_self;

        info!(epoch = change.epoch, is_sequencer = change.is_self, "epoch changed");
    }

    /// Records an acknowledgment for the current pending payload.
    ///
    /// Called when receiving an ack from a validator.
    /// Returns the certified payload if quorum is reached.
    pub async fn acknowledge(&self) -> Option<P> {
        self.automaton.acknowledge().await
    }

    /// Certifies a payload directly.
    ///
    /// Used by validators to record payloads that have been certified
    /// by the sequencer.
    pub async fn certify(&self, payload: P) {
        self.automaton.certify(payload).await;
    }

    /// Returns a reference to the automaton.
    ///
    /// Useful for integrating with commonware's Engine.
    pub const fn automaton(&self) -> &PayloadAutomaton<P, E::PublicKey> {
        &self.automaton
    }

    /// Returns a reference to the epoch manager.
    pub const fn epoch_manager(&self) -> &E {
        &self.epoch_manager
    }

    /// Returns a reference to the signer.
    pub const fn signer(&self) -> &S {
        &self.signer
    }
}

#[cfg(test)]
mod tests {
    use commonware_cryptography::{Hasher as _, ed25519, sha256};
    use futures::stream;

    use super::*;
    use crate::types::Height;

    // Test payload using commonware's sha256::Digest
    #[derive(Clone, Debug, PartialEq)]
    struct TestPayload {
        data: Vec<u8>,
        height: Height,
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

    // Mock epoch manager
    #[derive(Clone)]
    struct MockEpochManager {
        is_sequencer: bool,
    }

    impl EpochManager for MockEpochManager {
        type PublicKey = String;

        fn current_epoch(&self) -> u64 {
            0
        }

        fn sequencer(&self, _epoch: u64) -> Option<Self::PublicKey> {
            Some("sequencer".to_string())
        }

        fn is_sequencer(&self, _key: &Self::PublicKey) -> bool {
            self.is_sequencer
        }

        async fn transfer_leader(&self) -> Result<(), TransferError> {
            Err(TransferError::NotSupported)
        }

        fn subscribe(&self) -> EpochStream<Self::PublicKey> {
            Box::pin(stream::empty())
        }

        fn validators(&self, _epoch: u64) -> Option<Vec<Self::PublicKey>> {
            Some(vec!["validator1".to_string(), "validator2".to_string()])
        }

        fn quorum_threshold(&self, _epoch: u64) -> Option<usize> {
            Some(2)
        }
    }

    // Use commonware's ed25519 signer for testing
    type MockSigner = ed25519::PrivateKey;

    fn create_test_signer() -> MockSigner {
        use commonware_cryptography::Signer as _;
        MockSigner::from_seed(42)
    }

    #[tokio::test]
    async fn test_conductor_not_sequencer() {
        let config = ConductorConfig::default();
        let epoch_manager = MockEpochManager { is_sequencer: false };
        let signer = create_test_signer();

        let conductor: Conductor<TestPayload, MockEpochManager, MockSigner> =
            Conductor::new(config, epoch_manager, signer);
        conductor.start().await;

        let payload = TestPayload { data: vec![1, 2, 3], height: 0 };

        let result = conductor.commit(payload).await;
        assert!(matches!(result, Err(ConductorError::NotSequencer)));
    }

    #[tokio::test]
    async fn test_conductor_commit_as_sequencer() {
        let config = ConductorConfig::default();
        let epoch_manager = MockEpochManager { is_sequencer: true };
        let signer = create_test_signer();

        let conductor: Conductor<TestPayload, MockEpochManager, MockSigner> =
            Conductor::new(config, epoch_manager, signer);

        // Manually set as sequencer for testing
        {
            let mut state = conductor.state.write().await;
            state.is_sequencer = true;
            state.running = true;
        }

        let payload = TestPayload { data: vec![1, 2, 3], height: 0 };

        let result = conductor.commit(payload).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_conductor_invalid_height() {
        let config = ConductorConfig::default();
        let epoch_manager = MockEpochManager { is_sequencer: true };
        let signer = create_test_signer();

        let genesis = TestPayload { data: vec![0], height: 0 };
        let conductor: Conductor<TestPayload, MockEpochManager, MockSigner> =
            Conductor::with_genesis(config, epoch_manager, signer, genesis);

        // Manually set as sequencer for testing
        {
            let mut state = conductor.state.write().await;
            state.is_sequencer = true;
            state.running = true;
        }

        // Try to commit with wrong height
        let payload = TestPayload {
            data: vec![1, 2, 3],
            height: 5, // Should be 1
        };

        let result = conductor.commit(payload).await;
        assert!(matches!(result, Err(ConductorError::InvalidHeight { expected: 1, got: 5 })));
    }

    #[tokio::test]
    async fn test_conductor_epoch_change() {
        let config = ConductorConfig::default();
        let epoch_manager = MockEpochManager { is_sequencer: false };
        let signer = create_test_signer();

        let conductor: Conductor<TestPayload, MockEpochManager, MockSigner> =
            Conductor::new(config, epoch_manager, signer);
        conductor.start().await;

        assert!(!conductor.leader().await);

        // Handle epoch change where we become sequencer
        conductor
            .handle_epoch_change(EpochChange {
                epoch: 1,
                sequencer: "us".to_string(),
                is_self: true,
            })
            .await;

        assert!(conductor.leader().await);
        assert_eq!(conductor.current_epoch().await, 1);
    }

    #[tokio::test]
    async fn test_conductor_latest() {
        let config = ConductorConfig::default();
        let epoch_manager = MockEpochManager { is_sequencer: true };
        let signer = create_test_signer();

        let genesis = TestPayload { data: vec![0], height: 0 };
        let conductor: Conductor<TestPayload, MockEpochManager, MockSigner> =
            Conductor::with_genesis(config, epoch_manager, signer, genesis.clone());

        let latest = conductor.latest().await;
        assert_eq!(latest, Some(genesis));
    }

    #[tokio::test]
    async fn test_conductor_acknowledge() {
        let config = ConductorConfig { quorum_threshold: 2 };
        let epoch_manager = MockEpochManager { is_sequencer: true };
        let signer = create_test_signer();

        let conductor: Conductor<TestPayload, MockEpochManager, MockSigner> =
            Conductor::new(config, epoch_manager, signer);

        // Manually set as sequencer
        {
            let mut state = conductor.state.write().await;
            state.is_sequencer = true;
            state.running = true;
        }

        let payload = TestPayload { data: vec![1, 2, 3], height: 0 };

        // Commit the payload
        conductor.commit(payload.clone()).await.unwrap();

        // First ack - not certified
        assert!(conductor.acknowledge().await.is_none());

        // Second ack - certified
        let certified = conductor.acknowledge().await;
        assert!(certified.is_some());
        assert_eq!(certified.unwrap(), payload);
    }

    #[tokio::test]
    async fn test_conductor_transfer_leader() {
        let config = ConductorConfig::default();
        let epoch_manager = MockEpochManager { is_sequencer: true };
        let signer = create_test_signer();

        let conductor: Conductor<TestPayload, MockEpochManager, MockSigner> =
            Conductor::new(config, epoch_manager, signer);

        let result = conductor.transfer_leader().await;
        assert!(matches!(result, Err(TransferError::NotSupported)));
    }
}
