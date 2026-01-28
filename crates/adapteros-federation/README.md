# adapteros-federation

Cross-host federation signatures for telemetry bundles, enabling deterministic replay verification across multiple hosts.

## Features

- **Ed25519 Signatures**: Cryptographically signed bundle metadata for cross-host verification
- **Chain Validation**: Verify Merkle chain continuity across hosts with `prev_host_hash` linkage
- **Database Storage**: Persistent signature storage with verification status tracking
- **Telemetry Integration**: 100% sampling for federation events per Telemetry Ruleset #9
- **Secure Enclave Support**: Optional hardware-backed signing (future enhancement)

## Policy Compliance

This crate implements and enforces the following policy packs:

- **Determinism Ruleset (#2)**: Reproducible signature chains with HKDF-seeded RNG
- **Isolation Ruleset (#8)**: Per-tenant signature isolation
- **Telemetry Ruleset (#9)**: Signed bundle rotation with canonical JSON
- **Artifacts Ruleset (#13)**: Signature verification gates

## Public API

### FederationManager (8 Methods)

**Core Operations:**
1. `new(db, keypair)` - Create manager with Ed25519 keypair
2. `with_telemetry(db, keypair, telemetry)` - Create with telemetry writer
3. `with_host_id(db, keypair, host_id)` - Create with custom host ID (testing)

**Signing & Verification:**
4. `sign_bundle(metadata)` - Sign a telemetry bundle
5. `verify_signature(signature, public_key, metadata)` - Verify single signature
6. `verify_cross_host_chain(host_chain)` - Verify chain continuity

**Database Operations:**
7. `get_signatures_for_bundle(bundle_hash)` - Retrieve all signatures for a bundle
8. `get_host_chain(host_id, limit)` - Retrieve signature chain for a host
9. `mark_verified(signature_id)` - Mark signature as verified

### Public Types (2 Types)

1. `FederationManager` - Main federation coordinator
2. `FederationSignature` - Signature record structure

## Usage

### Basic Bundle Signing

```rust
use adapteros_federation::FederationManager;
use adapteros_crypto::Keypair;
use adapteros_db::Db;
use adapteros_telemetry::StoredBundleMetadata;

#[tokio::main]
async fn main() -> Result<()> {
    // Connect to database
    let db = Db::connect("var/cp.db").await?;
    db.migrate().await?;
    
    // Create federation manager
    let keypair = Keypair::generate();
    let manager = FederationManager::new(db, keypair)?;
    
    // Sign a bundle
    let metadata = load_bundle_metadata()?;
    let signature = manager.sign_bundle(&metadata).await?;
    
    println!("Bundle signed: {}", signature.bundle_hash);
    
    Ok(())
}
```

### Cross-Host Chain Verification

```rust
use adapteros_federation::FederationManager;

#[tokio::main]
async fn main() -> Result<()> {
    let db = Db::connect("var/cp.db").await?;
    let keypair = Keypair::generate();
    let manager = FederationManager::new(db, keypair)?;
    
    // Get signatures for a bundle
    let signatures = manager
        .get_signatures_for_bundle("bundle_hash")
        .await?;
    
    // Verify cross-host chain
    manager.verify_cross_host_chain(&signatures).await?;
    
    println!("Cross-host chain verified!");
    
    Ok(())
}
```

### With Telemetry

```rust
use adapteros_federation::FederationManager;
use adapteros_telemetry::TelemetryWriter;

#[tokio::main]
async fn main() -> Result<()> {
    let db = Db::connect("var/cp.db").await?;
    let keypair = Keypair::generate();
    
    // Create telemetry writer
    let telemetry = TelemetryWriter::new("var/telemetry", 500_000, 256 * 1024 * 1024)?;
    
    // Create manager with telemetry
    let manager = FederationManager::with_telemetry(db, keypair, telemetry)?;
    
    // Sign bundle (emits telemetry events)
    let signature = manager.sign_bundle(&metadata).await?;
    
    Ok(())
}
```

## Database Schema

### `federation_bundle_signatures` Table

```sql
CREATE TABLE federation_bundle_signatures (
    id TEXT PRIMARY KEY,
    host_id TEXT NOT NULL,
    bundle_hash TEXT NOT NULL,
    signature TEXT NOT NULL,
    prev_host_hash TEXT,
    created_at TEXT NOT NULL,
    verified INTEGER NOT NULL DEFAULT 0
);
```

**Indexes:**
- `idx_federation_bundle_hash` - Fast bundle lookup
- `idx_federation_host_created` - Host chain queries
- `idx_federation_verified` - Verification status filtering

## Telemetry Events

Federation operations emit the following telemetry events (100% sampling):

### `federation.bundle_signed`

Emitted when a bundle is signed:

```json
{
  "event_type": "federation.bundle_signed",
  "level": "Info",
  "message": "Federation bundle signed: <bundle_hash>",
  "component": "adapteros-federation",
  "metadata": {
    "host_id": "hostname",
    "bundle_hash": "b3:...",
    "signature": "ed25519_sig...",
    "prev_bundle_hash": "b3:..."
  }
}
```

### `federation.chain_verified`

Emitted when a cross-host chain is successfully verified:

```json
{
  "event_type": "federation.chain_verified",
  "level": "Info",
  "message": "Federation chain verified: <N> signatures",
  "component": "adapteros-federation",
  "metadata": {
    "chain_length": 5,
    "first_host": "host1",
    "last_host": "host5",
    "hosts": ["host1", "host2", "host3", "host4", "host5"]
  }
}
```

### `federation.chain_break`

Emitted when a chain break is detected (ERROR level):

```json
{
  "event_type": "federation.chain_break",
  "level": "Error",
  "message": "Federation chain break: <prev_host> -> <curr_host>",
  "component": "adapteros-federation",
  "metadata": {
    "prev_host": "host1",
    "curr_host": "host2",
    "expected_prev_hash": "b3:...",
    "actual_prev_hash": "b3:..."
  }
}
```

## CLI Integration

The `aosctl` CLI provides federation verification commands:

```bash
# Verify cross-host federation signatures
aosctl federation-verify --bundle-dir var/telemetry

# Verify with custom database
aosctl federation-verify --bundle-dir var/telemetry --database var/cp.db

# JSON output
aosctl federation-verify --bundle-dir var/telemetry --json > federation.json
```

## Integration with adapteros-verify

Federation verification is integrated into the golden-run verification system:

```rust
use adapteros_verify::verify_cross_host;

#[tokio::main]
async fn main() -> Result<()> {
    let bundle_dir = Path::new("var/telemetry");
    let db = Db::connect("var/cp.db").await?;
    
    // Verify cross-host chain
    verify_cross_host(bundle_dir, &db).await?;
    
    Ok(())
}
```

## Testing

The crate includes comprehensive test coverage:

### Unit Tests

Run unit tests with:

```bash
cargo test --package adapteros-federation
```

### Integration Tests

Run integration tests with:

```bash
# Federation chain validation
cargo test --test federation_chain

# Cross-host replay verification
cargo test --test cross_host_replay
```

## Architecture

```
┌─────────────────────────────────────────────────────┐
│              FederationManager                      │
├─────────────────────────────────────────────────────┤
│  • sign_bundle()                                    │
│  • verify_cross_host_chain()                        │
│  • get_signatures_for_bundle()                      │
└──────────┬──────────────────────────────────────────┘
           │
           ├──> Ed25519 Signatures (adapteros-crypto)
           │
           ├──> Database Storage (adapteros-db)
           │
           ├──> Telemetry Events (adapteros-telemetry)
           │
           └──> Bundle Metadata (adapteros-telemetry)
```

## Chain Verification Algorithm

1. **Empty Chain**: Valid (no signatures to verify)
2. **Single Signature**: Valid (no linkage to check)
3. **Multiple Signatures**: For each adjacent pair `(prev, curr)`:
   - Verify `curr.prev_host_hash == prev.bundle_hash`
   - Verify `curr.created_at >= prev.created_at`
   - Emit telemetry event on break detection

## Error Handling

The federation manager returns `Result<T>` using `adapteros_core::AosError`:

```rust
pub enum AosError {
    Validation(String),  // Chain breaks, timestamp violations
    Database(String),    // Storage failures
    Crypto(String),      // Signature verification failures
    Io(String),          // Hostname lookup, etc.
    Serialization(String), // JSON encoding errors
}
```

## Dependencies

- `adapteros-core` - Error types and B3Hash
- `adapteros-crypto` - Ed25519 keypairs and signatures
- `adapteros-db` - Database storage and migrations
- `adapteros-telemetry` - Event logging and bundle metadata
- `adapteros-replay` - Replay verification integration

## Automated Federation Daemon

The federation system includes a continuous verification daemon that runs periodic sweeps:

```rust
use adapteros_orchestrator::{FederationDaemon, FederationDaemonConfig};
use adapteros_federation::FederationManager;
use adapteros_policy::PolicyHashWatcher;

#[tokio::main]
async fn main() -> Result<()> {
    let db = Db::connect("var/cp.db").await?;
    let keypair = Keypair::generate();
    
    let federation = FederationManager::new(db.clone(), keypair)?;
    let policy_watcher = PolicyHashWatcher::new(
        Arc::new(db.clone()),
        Arc::new(telemetry),
        Some("cpid-001".to_string()),
    );
    
    let config = FederationDaemonConfig {
        interval_secs: 300, // 5 minutes
        max_hosts_per_sweep: 10,
        enable_quarantine: true,
        quorum_min_peers: 2,
    };
    
    let daemon = FederationDaemon::new(
        Arc::new(federation),
        Arc::new(policy_watcher),
        Arc::new(telemetry),
        Arc::new(db),
        config,
    );
    
    // Start daemon in background
    let handle = Arc::new(daemon).start();
    
    // Daemon will run periodic verification and trigger quarantine on failures
    handle.await?;
    
    Ok(())
}
```

## Secure Enclave Integration

Federation bundles can be signed with hardware-backed keys:

```rust
use adapteros_federation::attestation::{attest_bundle, AttestationInfo};

// Sign with Secure Enclave (macOS only)
let payload = b"federation bundle data";
let (signature, attestation) = attest_bundle(payload)?;

assert!(attestation.hardware_backed);
println!("Signed with enclave: {}", attestation.enclave_id.unwrap());
```

## Tick Ledger Integration

Federation signatures are linked to the deterministic tick ledger for replay validation:

```rust
use adapteros_deterministic_exec::global_ledger::{
    GlobalTickLedger, FederationMetadata
};

let ledger = GlobalTickLedger::new(db, "tenant-001".to_string(), "host-001".to_string());

// Commit tick with federation metadata
let metadata = FederationMetadata {
    bundle_hash: Some("b3:abc123...".to_string()),
    prev_host_hash: Some("b3:def456...".to_string()),
    signature: Some("ed25519_sig...".to_string()),
};

ledger.commit_tick_with_federation_meta(42, metadata).await?;
```

## Future Enhancements

- **Key Rotation**: Automatic key rotation at CP promotion
- **Multi-Signature Thresholds**: Require N-of-M signatures for promotion
- **Cross-Tenant Federation**: Federated verification across tenant boundaries
- **Signature Aggregation**: BLS signatures for efficient multi-signature verification

## See Also

- [adapteros-verify](../adapteros-verify) - Golden-run verification system
- [adapteros-telemetry](../adapteros-telemetry) - Telemetry bundle system
- [adapteros-crypto](../adapteros-crypto) - Cryptographic primitives
- [adapteros-replay](../adapteros-replay) - Determinism replay verification

## License

MIT OR Apache-2.0
