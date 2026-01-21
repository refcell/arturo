# arturo

Minimal sequencer consensus built on commonware primitives.

## Installation

```toml
arturo = "0.1"
```

## Why

Optimism's `op-conductor` provides high-availability sequencer consensus using Raft. It works, but Raft carries complexity that isn't always necessary for the core problem: ordering and replicating payloads across a small cluster with a single leader.

arturo strips this down to the essentials. Instead of Raft's leader election and log replication, it uses commonware's `ordered_broadcast` which provides sequencer-driven broadcast with quorum-based certification. The result is a trait-abstracted library where payload types, epoch management, and cryptographic schemes are all pluggable rather than hardcoded.

The core insight is that op-conductor's `CommitUnsafePayload` and `LatestUnsafePayload` map directly to ordered_broadcast's propose/certify model. A single sequencer per epoch proposes chunks, validators acknowledge them, and once a quorum is reached the payload is certified. Leadership transfer becomes an epoch transition with a new sequencer identity.

### Why "arturo"

The name honors [Arturo Toscanini](https://en.wikipedia.org/wiki/Arturo_Toscanini), the legendary conductor who revolutionized orchestral performance in the early 20th century. Toscanini was famous for rejecting the excessive ornamentation of the Romantic era in favor of fidelity to the score itself. Where his contemporaries added layers of personal interpretation, Toscanini stripped performances back to what the composer actually wrote, insisting that the music speak for itself.

This project takes the same approach to sequencer consensus. Where `op-conductor` layers Raft on top of the core problem, arturo returns to fundamentals: ordered broadcast with quorum certification. The name felt right for a new kind of conductor built on the principle that less machinery means clearer execution.

## Usage

Define your payload type by implementing the `Payload` trait. This requires a `digest()` method returning something that implements commonware's `Digest`, and a `height()` method for ordering.

```rust,ignore
use arturo::Payload;

struct MyPayload {
    hash: [u8; 32],
    height: u64,
    data: Vec<u8>,
}

impl Payload for MyPayload {
    type Digest = [u8; 32];
    fn digest(&self) -> [u8; 32] { self.hash }
    fn height(&self) -> u64 { self.height }
}
```

Provide an `EpochManager` implementation that controls leader election. This can be as simple as a static configuration or as complex as an external coordination service.

```rust,ignore
use arturo::{EpochManager, Epoch, EpochChange};

struct StaticEpochManager { /* ... */ }

impl EpochManager for StaticEpochManager {
    type PublicKey = ed25519::PublicKey;
    fn current_epoch(&self) -> Epoch { /* ... */ }
    fn sequencer(&self, epoch: Epoch) -> Option<Self::PublicKey> { /* ... */ }
    fn is_sequencer(&self, key: &Self::PublicKey) -> bool { /* ... */ }
    // ...
}
```

Wire everything together with `Conductor`, which is generic over your payload type, epoch manager, and cryptographic scheme.

```rust,ignore
use arturo::Conductor;
use commonware_cryptography::ed25519::Scheme as Ed25519;

let conductor: Conductor<MyPayload, StaticEpochManager, Ed25519> =
    Conductor::new(config);

// Check leadership
if conductor.leader() {
    conductor.commit(payload).await?;
}

// Query latest certified payload
let latest = conductor.latest().await;
```

## Examples

See [`examples/`](examples/) for runnable code:

- **basic** — minimal conductor setup with ed25519
- **custom_payload** — implementing a custom payload type
- **static_sequencer** — static validator set configuration

Run with `cargo run --example <name>`.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.

## Acknowledgments

This project is built entirely on [commonware](https://commonware.xyz) primitives. The `ordered_broadcast` module from [commonware-consensus](https://docs.rs/commonware-consensus) provides the sequencer/validator broadcast protocol, [commonware-cryptography](https://docs.rs/commonware-cryptography) supplies pluggable signature schemes including ed25519 and BLS, and [commonware-codec](https://docs.rs/commonware-codec) handles serialization. These low-level building blocks make it possible to construct consensus systems without reinventing the fundamentals. The commonware team has done exceptional work creating composable, well-documented infrastructure for distributed systems.