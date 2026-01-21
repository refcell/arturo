# demo

This demo visualizes arturo's sequencer consensus in action with multiple participants and a terminal-based UI. The demo creates N participants (default 3) that run conductors with round-robin leader election and displays their state in real-time.

The demo consists of three main components that work together to simulate a consensus network. Participants are conductor instances that wrap the arturo library with ed25519 signing and a round-robin epoch manager. Each participant maintains its own view of the current epoch, whether it is the leader, and the certified payload height. The epoch manager rotates leadership deterministically based on `epoch % participant_count`.

The sidecar service drives the consensus process by generating payloads and triggering acknowledgments. On a configurable interval (default 2 seconds), the sidecar finds the current leader, creates a new payload with the next expected height, commits it through the leader's conductor, and then calls `acknowledge()` on all participants to simulate validator responses. After a configurable number of commits (default 3), the sidecar advances to the next epoch, rotating leadership to the next participant in the round-robin order.

The TUI renders each participant as a vertical panel showing its ID, current role (LEADER in green or Validator in gray), epoch number, next expected height, and count of certified payloads. The display updates every 100ms to reflect state changes. Press `q`, `Esc`, or `Ctrl+C` to exit the demo.

Run the demo with `just demo` or `cargo run --features demo --bin demo`. Command-line options include `--participants N` to set the number of participants, `--interval-ms` to control the commit interval, and `--commits-per-epoch` to adjust how often leadership rotates. Enable verbose logging with `just demo-verbose` or by setting `RUST_LOG=debug`.

The code is organized in `demo/src/` with `main.rs` as the entry point that wires everything together. The `payload.rs` module defines `DemoPayload` implementing the arturo `Payload` trait with height, timestamp, and data fields. The `epoch.rs` module provides `RoundRobinEpochManager` implementing the arturo `EpochManager` trait with deterministic rotation. The `participant.rs` module wraps `Conductor` with convenience methods for the demo. The `sidecar.rs` module contains the background task that generates payloads and advances epochs. The `tui/` subdirectory contains the ratatui-based terminal UI split into `app.rs` for state, `view.rs` for rendering, `input.rs` for keyboard handling, and `mod.rs` for the main loop.

This demo illustrates how arturo's trait-based design allows plugging in different epoch managers while maintaining the same core consensus logic. The round-robin manager here is intentionally simple for demonstration purposes. Production deployments would use health-based election, external coordination services, or other strategies as shown in the conductor binary.
