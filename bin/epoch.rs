//! HTTP health-based epoch manager.
//!
//! This module implements leader election based on peer health checks.
//! The leader is determined by sorting healthy peers and selecting the first one.
//!
//! ## Tradeoffs
//!
//! **Static Configuration** (simplest):
//! - No network overhead
//! - Manual failover required
//! - Best for single-node deployments or controlled environments
//!
//! **HTTP Health-Based** (this implementation):
//! - Automatic failover when leader becomes unhealthy
//! - Requires health endpoints on all nodes
//! - Leader determined by sorted order of healthy peers
//! - No external dependencies
//! - Trade-off: Health check latency affects failover time
//!
//! **etcd/Consul** (production-grade):
//! - Battle-tested distributed consensus
//! - Strong consistency guarantees
//! - Adds external infrastructure dependency
//! - Better for large-scale deployments

use std::{sync::Arc, time::Duration};

use arturo::{Epoch, EpochChange, EpochManager, EpochStream, TransferError};
use commonware_cryptography::ed25519;
use futures::stream;
use tokio::sync::{RwLock, broadcast};
use tracing::{debug, info, warn};

use crate::health::HealthTracker;

/// State for the health-based epoch manager.
#[derive(Debug)]
struct EpochState {
    /// Current epoch number.
    epoch: Epoch,
    /// Current leader URL (if any).
    leader: Option<String>,
    /// This node's URL.
    self_url: String,
}

/// Health-based epoch manager.
///
/// Determines leadership by polling peer health endpoints and selecting
/// the first healthy peer in sorted order (deterministic leader election).
#[derive(Clone)]
pub struct HealthBasedEpochManager {
    /// Health tracker for peer monitoring.
    health_tracker: HealthTracker,
    /// Internal state.
    state: Arc<RwLock<EpochState>>,
    /// Broadcast channel for epoch changes.
    epoch_tx: broadcast::Sender<EpochChange<ed25519::PublicKey>>,
    /// This node's public key.
    public_key: ed25519::PublicKey,
    /// Peer public keys (indexed by sorted URL order).
    peer_keys: Arc<Vec<ed25519::PublicKey>>,
    /// All node URLs (including self) in sorted order.
    all_urls: Arc<Vec<String>>,
    /// Quorum threshold.
    quorum_threshold: usize,
}

impl std::fmt::Debug for HealthBasedEpochManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HealthBasedEpochManager")
            .field("all_urls", &self.all_urls)
            .field("quorum_threshold", &self.quorum_threshold)
            .finish_non_exhaustive()
    }
}

impl HealthBasedEpochManager {
    /// Creates a new health-based epoch manager.
    ///
    /// # Arguments
    ///
    /// * `self_url` - This node's URL
    /// * `peer_urls` - List of peer URLs
    /// * `public_key` - This node's public key
    /// * `peer_keys` - Public keys of peers (must match peer_urls order after sorting)
    /// * `health_interval` - Interval between health checks
    /// * `quorum_threshold` - Required acknowledgments for certification
    pub fn new(
        self_url: String,
        peer_urls: Vec<String>,
        public_key: ed25519::PublicKey,
        peer_keys: Vec<ed25519::PublicKey>,
        health_interval: Duration,
        quorum_threshold: usize,
    ) -> Self {
        // Create sorted list of all URLs including self
        let mut all_urls: Vec<String> = peer_urls.clone();
        all_urls.push(self_url.clone());
        all_urls.sort();

        let (epoch_tx, _) = broadcast::channel(16);

        Self {
            health_tracker: HealthTracker::new(peer_urls, health_interval),
            state: Arc::new(RwLock::new(EpochState { epoch: 0, leader: None, self_url })),
            epoch_tx,
            public_key,
            peer_keys: Arc::new(peer_keys),
            all_urls: Arc::new(all_urls),
            quorum_threshold,
        }
    }

    /// Spawns the background health polling task.
    pub fn spawn_health_poller(self, interval: Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                self.poll_and_update().await;
            }
        })
    }

    /// Polls peer health and updates leader if needed.
    async fn poll_and_update(&self) {
        self.health_tracker.check_all_peers().await;

        let healthy_peers = self.health_tracker.healthy_peers().await;
        let state = self.state.read().await;

        // Build list of healthy URLs including self
        let mut candidates: Vec<String> = healthy_peers;
        candidates.push(state.self_url.clone());
        candidates.sort();

        // Leader is the first in sorted order
        let new_leader = candidates.first().cloned();

        drop(state);

        // Check if leader changed
        let mut state = self.state.write().await;
        if state.leader != new_leader {
            let old_leader = state.leader.take();
            state.leader = new_leader.clone();
            state.epoch += 1;

            let is_self = new_leader.as_ref() == Some(&state.self_url);

            info!(
                epoch = state.epoch,
                old_leader = ?old_leader,
                new_leader = ?new_leader,
                is_self = is_self,
                "leader changed"
            );

            // Broadcast epoch change
            let change =
                EpochChange { epoch: state.epoch, sequencer: self.public_key.clone(), is_self };

            if self.epoch_tx.send(change).is_err() {
                debug!("no epoch change subscribers");
            }
        }
    }

    /// Returns the public key for a URL.
    fn key_for_url(&self, url: &str) -> Option<ed25519::PublicKey> {
        // Find the index of this URL in the sorted list
        let idx = self.all_urls.iter().position(|u| u == url)?;

        // Check if it's our key
        let state = self.state.try_read().ok()?;
        if url == state.self_url {
            return Some(self.public_key.clone());
        }

        // Otherwise look up in peer_keys
        // Note: peer_keys is indexed by peer order, not all_urls order
        // We need to adjust the index
        let self_idx = self.all_urls.iter().position(|u| u == &state.self_url)?;
        let peer_idx = if idx < self_idx { idx } else { idx - 1 };
        self.peer_keys.get(peer_idx).cloned()
    }
}

