# Policy Hash Watcher

The Policy Hash Watcher detects runtime policy pack mutations and triggers quarantine to enforce determinism guarantees.

## Overview

Per **Determinism Ruleset #2**: "refuse to serve if policy hashes don't match"

The watcher implements a hybrid persistence model:
- **Baseline hashes** stored in database (persistent, audit trail)
- **Runtime cache** for O(1) validation during hot path
- **In-memory delta buffer** for violation tracking

## Components

### PolicyHashWatcher (`hash_watcher.rs`)

Core functionality:
- `register_baseline()` - Store BLAKE3 hash for policy pack
- `validate()` - Check current hash against baseline
- `start_background_watcher()` - Periodic validation loop
- `get_violations()` - Retrieve detected violations

### QuarantineManager (`quarantine.rs`)

Enforces strict quarantine when violations detected:

| Operation | During Quarantine |
|-----------|-------------------|
| Inference | DENIED |
| Adapter Load | DENIED |
| Adapter Swap | DENIED |
| Training | DENIED |
| Audit (read-only) | ALLOWED |
| Status (read-only) | ALLOWED |
| Metrics (read-only) | ALLOWED |

## Database Schema

See `migrations/0037_policy_hashes.sql`:

```sql
CREATE TABLE policy_hashes (
    id TEXT PRIMARY KEY,
    policy_pack_id TEXT NOT NULL,
    baseline_hash TEXT NOT NULL,  -- BLAKE3 hex
    cpid TEXT,                     -- Control Plane ID
    signer_pubkey TEXT,            -- Ed25519 public key
    created_at TEXT NOT NULL
);
```

## Usage

```rust
use adapteros_policy::PolicyHashWatcher;

let watcher = PolicyHashWatcher::new(db, telemetry, cpid);

// Register baseline at startup
watcher.register_baseline("isolation-pack", &hash, Some(&pubkey)).await?;

// Validate during request processing
let result = watcher.validate("isolation-pack", &current_hash).await?;
if !result.valid {
    // Quarantine triggered
}
```

## CLI Commands

```bash
# List registered policy hashes
aosctl policy list --hashes

# Validate policy integrity
aosctl policy validate

# Clear quarantine (requires operator confirmation)
aosctl policy quarantine clear --confirm
```

## Telemetry Events

Validation events are logged via `PolicyHashValidationEvent`:
- `policy_pack_id`
- `status` (valid/mismatch/missing_baseline)
- `baseline_hash`
- `current_hash`
- `cpid`
