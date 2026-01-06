# PR-006: Metallib Hash Enforcement

## Summary

Ensure Metal shader library (`.metallib`) hashes are computed at build time, verified at runtime, and included in backend attestation to detect kernel tampering or compiler drift.

## Problem Statement

Metal kernels are compiled from `.metal` sources to `.metallib` binaries. Currently:

1. **No build-time hash**: Metallib files don't have accompanying hash files
2. **No runtime verification**: Loaded metallib isn't verified against expected hash
3. **Attestation gap**: `backend_attestation_b3` doesn't include metallib hash
4. **Compiler drift undetected**: Different Xcode versions may produce different metallibs

A tampered or recompiled metallib could produce subtly different floating-point results, breaking determinism without detection.

## Solution

1. Build script produces `kernel.metallib.b3hash` alongside `.metallib`
2. Runtime loads expected hash and verifies loaded metallib
3. `DeterminismReport` includes `metallib_hash` field
4. `backend_attestation_b3` incorporates metallib hash
5. Hash mismatch triggers `DeterminismViolation::MetallibHashMismatch`

---

## Implementation Details

### File Changes

#### 1. `metal/build.sh`

**Update build script to produce hash file**:

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUTPUT_DIR="${SCRIPT_DIR}/../target/metal"
METALLIB_NAME="adapteros_kernels.metallib"

mkdir -p "${OUTPUT_DIR}"

echo "==> Compiling Metal shaders..."

# Compile .metal sources to .air (intermediate)
xcrun -sdk macosx metal \
    -c "${SCRIPT_DIR}/kernels.metal" \
    -o "${OUTPUT_DIR}/kernels.air" \
    -O2 \
    -fno-fast-math \
    -std=metal3.0

# Link .air to .metallib
xcrun -sdk macosx metallib \
    "${OUTPUT_DIR}/kernels.air" \
    -o "${OUTPUT_DIR}/${METALLIB_NAME}"

# Compute BLAKE3 hash of metallib
echo "==> Computing BLAKE3 hash..."
if command -v b3sum &> /dev/null; then
    HASH=$(b3sum --no-names "${OUTPUT_DIR}/${METALLIB_NAME}")
elif command -v blake3 &> /dev/null; then
    HASH=$(blake3 "${OUTPUT_DIR}/${METALLIB_NAME}")
else
    # Fallback: use Rust tool
    HASH=$(cargo run --quiet -p adapteros-cli -- hash-file "${OUTPUT_DIR}/${METALLIB_NAME}")
fi

# Write hash file
echo "${HASH}" > "${OUTPUT_DIR}/${METALLIB_NAME}.b3hash"
echo "==> Hash: ${HASH}"

# Write metadata JSON
cat > "${OUTPUT_DIR}/${METALLIB_NAME}.meta.json" << EOF
{
    "metallib_name": "${METALLIB_NAME}",
    "metallib_hash_b3": "${HASH}",
    "build_timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
    "metal_version": "$(xcrun -sdk macosx metal --version 2>&1 | head -1)",
    "compiler_flags": "-O2 -fno-fast-math -std=metal3.0",
    "sdk": "macosx"
}
EOF

echo "==> Build complete: ${OUTPUT_DIR}/${METALLIB_NAME}"
echo "==> Hash file: ${OUTPUT_DIR}/${METALLIB_NAME}.b3hash"
echo "==> Metadata: ${OUTPUT_DIR}/${METALLIB_NAME}.meta.json"

# Clean up intermediate files
rm -f "${OUTPUT_DIR}/kernels.air"
```

#### 2. `crates/adapteros-lora-kernel-mtl/src/lib.rs`

**Add metallib loading with hash verification**:

```rust
use adapteros_core::{AosError, B3Hash, Result};
use metal::{Device, Library};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Cached metallib info
static METALLIB_INFO: OnceLock<MetallibInfo> = OnceLock::new();

