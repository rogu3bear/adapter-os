# Kernel Hash Trace

This document traces the complete lifecycle of the Metal kernel hash from build time through runtime verification and attestation.

## Hash: `30f24dde3bbe7640c8696e18912c2a294359e6895e743001b72d693544dd4361`

## 1. Build Time Generation

### Location: `crates/adapteros-lora-kernel-mtl/build.rs`

**Step 1: Compile Metal Shaders**
- Metal shaders are compiled to `adapteros_kernels.metallib` by the build script
- The compiled binary is read from the build directory

**Step 2: Compute BLAKE3 Hash** (lines 343-347)
```rust
let metallib_bytes = std::fs::read(kernel_src_dir.join("adapteros_kernels.metallib"))
    .expect("Failed to read metallib");
let hash = blake3::hash(&metallib_bytes);
let hash_hex = hash.to_hex();
```

**Step 3: Emit Build Warning** (line 350)
```rust
println!("cargo:warning=Kernel hash: {}", hash_hex);
```
- This is the warning you see during `cargo build`
- Uses Cargo's `cargo:warning=` mechanism for informational output

**Step 4: Write Hash File** (line 351)
```rust
std::fs::write(shaders_dir.join("kernel_hash.txt"), hash_hex.as_str())
```
- Writes to: `crates/adapteros-lora-kernel-mtl/shaders/kernel_hash.txt`
- This file is embedded at compile time

**Step 5: Generate Signed Manifest** (lines 425-569)
- Creates `manifests/metallib_manifest.json` with:
  - `kernel_hash`: The BLAKE3 hash
  - Build metadata (xcrun_version, sdk_version, rust_version, build_timestamp)
- Signs the manifest with Ed25519 deterministic test key
- Writes signature to `manifests/metallib_manifest.json.sig`

## 2. Compile-Time Embedding

### Location: `crates/adapteros-lora-kernel-mtl/src/lib.rs`

**Embedded Constants** (lines 190-200)
```rust
#[cfg(target_os = "macos")]
const METALLIB_BYTES: &[u8] = include_bytes!("../shaders/adapteros_kernels.metallib");
#[cfg(target_os = "macos")]
const METALLIB_HASH: &str = include_str!("../shaders/kernel_hash.txt");
```

- `METALLIB_BYTES`: The compiled Metal library binary embedded in the Rust binary
- `METALLIB_HASH`: The hash string from `kernel_hash.txt` embedded as a string constant

**Manifest Files** (embedded via `include_str!` in `manifest.rs`)
- `manifests/metallib_manifest.json`: Build metadata including kernel_hash
- `manifests/metallib_manifest.json.sig`: Ed25519 signature of the manifest

## 3. Runtime Verification

### Location: `crates/adapteros-lora-kernel-mtl/src/lib.rs`

**Step 1: Constructor Verification** (line 278)
```rust
pub fn new() -> Result<Self> {
    let _manifest = verify_embedded_manifest(METALLIB_BYTES, None)?;
    // ...
}
```
- Verifies the embedded manifest signature on first instantiation
- Ensures kernel hash in manifest matches actual metallib hash

**Step 2: Library Load Verification** (lines 476-515)
```rust
fn load_library(&mut self) -> Result<()> {
    // Compute hash of embedded metallib
    let actual_hash = B3Hash::hash(METALLIB_BYTES);
    
    // Parse expected hash from embedded constant
    let expected_hash_str = METALLIB_HASH.trim();
    let expected_hash = B3Hash::from_hex(expected_hash_str)?;
    
    // Verify match
    if actual_hash != expected_hash {
        return Err(AosError::DeterminismViolation(...));
    }
    
    tracing::info!("Kernel hash verified: {}", actual_hash.to_short_hex());
}
```

**Verification Flow:**
1. Computes BLAKE3 hash of `METALLIB_BYTES` at runtime
2. Parses expected hash from `METALLIB_HASH` constant
3. Compares actual vs expected
4. Fails with `DeterminismViolation` if mismatch (unless dev bypass enabled)
5. Logs success: `"Kernel hash verified: <short-hex>"`

