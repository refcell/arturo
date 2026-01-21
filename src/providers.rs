//! Provider implementations for commonware integration.
//!
//! This module provides trait implementations that bridge arturo's
//! abstractions with commonware's ordered_broadcast primitives.

use std::{collections::HashMap, sync::Arc};

use commonware_consensus::{
    ordered_broadcast::types::SequencersProvider, types::Epoch as ConsensusEpoch,
};
use commonware_cryptography::PublicKey;
use commonware_utils::ordered::Set;
use tokio::sync::RwLock;

use crate::{traits::EpochManager, types::Epoch};

/// A sequencers provider backed by an [`EpochManager`].
///
/// This bridges arturo's `EpochManager` trait with commonware's
/// `SequencersProvider` trait.
///
/// # Type Parameters
///
/// * `E` - The epoch manager type
/// * `K` - The public key type
pub struct EpochSequencersProvider<E, K>
where
    E: EpochManager<PublicKey = K>,
    K: PublicKey,
{
    epoch_manager: E,
    /// Cache of sequencer sets by epoch.
    cache: Arc<RwLock<HashMap<Epoch, Arc<Set<K>>>>>,
}

impl<E, K> Clone for EpochSequencersProvider<E, K>
where
    E: EpochManager<PublicKey = K>,
    K: PublicKey,
{
    fn clone(&self) -> Self {
        Self { epoch_manager: self.epoch_manager.clone(), cache: Arc::clone(&self.cache) }
    }
}

impl<E, K> std::fmt::Debug for EpochSequencersProvider<E, K>
where
    E: EpochManager<PublicKey = K>,
    K: PublicKey,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EpochSequencersProvider").finish_non_exhaustive()
    }
}

impl<E, K> EpochSequencersProvider<E, K>
where
    E: EpochManager<PublicKey = K>,
    K: PublicKey,
{
    /// Creates a new sequencers provider from an epoch manager.
    pub fn new(epoch_manager: E) -> Self {
        Self { epoch_manager, cache: Arc::new(RwLock::new(HashMap::new())) }
    }

    /// Clears the cache.
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }
}

impl<E, K> SequencersProvider for EpochSequencersProvider<E, K>
where
    E: EpochManager<PublicKey = K>,
    K: PublicKey,
{
    type PublicKey = K;

    fn sequencers(&self, epoch: ConsensusEpoch) -> Option<Arc<Set<Self::PublicKey>>> {
        // For single-sequencer-per-epoch model, we return a set with just the sequencer
        let sequencer = self.epoch_manager.sequencer(epoch.get())?;
        let set = Set::from_iter_dedup([sequencer]);
        Some(Arc::new(set))
    }
}

/// A static sequencers provider with a fixed set of sequencers per epoch.
///
/// Useful for testing or configurations where the sequencer set is
/// known ahead of time.
pub struct StaticSequencersProvider<K: PublicKey> {
    /// Sequencers by epoch.
    epochs: Arc<HashMap<Epoch, Arc<Set<K>>>>,
}

impl<K: PublicKey> Clone for StaticSequencersProvider<K> {
    fn clone(&self) -> Self {
        Self { epochs: Arc::clone(&self.epochs) }
    }
}

impl<K: PublicKey> std::fmt::Debug for StaticSequencersProvider<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StaticSequencersProvider")
            .field("epochs_count", &self.epochs.len())
            .finish()
    }
}

impl<K: PublicKey> StaticSequencersProvider<K> {
    /// Creates a new static provider with the given epoch-sequencer mapping.
    pub fn new(epochs: HashMap<Epoch, Vec<K>>) -> Self {
        let epochs = epochs
            .into_iter()
            .map(|(epoch, sequencers)| {
                let set = Set::from_iter_dedup(sequencers);
                (epoch, Arc::new(set))
            })
            .collect();
        Self { epochs: Arc::new(epochs) }
    }

    /// Creates a provider with a single sequencer for all epochs.
    pub fn single(sequencer: K) -> Self {
        let mut epochs = HashMap::new();
        // Provide sequencer for epochs 0-1000 for testing
        for epoch in 0..1000 {
            epochs.insert(epoch, Arc::new(Set::from_iter_dedup([sequencer.clone()])));
        }
        Self { epochs: Arc::new(epochs) }
    }
}

impl<K: PublicKey> SequencersProvider for StaticSequencersProvider<K> {
    type PublicKey = K;

    fn sequencers(&self, epoch: ConsensusEpoch) -> Option<Arc<Set<Self::PublicKey>>> {
        self.epochs.get(&epoch.get()).cloned()
    }
}

/// A validators provider for ordered_broadcast.
///
/// Similar to sequencers but provides the validator set for chunk signing.
pub struct ValidatorsProvider<E, K>
where
    E: EpochManager<PublicKey = K>,
    K: PublicKey,
{
    epoch_manager: E,
    _key: std::marker::PhantomData<K>,
}

