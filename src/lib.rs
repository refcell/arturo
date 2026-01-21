//! Arturo: Minimal op-conductor rewrite using commonware ordered_broadcast.
//!
//! Arturo provides a trait-abstracted Rust library for unsafe payload ordering
//! and replication, mapping op-conductor's core consensus functionality to
//! commonware's ordered_broadcast primitives.
//!
//! # Design Principles
//!
//! - **Trait-abstracted**: All behavior is abstracted through traits
//! - **Generic payloads**: Not Optimism-specific, works with any payload type
//! - **Pluggable cryptography**: Uses commonware's cryptographic abstractions
//! - **Composable**: Building blocks, not a monolithic service
//!
//! # Core Components
//!
//! - [`Payload`]: Trait for user-defined payload types
//! - [`EpochManager`]: Trait for epoch/leader management
//! - [`PayloadAutomaton`]: Generic automaton implementing commonware's `Automaton`
//! - [`Conductor`]: Main orchestrator with `commit()`/`latest()`/`leader()`
//!
//! # Conceptual Mapping
//!
//! | op-conductor (Go/Raft) | arturo (Rust/ordered_broadcast) |
//! |------------------------|--------------------------------|
//! | Single Leader | Single Sequencer per epoch |
//! | Followers | Validators (sign/ack chunks) |
//! | `CommitUnsafePayload()` | `Conductor::commit()` |
//! | `LatestUnsafePayload()` | `Conductor::latest()` |
//! | Leader election | Epoch transition via `EpochManager` |
//! | Raft log replication | Chunk certification via quorum acks |
//! | `LeaderCh()` notification | `Conductor::leader_channel()` |
//!
//! # Example
//!
//! ```ignore
//! use arturo::{Conductor, ConductorConfig, EpochManager, Payload};
//!
//! // Define your payload type
//! #[derive(Clone)]
//! struct MyPayload {
//!     hash: [u8; 32],
//!     height: u64,
//!     data: Vec<u8>,
//! }
//!
//! impl Payload for MyPayload {
//!     type Digest = [u8; 32];
//!
//!     fn digest(&self) -> Self::Digest { self.hash }
//!     fn height(&self) -> u64 { self.height }
//!     fn encode(&self) -> Vec<u8> { /* ... */ }
//!     fn decode(bytes: &[u8]) -> Option<Self> { /* ... */ }
//! }
//!
//! // Implement your epoch manager
//! struct MyEpochManager { /* ... */ }
//! impl EpochManager for MyEpochManager { /* ... */ }
//!
//! // Create and use the conductor
//! let conductor: Conductor<MyPayload, MyEpochManager, MySigner> =
//!     Conductor::new(ConductorConfig::default(), epoch_manager, signer);
//!
//! // Start the conductor
//! conductor.start().await;
//!
//! // Commit payloads when acting as sequencer
//! if conductor.leader().await {
//!     conductor.commit(payload).await?;
//! }
//!
//! // Query the latest certified payload
//! let latest = conductor.latest().await;
//! ```

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![warn(unreachable_pub)]

pub mod automaton;
pub mod conductor;
pub mod providers;
pub mod traits;
pub mod types;

// Re-export main types for convenience
pub use automaton::{PayloadAutomaton, PayloadContext};
// Re-export commonly used commonware types
pub use commonware_consensus::Automaton;
pub use commonware_cryptography::{Digest, Signer};
pub use conductor::{Conductor, ConductorConfig};
pub use providers::{EpochSequencersProvider, StaticSequencersProvider, ValidatorsProvider};
pub use traits::{EpochManager, EpochStream, Payload, PayloadStore, StoreError};
pub use types::{ConductorError, Epoch, EpochChange, Height, PendingPayload, TransferError};