**Dev Bypass:**
- Set `AOS_DEV_SKIP_METALLIB_CHECK=1` to skip verification (debug builds only)
- Blocked in production mode (`AOS_SERVER_PRODUCTION_MODE=1`)

## 4. Manifest Verification

### Location: `crates/adapteros-lora-kernel-mtl/src/manifest.rs`

**Function: `verify_embedded_manifest()`** (lines 243-296)

**Step 1: Load Embedded Files**
```rust
let manifest_json = include_str!("../manifests/metallib_manifest.json");
let signature_data = include_str!("../manifests/metallib_manifest.json.sig");
```

**Step 2: Compute Actual Hash**
```rust
let actual_hash = B3Hash::hash(metallib_bytes);
```

**Step 3: Verify Signature** (if not in dev mode)
- Verifies Ed25519 signature of manifest JSON
- Ensures manifest hasn't been tampered with

**Step 4: Compare Kernel Hash**
- Extracts `kernel_hash` from verified manifest
- Compares with actual computed hash
- Logs warning if mismatch in dev mode

**Dev Bypass:**
- Set `AOS_SKIP_KERNEL_SIGNATURE_VERIFY=1` or `AOS_DEBUG_SKIP_KERNEL_SIG=1`
- Still compares kernel hash even when signature check is skipped

## 5. Determinism Attestation

### Location: `crates/adapteros-lora-kernel-mtl/src/lib.rs`

**Function: `attest_determinism()`** (lines 1526-1591)

**Step 1: Extract Hash from Constant**
```rust
let metallib_hash = adapteros_core::B3Hash::from_hex(crate::METALLIB_HASH.trim())?;
```

**Step 2: Verify Embedded Manifest**
```rust
let manifest_result = crate::verify_embedded_manifest(crate::METALLIB_BYTES, None);
```

**Step 3: Runtime Hash Verification**
```rust
let actual_hash = adapteros_core::B3Hash::hash(crate::METALLIB_BYTES);
let metallib_verified = actual_hash == metallib_hash;
```

