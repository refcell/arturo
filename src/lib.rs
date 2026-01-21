#![doc = include_str!("../README.md")]
#![doc(issue_tracker_base_url = "https://github.com/refcell/arturo/issues/")]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(not(any(test, feature = "bin")), warn(unused_crate_dependencies))]

// Binary-only dependencies (used by bin/main.rs, not lib)
// Re-export commonly used commonware types
pub use commonware_consensus::Automaton;
pub use commonware_cryptography::{Digest, Signer};
#[cfg(feature = "bin")]
use {
    alloy_primitives as _, alloy_rpc_types_engine as _, axum as _, clap as _, hex as _,
    op_alloy_rpc_types_engine as _, reqwest as _, serde as _, serde_json as _, toml as _,
    tracing_subscriber as _,
};

mod automaton;
pub use automaton::{PayloadAutomaton, PayloadContext};

mod conductor;
pub use conductor::{Conductor, ConductorConfig};

mod providers;
pub use providers::{EpochSequencersProvider, StaticSequencersProvider, ValidatorsProvider};

mod traits;
pub use traits::{EpochManager, EpochStream, Payload, PayloadStore, StoreError};

mod types;
pub use types::{ConductorError, Epoch, EpochChange, Height, PendingPayload, TransferError};