/// Information about the loaded metallib
#[derive(Debug, Clone)]
pub struct MetallibInfo {
    /// Path to the metallib file
    pub path: PathBuf,
    /// BLAKE3 hash of the metallib
    pub hash: B3Hash,
    /// Whether hash was verified against expected
    pub verified: bool,
    /// Metal library handle (not Clone, accessed via get_library())
    library_loaded: bool,
}

/// Expected metallib hash (embedded at build time or loaded from file)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedMetallibHash {
    pub metallib_name: String,
    pub metallib_hash_b3: String,
    pub build_timestamp: Option<String>,
    pub metal_version: Option<String>,
    pub compiler_flags: Option<String>,
}

impl ExpectedMetallibHash {
    /// Load expected hash from .b3hash file
    pub fn load_from_file(metallib_path: &Path) -> Result<Self> {
        let hash_path = metallib_path.with_extension("metallib.b3hash");
        let meta_path = metallib_path.with_extension("metallib.meta.json");

        // Try to load full metadata first
        if meta_path.exists() {
            let content = std::fs::read_to_string(&meta_path)?;
            let expected: ExpectedMetallibHash = serde_json::from_str(&content)?;
            return Ok(expected);
        }

        // Fall back to just hash file
        if hash_path.exists() {
            let hash_str = std::fs::read_to_string(&hash_path)?.trim().to_string();
            return Ok(Self {
                metallib_name: metallib_path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                metallib_hash_b3: hash_str,
                build_timestamp: None,
                metal_version: None,
                compiler_flags: None,
            });
        }

        Err(AosError::Config(format!(
            "No hash file found for metallib: {}",
            metallib_path.display()
        )))
    }

    /// Parse the hash string to B3Hash
    pub fn to_b3hash(&self) -> Result<B3Hash> {
        B3Hash::from_hex(&self.metallib_hash_b3)
    }
}

/// Load and verify metallib from path.
///
/// # Verification Process
///
/// 1. Compute BLAKE3 hash of metallib file
/// 2. Load expected hash from `.b3hash` or `.meta.json` file
/// 3. Compare hashes
/// 4. If mismatch and `strict = true`, return error
/// 5. Cache result for subsequent calls
///
/// # Errors
///
/// Returns `AosError::DeterminismViolation` if:
/// - Hash mismatch in strict mode
/// - Expected hash file not found in strict mode
pub fn load_metallib_verified(
    device: &Device,
    metallib_path: &Path,
    strict: bool,
) -> Result<MetallibInfo> {
    // Check cache first
    if let Some(info) = METALLIB_INFO.get() {
        if info.path == metallib_path {
            return Ok(info.clone());
        }
    }

    // Compute actual hash
    let metallib_bytes = std::fs::read(metallib_path)?;
    let actual_hash = B3Hash::hash(&metallib_bytes);

    tracing::info!(
        path = %metallib_path.display(),
        hash = %actual_hash.to_short_hex(),
        "Computing metallib hash"
    );

    // Load expected hash
    let expected = match ExpectedMetallibHash::load_from_file(metallib_path) {
        Ok(exp) => Some(exp),
        Err(e) => {
            if strict {
                return Err(AosError::DeterminismViolation(format!(
                    "Metallib hash file required in strict mode: {}",
                    e
                )));
            }
            tracing::warn!(
                path = %metallib_path.display(),
                error = %e,
                "No expected hash file - cannot verify metallib"
            );
            None
        }
    };

    // Verify hash if expected is available
    let verified = if let Some(ref exp) = expected {
        let expected_hash = exp.to_b3hash()?;
        if actual_hash != expected_hash {
            if strict {
                return Err(AosError::DeterminismViolation(format!(
                    "Metallib hash mismatch: expected {}, got {}",
                    expected_hash.to_short_hex(),
                    actual_hash.to_short_hex()
                )));
            }
            tracing::error!(
                expected = %expected_hash.to_short_hex(),
                actual = %actual_hash.to_short_hex(),
                "METALLIB HASH MISMATCH - determinism may be compromised"
            );
            metrics::counter!("metallib_hash_mismatch_total").increment(1);
            false
        } else {
            tracing::info!(
                hash = %actual_hash.to_short_hex(),
                "Metallib hash verified"
            );
            true
        }
    } else {
        false
    };

    // Load the library
    let _library = device.new_library_with_file(metallib_path)?;

    let info = MetallibInfo {
        path: metallib_path.to_path_buf(),
        hash: actual_hash,
        verified,
        library_loaded: true,
    };

    // Cache the info
    let _ = METALLIB_INFO.set(info.clone());

    Ok(info)
}

