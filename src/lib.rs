#![doc = include_str!("../README.md")]
#![doc(issue_tracker_base_url = "https://github.com/refcell/arturo/issues/")]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

// Re-export commonly used commonware types
pub use commonware_consensus::Automaton;
pub use commonware_cryptography::{Digest, Signer};

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