**Step 4: Generate Attestation Report**
- Includes `kernel_hash` in `KernelManifest` attestation data
- Reports verification status (`metallib_verified`)
- Logs error if mismatch (but doesn't fail - reports in attestation)
- Used for determinism guarantees and audit trails

**Attestation Output:**
```rust
attestation::DeterminismReport {
    kernel_manifest: Some(KernelManifest {
        kernel_hash: "30f24dde3bbe7640c8696e18912c2a294359e6895e743001b72d693544dd4361",
        xcrun_version: "...",
        sdk_version: "...",
        rust_version: "...",
        build_timestamp: "...",
    }),
    metallib_verified: true,
    // ... other determinism metadata
}
```

## 6. Database Storage

### Location: `crates/adapteros-db/src/`

**Plans Table** (`migrations/0001_init.sql` line 85)
```sql
CREATE TABLE plans (
    ...
    kernel_hashes_json TEXT NOT NULL,
    ...
);
```

**Plan Metadata** (`crates/adapteros-db/src/models.rs` line 224)
```rust
pub struct Plan {
    pub kernel_hashes_json: String,  // JSON array of kernel hashes
    // ...
}
```

**Usage:**
- Plans store kernel hashes in `kernel_hashes_json` field
- Used to track which kernel version was used for a plan
- Enables plan rebuild detection when kernels change

**Plan Rebuild Detection** (`crates/adapteros-server-api/src/handlers/plans.rs` lines 277-296)
```rust
// Compare kernel hashes between old and new plans
let diff_summary = match (old_hash, new_hash) {
    (Some(old), Some(new)) if old != new => {
        "Metal kernels updated (hash changed)".to_string()
    }
    _ => "Plan rebuilt with current Metal kernels".to_string(),
};
```

## 7. API Responses

### Location: `crates/adapteros-server-api/src/handlers/`

**Plan Details Response** (`handlers/plans.rs` lines 183-198)
```rust
kernel_hash_b3: {
    match sqlx::query_scalar::<_, Option<String>>(
        "SELECT kernel_hash FROM plan_metadata WHERE plan_id = ?",
    )
    .bind(&plan.id)
    .fetch_optional(state.db.pool())
    .await
    {
        Ok(hash) => hash.flatten(),
        Err(e) => {
            tracing::warn!("Failed to fetch kernel hash for plan {}: {}", plan.id, e);
            None
        }
    }
}
```

**Replay Sessions** (`handlers/replay.rs` lines 61, 333, 427, 851)
- Replay sessions store `kernel_hash_b3` for determinism tracking
- Used to verify replay environment matches original execution

## 8. Orchestrator Gates

### Location: `crates/adapteros-orchestrator/src/gates/metallib.rs`

**Metallib Gate Verification** (lines 99-137)
- Verifies kernel hash matches expected value from manifest
- Used in pre-flight checks before worker execution
- Ensures worker has correct kernel version

```rust
let actual_hash = B3Hash::hash(&metallib_bytes);
let expected = B3Hash::from_hex(&expected_hash)?;

if actual_hash != expected {
    return Err(AosError::Verification(format!(
        "Kernel hash mismatch: expected {}, got {}",
        expected_hash,
        actual_hash.to_hex()
    )));
}
```

## 9. CI/CD Integration

### Location: `.github/workflows/`

**Metal Build Workflow** (`.github/workflows/metal-build.yml`)
- Verifies `kernel_hash.txt` exists after build
- Displays hash in CI logs
- Ensures hash is committed to repository

**Multi-Backend Workflow** (`.github/workflows/multi-backend.yml`)
- Catches and displays kernel hash during build
- Used for build verification

## 10. Test Verification

### Location: `tests/`

**Kernel Compilation Test** (`tests/kernel_compilation_test.rs`)
- Verifies `kernel_hash.txt` is created
- Ensures hash is deterministic across builds

**Kernel Verification Test** (`tests/kernel_verification_test.rs`)
- Tests hash verification logic
- Ensures mismatches are detected

## Summary: Complete Flow

```
1. BUILD TIME
   build.rs → Compile metallib → Compute BLAKE3 hash
   ↓
   Write kernel_hash.txt
   ↓
   Generate & sign manifest.json (includes kernel_hash)
   ↓
   Emit cargo:warning with hash

2. COMPILE TIME
   include_str!("kernel_hash.txt") → METALLIB_HASH constant
   include_bytes!("metallib") → METALLIB_BYTES constant
   include_str!("manifest.json") → Embedded manifest

3. RUNTIME
   MetalKernels::new()
   ↓
   verify_embedded_manifest() → Verify signature & hash
   ↓
   load_library()
   ↓
   B3Hash::hash(METALLIB_BYTES) == B3Hash::from_hex(METALLIB_HASH)
   ↓
   Success: Load Metal library
   Failure: DeterminismViolation error

4. ATTESTATION
   attest_determinism()
   ↓
   Extract hash from METALLIB_HASH
   ↓
   Verify against actual metallib hash
   ↓
   Include in DeterminismReport

5. PERSISTENCE
   Plans store kernel_hashes_json
   Replay sessions track kernel_hash_b3
   API responses include kernel_hash_b3
```

## Security & Determinism Guarantees

1. **Build-Time Integrity**: Hash computed immediately after compilation
2. **Compile-Time Embedding**: Hash embedded as constant, cannot be modified without recompilation
3. **Runtime Verification**: Hash verified before library load
4. **Manifest Signing**: Ed25519 signature prevents tampering
5. **Attestation**: Hash included in determinism reports for audit
6. **Database Tracking**: Plans track kernel versions for rebuild detection

## Related Documentation

- `docs/METAL_BACKEND.md`: Metal backend architecture
- `docs/hardening/PR-006-METALLIB-HASH-ENFORCEMENT.md`: Hash enforcement policy
- `docs/CONTENT_ADDRESSING_INTEGRITY_VERIFICATION.md`: Content addressing system