/// Get the current metallib hash (for attestation).
///
/// Returns None if metallib hasn't been loaded yet.
pub fn get_metallib_hash() -> Option<B3Hash> {
    METALLIB_INFO.get().map(|info| info.hash)
}

/// Get full metallib info.
pub fn get_metallib_info() -> Option<MetallibInfo> {
    METALLIB_INFO.get().cloned()
}
```

#### 3. `crates/adapteros-lora-kernel-api/src/attestation.rs`

**Update `DeterminismReport` to include metallib hash**:

```rust
use adapteros_core::B3Hash;
use serde::{Deserialize, Serialize};

/// Determinism report for backend attestation.
///
/// This struct captures all factors that affect determinism for a backend,
/// enabling verification that the same configuration was used across runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterminismReport {
    /// Backend identifier (metal, coreml, mlx)
    pub backend: String,

    /// Metallib hash for Metal backend (None for non-Metal backends)
    pub metallib_hash: Option<B3Hash>,

    /// Whether metallib hash was verified against expected
    pub metallib_verified: bool,

    /// RNG seeding method (hkdf, chacha, etc.)
    pub rng_seed_method: String,

    /// Floating point mode (strict, relaxed)
    pub floating_point_mode: String,

    /// Determinism level achieved (bitexact, approximate, non_deterministic)
    pub determinism_level: String,

    /// Compiler/runtime version string
    pub runtime_version: String,

    /// Relevant compiler flags
    pub compiler_flags: Vec<String>,

    /// Device identifier (for multi-GPU setups)
    pub device_id: Option<String>,
}

impl DeterminismReport {
    /// Create a report for Metal backend.
    pub fn for_metal(metallib_info: Option<&MetallibInfo>) -> Self {
        Self {
            backend: "metal".to_string(),
            metallib_hash: metallib_info.map(|i| i.hash),
            metallib_verified: metallib_info.map(|i| i.verified).unwrap_or(false),
            rng_seed_method: "hkdf".to_string(),
            floating_point_mode: "strict".to_string(),
            determinism_level: if metallib_info.map(|i| i.verified).unwrap_or(false) {
                "bitexact".to_string()
            } else {
                "approximate".to_string()
            },
            runtime_version: get_metal_version(),
            compiler_flags: vec![
                "-O2".to_string(),
                "-fno-fast-math".to_string(),
                "-std=metal3.0".to_string(),
            ],
            device_id: None,
        }
    }

    /// Create a report for MLX backend.
    pub fn for_mlx() -> Self {
        Self {
            backend: "mlx".to_string(),
            metallib_hash: None, // MLX uses its own kernels
            metallib_verified: false,
            rng_seed_method: "hkdf".to_string(),
            floating_point_mode: "strict".to_string(),
            determinism_level: "bitexact".to_string(),
            runtime_version: get_mlx_version(),
            compiler_flags: vec![],
            device_id: None,
        }
    }

    /// Create a report for CoreML backend.
    pub fn for_coreml() -> Self {
        Self {
            backend: "coreml".to_string(),
            metallib_hash: None,
            metallib_verified: false,
            rng_seed_method: "hkdf".to_string(),
            floating_point_mode: "relaxed".to_string(), // CoreML doesn't guarantee strict
            determinism_level: "approximate".to_string(),
            runtime_version: get_coreml_version(),
            compiler_flags: vec![],
            device_id: None,
        }
    }

