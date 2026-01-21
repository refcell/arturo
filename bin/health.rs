//! Health checking for peer nodes.
//!
//! Provides both a health server (axum handler) and client (reqwest)
//! for monitoring peer node health status.

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Health status response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Whether the node is healthy.
    pub healthy: bool,
    /// The node's identity.
    pub identity: String,
    /// Current epoch.
    pub epoch: u64,
    /// Whether this node is the current leader.
    pub is_leader: bool,
}

/// Tracked health status of a peer.
#[derive(Debug, Clone)]
pub struct PeerHealth {
    /// Peer URL.
    pub url: String,
    /// Whether the peer is currently healthy.
    pub healthy: bool,
    /// Last successful health check time.
    pub last_seen: Option<Instant>,
    /// Number of consecutive failures.
    pub consecutive_failures: u32,
}

impl PeerHealth {
    /// Creates a new peer health tracker.
    pub fn new(url: String) -> Self {
        Self { url, healthy: false, last_seen: None, consecutive_failures: 0 }
    }

    /// Marks the peer as healthy.
    pub fn mark_healthy(&mut self) {
        self.healthy = true;
        self.last_seen = Some(Instant::now());
        self.consecutive_failures = 0;
    }

    /// Marks the peer as unhealthy.
    pub fn mark_unhealthy(&mut self) {
        self.healthy = false;
        self.consecutive_failures += 1;
    }
}

/// Shared state for health tracking.
#[derive(Debug, Clone)]
pub struct HealthTracker {
    /// Map of peer URL to health status.
    peers: Arc<RwLock<HashMap<String, PeerHealth>>>,
    /// HTTP client for health checks.
    client: reqwest::Client,
    /// Health check timeout.
    timeout: Duration,
}

impl HealthTracker {
    /// Creates a new health tracker.
    pub fn new(peer_urls: Vec<String>, timeout: Duration) -> Self {
        let mut peers = HashMap::new();
        for url in peer_urls {
            peers.insert(url.clone(), PeerHealth::new(url));
        }

        Self {
            peers: Arc::new(RwLock::new(peers)),
            client: reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .expect("failed to build reqwest client"),
            timeout,
        }
    }

    /// Check health of a single peer.
    pub async fn check_peer(&self, url: &str) -> bool {
        let health_url = format!("{url}/health");

        match self.client.get(&health_url).timeout(self.timeout).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    if let Ok(status) = response.json::<HealthStatus>().await {
                        debug!(peer = %url, healthy = %status.healthy, "health check succeeded");
                        return status.healthy;
                    }
                }
                warn!(peer = %url, "health check returned non-success status");
                false
            }
            Err(e) => {
                warn!(peer = %url, error = %e, "health check failed");
                false
            }
        }
    }

    /// Check health of all peers.
    pub async fn check_all_peers(&self) {
        let urls: Vec<String> = {
            let peers = self.peers.read().await;
            peers.keys().cloned().collect()
        };

        for url in urls {
            let healthy = self.check_peer(&url).await;
            let mut peers = self.peers.write().await;
            if let Some(peer) = peers.get_mut(&url) {
                if healthy {
                    peer.mark_healthy();
                } else {
                    peer.mark_unhealthy();
                }
            }
        }
    }

    /// Returns a sorted list of healthy peer URLs.
    pub async fn healthy_peers(&self) -> Vec<String> {
        let peers = self.peers.read().await;
        let mut healthy: Vec<_> =
            peers.values().filter(|p| p.healthy).map(|p| p.url.clone()).collect();
        healthy.sort();
        healthy
    }

    /// Returns all peer health statuses.
    pub async fn all_peers(&self) -> Vec<PeerHealth> {
        let peers = self.peers.read().await;
        peers.values().cloned().collect()
    }
}

/// Shared state for the health endpoint.
#[derive(Clone)]
pub struct HealthState {
    /// This node's identity.
    pub identity: String,
    /// Current epoch (updated by epoch manager).
    pub epoch: Arc<RwLock<u64>>,
    /// Whether this node is the leader (updated by epoch manager).
    pub is_leader: Arc<RwLock<bool>>,
}

impl HealthState {
    /// Creates a new health state.
    pub fn new(identity: String) -> Self {
        Self { identity, epoch: Arc::new(RwLock::new(0)), is_leader: Arc::new(RwLock::new(false)) }
    }

    /// Updates the current epoch.
    pub async fn set_epoch(&self, epoch: u64) {
        *self.epoch.write().await = epoch;
    }

    /// Updates the leader status.
    pub async fn set_is_leader(&self, is_leader: bool) {
        *self.is_leader.write().await = is_leader;
    }
}

/// Health endpoint handler.
pub async fn health_handler(State(state): State<HealthState>) -> impl IntoResponse {
    let epoch = *state.epoch.read().await;
    let is_leader = *state.is_leader.read().await;

    let status = HealthStatus { healthy: true, identity: state.identity.clone(), epoch, is_leader };

    (StatusCode::OK, Json(status))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_health_new() {
        let peer = PeerHealth::new("http://localhost:8080".to_string());
        assert!(!peer.healthy);
        assert!(peer.last_seen.is_none());
        assert_eq!(peer.consecutive_failures, 0);
    }

    #[test]
    fn test_peer_health_mark_healthy() {
        let mut peer = PeerHealth::new("http://localhost:8080".to_string());
        peer.mark_unhealthy();
        peer.mark_unhealthy();
        assert_eq!(peer.consecutive_failures, 2);

        peer.mark_healthy();
        assert!(peer.healthy);
        assert!(peer.last_seen.is_some());
        assert_eq!(peer.consecutive_failures, 0);
    }

    #[tokio::test]
    async fn test_health_tracker_healthy_peers() {
        let tracker = HealthTracker::new(
            vec!["http://a:8080".to_string(), "http://b:8080".to_string()],
            Duration::from_secs(5),
        );

        // Initially no peers are healthy
        assert!(tracker.healthy_peers().await.is_empty());

        // Mark one as healthy
        {
            let mut peers = tracker.peers.write().await;
            if let Some(peer) = peers.get_mut("http://a:8080") {
                peer.mark_healthy();
            }
        }

        let healthy = tracker.healthy_peers().await;
        assert_eq!(healthy, vec!["http://a:8080"]);
    }

    #[tokio::test]
    async fn test_health_state() {
        let state = HealthState::new("node1".to_string());

        assert_eq!(*state.epoch.read().await, 0);
        assert!(!*state.is_leader.read().await);

        state.set_epoch(5).await;
        state.set_is_leader(true).await;

        assert_eq!(*state.epoch.read().await, 5);
        assert!(*state.is_leader.read().await);
    }
}
