# op-conductor

Minimal OP Stack conductor binary using the arturo library.

## Overview

This binary implements a minimal op-conductor with:
- HTTP health-based leader election
- JSON-RPC interface for payload submission and retrieval
- Pluggable epoch management via the arturo `EpochManager` trait

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      op-conductor                            │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─────────────┐     ┌──────────────────────────────────┐   │
│  │   Config    │────▶│         HealthBasedEpochManager   │   │
│  │   (TOML)   │     │  - Polls peer /health endpoints   │   │
│  └─────────────┘     │  - Leader = first healthy peer   │   │
│                      │  - Epoch increments on change    │   │
│                      └──────────────┬───────────────────┘   │
│                                     │                        │
│  ┌─────────────┐     ┌──────────────▼───────────────────┐   │
│  │  ed25519    │────▶│           Conductor               │   │
│  │  Signer    │     │  - Payload validation             │   │
│  └─────────────┘     │  - Certification (quorum acks)   │   │
│                      │  - Height tracking               │   │
│                      └──────────────┬───────────────────┘   │
│                                     │                        │
│                      ┌──────────────▼───────────────────┐   │
│                      │          Axum Router              │   │
│                      │  GET  /health                     │   │
│                      │  GET  /leader                     │   │
│                      │  POST /commit                     │   │
│                      │  POST /acknowledge                │   │
│                      │  GET  /latest                     │   │
│                      │  GET  /payload/:height            │   │
│                      └──────────────────────────────────┘   │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## Usage

### Start with default settings

```bash
cargo run --bin op-conductor -- --identity 1
```

### Start with peers

```bash
cargo run --bin op-conductor -- \
  --identity 1 \
  --peers http://peer1:8080,http://peer2:8080 \
  --bind-addr 0.0.0.0:8080
```

### Start with config file

```bash
cargo run --bin op-conductor -- --config config.toml
```

### Example config.toml

```toml
bind_addr = "0.0.0.0:8080"
identity = 1
peers = ["http://peer1:8080", "http://peer2:8080"]
health_interval_ms = 1000
quorum_threshold = 2
```

## Configuration Options

| Option | Environment Variable | Default | Description |
|--------|---------------------|---------|-------------|
| `--config` | `OP_CONDUCTOR_CONFIG` | - | Path to TOML config file |
| `--bind-addr` | `OP_CONDUCTOR_BIND_ADDR` | `127.0.0.1:8080` | HTTP server bind address |
| `--identity` | `OP_CONDUCTOR_IDENTITY` | - | Node identity seed for key derivation |
| `--peers` | `OP_CONDUCTOR_PEERS` | - | Comma-separated list of peer URLs |
| `--health-interval-ms` | `OP_CONDUCTOR_HEALTH_INTERVAL_MS` | `1000` | Health check interval in ms |
| `--quorum-threshold` | `OP_CONDUCTOR_QUORUM_THRESHOLD` | `1` | Required acks for certification |

## API Endpoints

### `GET /health`

Returns the health status of this node.

```json
{
  "healthy": true,
  "identity": "abc123...",
  "epoch": 5,
  "is_leader": true
}
```

### `GET /leader`

Returns the current leader status.

```json
{
  "is_leader": true,
  "epoch": 5,
  "next_height": 100
}
```

### `POST /commit`

Submit a payload for certification (sequencer only).

Request:
```json
{
  "payload": { ... }
}
```

Response:
```json
{
  "success": true
}
```

### `POST /acknowledge`

Record an acknowledgment for the pending payload.

Response:
```json
{
  "certified": true,
  "height": 100
}
```

### `GET /latest`

Returns the latest certified payload.

### `GET /payload/:height`

Returns the certified payload at a specific height.

## Leader Election Tradeoffs

### Static Configuration (simplest)
- No network overhead
- Manual failover required
- Best for single-node deployments or controlled environments
- Implementation: Configure a fixed leader in config

### HTTP Health-Based (this implementation)
- Automatic failover when leader becomes unhealthy
- Requires health endpoints on all nodes
- Leader determined by sorted order of healthy peers (deterministic)
- No external dependencies
- Trade-off: Health check latency affects failover time (configurable via `health_interval_ms`)

### etcd/Consul (production-grade)
- Battle-tested distributed consensus
- Strong consistency guarantees
- Adds external infrastructure dependency
- Better for large-scale deployments
- Implementation: Replace `HealthBasedEpochManager` with `EtcdEpochManager`

## Example: Multi-Node Setup

Terminal 1 (Node 1 - will be leader):
```bash
cargo run --bin op-conductor -- \
  --identity 1 \
  --bind-addr 127.0.0.1:8081 \
  --peers http://127.0.0.1:8082
```

Terminal 2 (Node 2):
```bash
cargo run --bin op-conductor -- \
  --identity 2 \
  --bind-addr 127.0.0.1:8082 \
  --peers http://127.0.0.1:8081
```

Check leader status:
```bash
curl http://127.0.0.1:8081/leader
curl http://127.0.0.1:8082/leader
```

## Graceful Degradation

If all peers become unhealthy:
1. Only the local node remains in the candidate list
2. The local node becomes the leader
3. Epoch increments to reflect the change
4. Operations continue with single-node quorum (if threshold allows)

This ensures the system remains available even during network partitions.