    /// Compute attestation hash for receipt binding.
    ///
    /// This hash uniquely identifies the backend configuration used for inference.
    /// Different configurations MUST produce different hashes.
    pub fn to_attestation_hash(&self) -> B3Hash {
        let mut components: Vec<Vec<u8>> = vec![
            self.backend.as_bytes().to_vec(),
            self.rng_seed_method.as_bytes().to_vec(),
            self.floating_point_mode.as_bytes().to_vec(),
            self.determinism_level.as_bytes().to_vec(),
            self.runtime_version.as_bytes().to_vec(),
        ];

        // Include metallib hash if present (critical for Metal determinism)
        if let Some(ref mlh) = self.metallib_hash {
            components.push(mlh.as_bytes().to_vec());
        }

        // Include metallib verification status
        components.push(vec![if self.metallib_verified { 1 } else { 0 }]);

        // Include device ID if present
        if let Some(ref device) = self.device_id {
            components.push(device.as_bytes().to_vec());
        }

        // Include sorted compiler flags
        let mut flags_sorted: Vec<_> = self.compiler_flags.iter().collect();
        flags_sorted.sort();
        for flag in flags_sorted {
            components.push(flag.as_bytes().to_vec());
        }

        // Length-prefix each component for unambiguous parsing
        let mut buf = Vec::new();
        for component in &components {
            buf.extend_from_slice(&(component.len() as u32).to_le_bytes());
            buf.extend_from_slice(component);
        }

        B3Hash::hash(&buf)
    }

    /// Check if this report indicates verified determinism.
    pub fn is_verified_deterministic(&self) -> bool {
        self.determinism_level == "bitexact" &&
        (self.backend != "metal" || self.metallib_verified)
    }
}