impl<E, K> Clone for ValidatorsProvider<E, K>
where
    E: EpochManager<PublicKey = K>,
    K: PublicKey,
{
    fn clone(&self) -> Self {
        Self { epoch_manager: self.epoch_manager.clone(), _key: std::marker::PhantomData }
    }
}

impl<E, K> std::fmt::Debug for ValidatorsProvider<E, K>
where
    E: EpochManager<PublicKey = K>,
    K: PublicKey,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidatorsProvider").finish_non_exhaustive()
    }
}

impl<E, K> ValidatorsProvider<E, K>
where
    E: EpochManager<PublicKey = K>,
    K: PublicKey,
{
    /// Creates a new validators provider from an epoch manager.
    pub const fn new(epoch_manager: E) -> Self {
        Self { epoch_manager, _key: std::marker::PhantomData }
    }

    /// Returns the validator set for an epoch.
    pub fn validators(&self, epoch: Epoch) -> Option<Vec<K>> {
        self.epoch_manager.validators(epoch)
    }

    /// Returns the quorum threshold for an epoch.
    pub fn quorum_threshold(&self, epoch: Epoch) -> Option<usize> {
        self.epoch_manager.quorum_threshold(epoch)
    }
}

#[cfg(test)]
mod tests {
    use commonware_cryptography::ed25519;
    use futures::stream;

    use super::*;
    use crate::{traits::EpochStream, types::TransferError};

    // Use commonware's ed25519 public key for testing
    type TestPublicKey = ed25519::PublicKey;

    fn create_test_public_key(seed: u64) -> TestPublicKey {
        use commonware_cryptography::Signer;
        ed25519::PrivateKey::from_seed(seed).public_key()
    }

    // Mock epoch manager using ed25519 public keys
    #[derive(Clone)]
    struct MockEpochManager {
        sequencer: TestPublicKey,
        validators: Vec<TestPublicKey>,
    }

    impl EpochManager for MockEpochManager {
        type PublicKey = TestPublicKey;

        fn current_epoch(&self) -> Epoch {
            0
        }

        fn sequencer(&self, _epoch: Epoch) -> Option<Self::PublicKey> {
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

        fn validators(&self, _epoch: Epoch) -> Option<Vec<Self::PublicKey>> {
            Some(self.validators.clone())
        }

        fn quorum_threshold(&self, _epoch: Epoch) -> Option<usize> {
            Some(2)
        }
    }

    #[test]
    fn test_epoch_sequencers_provider() {
        let sequencer = create_test_public_key(1);
        let epoch_manager = MockEpochManager {
            sequencer: sequencer.clone(),
            validators: vec![create_test_public_key(2), create_test_public_key(3)],
        };

        let provider = EpochSequencersProvider::new(epoch_manager);

        let sequencers = provider.sequencers(ConsensusEpoch::new(0)).unwrap();
        assert!((*sequencers).iter().any(|k| k == &sequencer));
        assert_eq!(sequencers.len(), 1);
    }

    #[test]
    fn test_static_sequencers_provider() {
        let sequencer = create_test_public_key(1);
        let provider = StaticSequencersProvider::single(sequencer.clone());

        let sequencers = provider.sequencers(ConsensusEpoch::new(0)).unwrap();
        assert!((*sequencers).iter().any(|k| k == &sequencer));

        let sequencers = provider.sequencers(ConsensusEpoch::new(500)).unwrap();
        assert!((*sequencers).iter().any(|k| k == &sequencer));

        // Epoch out of range returns None
        assert!(provider.sequencers(ConsensusEpoch::new(1001)).is_none());
    }

    #[test]
    fn test_static_sequencers_provider_custom_epochs() {
        let s1 = create_test_public_key(1);
        let s2 = create_test_public_key(2);

        let mut epochs = HashMap::new();
        epochs.insert(0, vec![s1.clone()]);
        epochs.insert(1, vec![s2.clone()]);

        let provider = StaticSequencersProvider::new(epochs);

        let seq0 = provider.sequencers(ConsensusEpoch::new(0)).unwrap();
        assert!((*seq0).iter().any(|k| k == &s1));

        let seq1 = provider.sequencers(ConsensusEpoch::new(1)).unwrap();
        assert!((*seq1).iter().any(|k| k == &s2));

        assert!(provider.sequencers(ConsensusEpoch::new(2)).is_none());
    }

    #[test]
    fn test_validators_provider() {
        let sequencer = create_test_public_key(1);
        let v1 = create_test_public_key(2);
        let v2 = create_test_public_key(3);

        let epoch_manager =
            MockEpochManager { sequencer, validators: vec![v1.clone(), v2.clone()] };

        let provider = ValidatorsProvider::new(epoch_manager);

        let validators = provider.validators(0).unwrap();
        assert_eq!(validators.len(), 2);
        assert!(validators.contains(&v1));
        assert!(validators.contains(&v2));

        let threshold = provider.quorum_threshold(0).unwrap();
        assert_eq!(threshold, 2);
    }
}