impl EpochManager for HealthBasedEpochManager {
    type PublicKey = ed25519::PublicKey;

    fn current_epoch(&self) -> Epoch {
        // Use try_read to avoid blocking, fallback to 0
        self.state.try_read().map(|s| s.epoch).unwrap_or(0)
    }

    fn sequencer(&self, epoch: Epoch) -> Option<Self::PublicKey> {
        let state = self.state.try_read().ok()?;
        if epoch != state.epoch {
            warn!(
                requested = epoch,
                current = state.epoch,
                "sequencer requested for non-current epoch"
            );
            return None;
        }

        state.leader.as_ref().and_then(|url| self.key_for_url(url))
    }

    fn is_sequencer(&self, key: &Self::PublicKey) -> bool {
        *key == self.public_key
            && self
                .state
                .try_read()
                .map(|s| s.leader.as_ref() == Some(&s.self_url))
                .unwrap_or(false)
    }

    async fn transfer_leader(&self) -> Result<(), TransferError> {
        // Health-based leader election doesn't support manual transfer.
        // The leader changes automatically when health status changes.
        Err(TransferError::NotSupported)
    }

    fn subscribe(&self) -> EpochStream<Self::PublicKey> {
        let mut rx = self.epoch_tx.subscribe();
        Box::pin(stream::poll_fn(move |cx| {
            use std::task::Poll;
            match rx.try_recv() {
                Ok(change) => Poll::Ready(Some(change)),
                Err(broadcast::error::TryRecvError::Empty) => {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                Err(_) => Poll::Ready(None),
            }
        }))
    }

    fn validators(&self, epoch: Epoch) -> Option<Vec<Self::PublicKey>> {
        let state = self.state.try_read().ok()?;
        if epoch != state.epoch {
            return None;
        }

        // All peers except the leader are validators
        let mut validators = self.peer_keys.as_ref().clone();
        validators.push(self.public_key.clone());
        Some(validators)
    }

    fn quorum_threshold(&self, _epoch: Epoch) -> Option<usize> {
        Some(self.quorum_threshold)
    }
}

#[cfg(test)]
mod tests {
    use commonware_cryptography::Signer as _;

    use super::*;

    fn create_test_keys() -> (ed25519::PrivateKey, ed25519::PublicKey) {
        let private = ed25519::PrivateKey::from_seed(42);
        let public = private.public_key();
        (private, public)
    }

    #[tokio::test]
    async fn test_epoch_manager_initial_state() {
        let (_, public_key) = create_test_keys();
        let manager = HealthBasedEpochManager::new(
            "http://localhost:8080".to_string(),
            vec!["http://peer1:8080".to_string()],
            public_key,
            vec![ed25519::PrivateKey::from_seed(1).public_key()],
            Duration::from_secs(1),
            1,
        );

        assert_eq!(manager.current_epoch(), 0);
        assert_eq!(manager.quorum_threshold(0), Some(1));
    }

    #[tokio::test]
    async fn test_epoch_manager_validators() {
        let (_, public_key) = create_test_keys();
        let peer_key = ed25519::PrivateKey::from_seed(1).public_key();

        let manager = HealthBasedEpochManager::new(
            "http://localhost:8080".to_string(),
            vec!["http://peer1:8080".to_string()],
            public_key,
            vec![peer_key],
            Duration::from_secs(1),
            2,
        );

        let validators = manager.validators(0).unwrap();
        assert_eq!(validators.len(), 2);
    }

    #[tokio::test]
    async fn test_transfer_not_supported() {
        let (_, public_key) = create_test_keys();
        let manager = HealthBasedEpochManager::new(
            "http://localhost:8080".to_string(),
            vec![],
            public_key,
            vec![],
            Duration::from_secs(1),
            1,
        );

        let result = manager.transfer_leader().await;
        assert!(matches!(result, Err(TransferError::NotSupported)));
    }
}