fn get_metal_version() -> String {
    // Query Metal runtime version
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        Command::new("xcrun")
            .args(["--sdk", "macosx", "metal", "--version"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.lines().next().unwrap_or("unknown").to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        "not_available".to_string()
    }
}

fn get_mlx_version() -> String {
    // MLX version from linked library
    "mlx-0.21.0".to_string() // Would query actual version
}

fn get_coreml_version() -> String {
    // CoreML version from system
    "coreml-8.0".to_string() // Would query actual version
}
```

#### 4. `crates/adapteros-core/src/evidence_envelope.rs`

**Verify backend_attestation includes metallib**:

```rust
impl InferenceReceiptRef {
    /// Validate that attestation properly reflects backend identity.
    pub fn validate_attestation(&self, expected_report: &DeterminismReport) -> bool {
        let Some(ref attestation) = self.backend_attestation_b3 else {
            return false;
        };

        let expected = expected_report.to_attestation_hash();
        attestation == &expected
    }
}
```

#### 5. `crates/adapteros-lora-worker/src/generation.rs`

**Capture determinism report during inference**:

```rust
use adapteros_lora_kernel_api::attestation::DeterminismReport;
use adapteros_lora_kernel_mtl::get_metallib_info;

impl InferenceEngine {
    /// Get determinism report for current backend.
    pub fn get_determinism_report(&self) -> DeterminismReport {
        match self.backend {
            BackendKind::Metal => {
                let metallib_info = get_metallib_info();
                DeterminismReport::for_metal(metallib_info.as_ref())
            }
            BackendKind::MLX => DeterminismReport::for_mlx(),
            BackendKind::CoreML => DeterminismReport::for_coreml(),
            BackendKind::CPU => DeterminismReport {
                backend: "cpu".to_string(),
                metallib_hash: None,
                metallib_verified: false,
                rng_seed_method: "hkdf".to_string(),
                floating_point_mode: "strict".to_string(),
                determinism_level: "bitexact".to_string(),
                runtime_version: "rust-cpu".to_string(),
                compiler_flags: vec![],
                device_id: None,
            },
        }
    }
}
```

---

## Acceptance Criteria

- [ ] Build script produces `.metallib.b3hash` file alongside metallib
- [ ] Build script produces `.metallib.meta.json` with full metadata
- [ ] `load_metallib_verified()` computes and verifies hash at runtime
- [ ] Hash mismatch in strict mode returns `DeterminismViolation` error
- [ ] Hash mismatch in non-strict mode logs error and increments metric
- [ ] `DeterminismReport` includes `metallib_hash` and `metallib_verified`
- [ ] `to_attestation_hash()` incorporates metallib hash
- [ ] Different metallib hashes produce different attestation hashes
- [ ] `metallib_hash_mismatch_total` metric exposed

---

## Test Plan

### Unit Tests

**File**: `crates/adapteros-lora-kernel-mtl/tests/metallib_verification_tests.rs`

```rust
#[test]
fn test_hash_file_loading() {
    let temp_dir = tempdir::TempDir::new("metallib_test").unwrap();
    let metallib_path = temp_dir.path().join("test.metallib");
    let hash_path = temp_dir.path().join("test.metallib.b3hash");

    // Create fake metallib
    std::fs::write(&metallib_path, b"fake metallib content").unwrap();
    let expected_hash = B3Hash::hash(b"fake metallib content");
    std::fs::write(&hash_path, expected_hash.to_hex()).unwrap();

    let expected = ExpectedMetallibHash::load_from_file(&metallib_path).unwrap();
    assert_eq!(expected.to_b3hash().unwrap(), expected_hash);
}

#[test]
fn test_hash_mismatch_detected() {
    let temp_dir = tempdir::TempDir::new("metallib_test").unwrap();
    let metallib_path = temp_dir.path().join("test.metallib");
    let hash_path = temp_dir.path().join("test.metallib.b3hash");

    // Create metallib with wrong hash file
    std::fs::write(&metallib_path, b"actual content").unwrap();
    std::fs::write(&hash_path, B3Hash::hash(b"different content").to_hex()).unwrap();

    // Non-strict: should succeed but not be verified
    // (Can't test actual Metal loading without device, but can test hash logic)
    let expected = ExpectedMetallibHash::load_from_file(&metallib_path).unwrap();
    let actual_hash = B3Hash::hash(b"actual content");
    assert_ne!(expected.to_b3hash().unwrap(), actual_hash);
}

#[test]
fn test_metadata_json_loading() {
    let temp_dir = tempdir::TempDir::new("metallib_test").unwrap();
    let metallib_path = temp_dir.path().join("test.metallib");
    let meta_path = temp_dir.path().join("test.metallib.meta.json");

    std::fs::write(&metallib_path, b"content").unwrap();
    std::fs::write(&meta_path, r#"{
        "metallib_name": "test.metallib",
        "metallib_hash_b3": "abc123...",
        "build_timestamp": "2026-01-06T00:00:00Z",
        "metal_version": "metal 3.0",
        "compiler_flags": "-O2 -fno-fast-math"
    }"#).unwrap();

    let expected = ExpectedMetallibHash::load_from_file(&metallib_path).unwrap();
    assert_eq!(expected.metal_version, Some("metal 3.0".to_string()));
}
```

### Attestation Tests

**File**: `crates/adapteros-lora-kernel-api/tests/attestation_tests.rs`

```rust
#[test]
fn test_attestation_hash_includes_metallib() {
    let hash1 = B3Hash::hash(b"metallib-v1");
    let hash2 = B3Hash::hash(b"metallib-v2");

    let report1 = DeterminismReport {
        backend: "metal".to_string(),
        metallib_hash: Some(hash1),
        metallib_verified: true,
        ..Default::default()
    };

    let report2 = DeterminismReport {
        backend: "metal".to_string(),
        metallib_hash: Some(hash2),
        metallib_verified: true,
        ..Default::default()
    };

    assert_ne!(
        report1.to_attestation_hash(),
        report2.to_attestation_hash(),
        "Different metallib hashes must produce different attestation"
    );
}

#[test]
fn test_attestation_hash_deterministic() {
    let report = DeterminismReport::for_metal(None);

    let hash1 = report.to_attestation_hash();
    let hash2 = report.to_attestation_hash();

    assert_eq!(hash1, hash2);
}

#[test]
fn test_verified_flag_affects_attestation() {
    let metallib_hash = B3Hash::hash(b"metallib");

    let verified = DeterminismReport {
        backend: "metal".to_string(),
        metallib_hash: Some(metallib_hash),
        metallib_verified: true,
        ..Default::default()
    };

    let unverified = DeterminismReport {
        backend: "metal".to_string(),
        metallib_hash: Some(metallib_hash),
        metallib_verified: false,
        ..Default::default()
    };

    assert_ne!(
        verified.to_attestation_hash(),
        unverified.to_attestation_hash(),
        "Verification status must affect attestation"
    );
}

#[test]
fn test_is_verified_deterministic() {
    let verified_metal = DeterminismReport {
        backend: "metal".to_string(),
        metallib_hash: Some(B3Hash::hash(b"test")),
        metallib_verified: true,
        determinism_level: "bitexact".to_string(),
        ..Default::default()
    };
    assert!(verified_metal.is_verified_deterministic());

    let unverified_metal = DeterminismReport {
        backend: "metal".to_string(),
        metallib_hash: Some(B3Hash::hash(b"test")),
        metallib_verified: false,
        determinism_level: "bitexact".to_string(),
        ..Default::default()
    };
    assert!(!unverified_metal.is_verified_deterministic());

    let mlx = DeterminismReport::for_mlx();
    assert!(mlx.is_verified_deterministic()); // MLX doesn't need metallib
}
```

### Integration Tests

**File**: `tests/metallib_e2e.rs`

```rust
#[cfg(target_os = "macos")]
#[tokio::test]
async fn test_inference_with_verified_metallib() {
    // Build metallib with hash
    run_command("bash", &["metal/build.sh"]).await;

    // Verify hash file exists
    let hash_path = Path::new("target/metal/adapteros_kernels.metallib.b3hash");
    assert!(hash_path.exists(), "Hash file should be created by build");

    // Start server and run inference
    let server = start_test_server().await;
    let response = server.post("/v1/inference")
        .json(&inference_request())
        .send()
        .await;

    assert!(response.status().is_success());

    // Check determinism report in trace
    let trace = server.get(&format!("/v1/trace/{}", response.trace_id))
        .send()
        .await
        .json::<InferenceTrace>();

    if trace.backend_used == Some("metal".to_string()) {
        assert!(trace.determinism_report.metallib_verified);
    }
}
```

---

## Build Integration

### CI Pipeline Addition

```yaml
# .github/workflows/ci.yml
jobs:
  build-metal:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4

      - name: Build Metal shaders
        run: bash metal/build.sh

      - name: Verify hash file created
        run: |
          test -f target/metal/adapteros_kernels.metallib.b3hash
          cat target/metal/adapteros_kernels.metallib.b3hash

      - name: Upload metallib artifacts
        uses: actions/upload-artifact@v4
        with:
          name: metallib
          path: |
            target/metal/*.metallib
            target/metal/*.b3hash
            target/metal/*.meta.json
```

---

## Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `metallib_hash_mismatch_total` | Counter | Metallib hash verification failures |
| `metallib_verified_loads_total` | Counter | Successfully verified metallib loads |
| `metallib_unverified_loads_total` | Counter | Metallib loads without verification |

---

## Deployment Notes

1. **Build pipeline must run `metal/build.sh`** before creating release artifacts
2. **Hash files must be distributed** alongside metallib files
3. **First deployment**: Set `strict = false` to allow unverified metallibs
4. **Subsequent deployments**: Enable `strict = true` after hash files are in place
5. **Xcode updates**: Rebuild metallib and update hash files after compiler changes
