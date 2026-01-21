//! Minimal conductor binary using the arturo library.
#![allow(unreachable_pub, dead_code, clippy::missing_const_for_fn, clippy::option_if_let_else)]
//!
//! This binary implements a minimal sequencer consensus conductor with:
//! - HTTP health-based leader election
//! - JSON-RPC interface for payload submission and retrieval
//! - Pluggable epoch management
//!
//! ## Usage
//!
//! ```bash
//! # Start with default settings
//! conductor --identity 1
//!
//! # Start with peers
//! conductor --identity 1 --peers http://peer1:8080,http://peer2:8080
//!
//! # Start with config file
//! conductor --config config.toml
//! ```

mod config;
mod epoch;
mod health;
mod payload;
mod rpc;

use std::time::Duration;

use arturo::{Conductor, ConductorConfig};
use commonware_cryptography::{Signer as _, ed25519};
use futures::StreamExt;
use tokio::signal;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::{
    config::Config, epoch::HealthBasedEpochManager, health::HealthState, payload::OpPayload,
    rpc::create_router,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    let config = Config::load()?;
    info!(?config, "loaded configuration");

    // Create ed25519 signer from identity seed
    let signer = ed25519::PrivateKey::from_seed(config.identity);
    let public_key = signer.public_key();
    info!(identity = %hex::encode(public_key.as_ref()), "initialized signer");

    // Create peer keys (for now, derive from sequential seeds)
    // In production, these would be configured or discovered
    let peer_keys: Vec<ed25519::PublicKey> = (1..=config.peers.len())
        .map(|i| {
            // Skip our own identity seed
            let seed = if i as u64 >= config.identity { i as u64 + 1 } else { i as u64 };
            ed25519::PrivateKey::from_seed(seed).public_key()
        })
        .collect();

    // Create self URL from bind address
    let self_url = format!("http://{}", config.bind_addr);

    // Create epoch manager
    let epoch_manager = HealthBasedEpochManager::new(
        self_url.clone(),
        config.peers.clone(),
        public_key.clone(),
        peer_keys,
        Duration::from_millis(config.health_interval_ms),
        config.quorum_threshold,
    );

    // Create conductor
    let conductor_config = ConductorConfig { quorum_threshold: config.quorum_threshold };
    let conductor: Conductor<OpPayload, HealthBasedEpochManager, ed25519::PrivateKey> =
        Conductor::new(conductor_config, epoch_manager.clone(), signer);

    // Start the conductor
    conductor.start().await;

    // Create health state
    let health_state = HealthState::new(hex::encode(public_key.as_ref()));

    // Spawn health polling task
    let health_interval = Duration::from_millis(config.health_interval_ms);
    let _health_handle = epoch_manager.clone().spawn_health_poller(health_interval);

    // Spawn epoch change listener
    let conductor_clone = conductor.clone();
    let health_state_clone = health_state.clone();
    let _epoch_handle = tokio::spawn(async move {
        let mut stream = conductor_clone.leader_channel();
        while let Some(change) = stream.next().await {
            info!(epoch = change.epoch, is_self = change.is_self, "epoch change received");
            conductor_clone.handle_epoch_change(change.clone()).await;
            health_state_clone.set_epoch(change.epoch).await;
            health_state_clone.set_is_leader(change.is_self).await;
        }
    });

    // Create router
    let router = create_router(conductor.clone(), health_state);

    // Start HTTP server
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    info!(addr = %config.bind_addr, "starting HTTP server");

    // Serve with graceful shutdown
    axum::serve(listener, router).with_graceful_shutdown(shutdown_signal()).await?;

    // Stop conductor
    conductor.stop().await;
    info!("conductor stopped");

    Ok(())
}

/// Waits for SIGINT or SIGTERM for graceful shutdown.
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {
            info!("received Ctrl+C, shutting down");
        }
        () = terminate => {
            info!("received SIGTERM, shutting down");
        }
    }
}
