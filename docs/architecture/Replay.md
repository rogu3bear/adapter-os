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
aos replay record --out var/replays/run_001.aosreplay \
  --adapter tenant-a/ml/inference/r001 -- \
  aosctl infer "Hello, world"
```

Records inference execution with:
- Adapter reference (from AOS archive)
- Global seed (derived from adapter manifest hash via HKDF)
- All execution events
- RNG state checkpoints
- Output hashes for verification

The referenced adapter must exist in the registry; its weights are loaded during recording but not stored in the replay bundle.

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

## Adapter Archives

Replay bundles capture state during execution of adapters loaded from AOS archives (`.aos` files). The unified AOS archive format uses a 64-byte header with offsets to manifest and weight sections:

```
Offset 0-3:    Magic bytes "AOS\x00"
Offset 4-7:    Flags (u32 LE, reserved)
Offset 8-15:   Weights offset (u64 LE)
Offset 16-23:  Weights size (u64 LE)
Offset 24-31:  Manifest offset (u64 LE)
Offset 32-39:  Manifest size (u64 LE)
Offset 40-63:  Reserved (padding to 64 bytes)
Offset 64+:    Weights (SafeTensors or Q15 quantized)
              Manifest (JSON metadata)
```

See [docs/AOS_FORMAT.md](../AOS_FORMAT.md) for complete specification. Replay bundles are distinct from AOS archives and contain execution state, not adapter weights.

## Deterministic Execution

Replay uses `DeterministicExecutor` from `adapteros-deterministic-exec`:

- Serial task execution (FIFO order)
- Logical tick counter (not wall-clock time)
- HKDF-seeded RNG (reproducible randomness)
- Event log recording

## Replay Bundle Recording

When recording a replay session:

1. **Adapter Loading**: Inference loads adapter from AOS archive (not stored in replay)
2. **Seed Initialization**: Global seed derived from adapter manifest hash via HKDF
3. **Execution Capture**: All deterministic operations logged (no non-deterministic operations allowed)
4. **State Snapshots**: RNG checkpoints recorded at key points
5. **Bundle Packaging**: Events serialized to `.aosreplay` JSON archive with signatures

Recorded adapters are referenced by ID in replay metadata. The actual adapter weights must be available at replay time; the bundle captures only the inference execution trace.

## Integration

Replay infrastructure integrates with:

- **Adapters**: References loaded adapters via ID from AOS archives
- **Telemetry**: Event capture during execution
- **Traces**: Span collection for distributed tracing
- **Verification**: Hash comparison for validation
- **Deterministic Executor**: Serial task execution with HKDF seeding

## Storage

Replay bundles are stored under `var/replays/` by default. Configure via:

- `AOS_REPLAY_DIR=var/replays` - Bundle storage directory
- Retention policy: Manual cleanup (automatic eviction planned)

Bundles reference adapters by ID; the actual `.aos` archives remain in the adapter registry. Replays are portable across systems provided referenced adapters are available.

## Verification

Each replay verifies:

1. **Event Ordering**: Events execute in recorded sequence
2. **Hash Consistency**: Intermediate hashes match stored values
3. **RNG State**: Random number generation reproduces identically
4. **Output Determinism**: Final outputs are byte-for-byte identical

Failures indicate non-deterministic behavior requiring investigation.

