# Deterministic Replay Architecture

## Overview

AdapterOS supports deterministic replay of inference runs for verification and debugging. Replay bundles capture execution state and can be replayed identically multiple times.

## Replay Bundle Format

Replay bundles (`.aosreplay`) are versioned JSON archives containing:

- **Header**: Version, seed, metadata, timestamp
- **Event Log**: Sequenced execution events with hashes
- **RNG Checkpoints**: Deterministic RNG state snapshots
- **Signature**: BLAKE3 hash and optional Ed25519 signature for integrity

## CLI Commands

### Record Execution

```bash
aos replay record --out var/replays/run_001.aosreplay -- \
  aosctl infer "Hello, world"
```

Records command execution with:
- Global seed (derived from command hash)
- All execution events
- RNG state checkpoints
- Output hashes for verification

### Run Replay

```bash
aos replay run --in var/replays/run_001.aosreplay
```

Executes the replay bundle using the deterministic executor:
- Uses recorded seed for RNG initialization
- Replays events in exact order
- Verifies intermediate hashes match

### Inspect Bundle

```bash
aos replay inspect --in var/replays/run_001.aosreplay
```

Displays bundle metadata:
- Event count
- Seed and IDs
- Time range
- Schema version

### Verify Determinism

```bash
aos replay verify --in var/replays/run_001.aosreplay --runs 10
```

Runs the bundle multiple times and verifies:
- All runs produce identical output hashes
- Event ordering is consistent
- No divergence detected

Returns non-zero exit code if determinism fails.

## Deterministic Execution

Replay uses `DeterministicExecutor` from `adapteros-deterministic-exec`:

- Serial task execution (FIFO order)
- Logical tick counter (not wall-clock time)
- HKDF-seeded RNG (reproducible randomness)
- Event log recording

## Integration

Replay infrastructure integrates with:

- **Telemetry**: Event capture during execution
- **Traces**: Span collection for distributed tracing
- **Verification**: Hash comparison for validation

## Storage

Replay bundles are stored under `var/replays/` by default. Configure via:

- `AOS_REPLAY_DIR=var/replays` - Bundle storage directory
- Retention policy: Manual cleanup (automatic eviction planned)

## Verification

Each replay verifies:

1. **Event Ordering**: Events execute in recorded sequence
2. **Hash Consistency**: Intermediate hashes match stored values
3. **RNG State**: Random number generation reproduces identically
4. **Output Determinism**: Final outputs are byte-for-byte identical

Failures indicate non-deterministic behavior requiring investigation.

