# Stub Implementations Reference

**Last Updated:** 2025-11-22
**Maintained by:** James KC Auchterlonie

This document provides comprehensive documentation of all stub implementations in AdapterOS.
Stubs allow the system to build and run on platforms without specific hardware or libraries,
enabling development, testing, and graceful degradation.

---

## Table of Contents

1. [Overview](#overview)
2. [MLX Backend Stubs](#mlx-backend-stubs)
3. [Platform-Specific Stubs](#platform-specific-stubs)
4. [Enclave/Security Stubs](#enclavesecurity-stubs)
5. [KMS Stubs](#kms-stubs)
6. [API Handler Stubs](#api-handler-stubs)
7. [Feature Flag Quick Reference Table](#feature-flag-quick-reference-table)
8. [Agent Guidance](#agent-guidance)

---

## Overview

### Purpose

Stub implementations serve several critical purposes in AdapterOS:

1. **Cross-Platform Development**: Enable development on platforms without specific hardware (e.g., developing MLX code on non-Apple hardware)
2. **Testing**: Provide deterministic, predictable behavior for unit and integration tests
3. **Graceful Degradation**: Allow production systems to continue operating when optional features are unavailable
4. **CI/CD Compatibility**: Enable builds and tests in environments without GPUs, HSMs, or specialized hardware

### How to Use This Guide

- **Developers**: Reference this document when encountering unexpected behavior to determine if a stub is active
- **DevOps/SRE**: Use the feature flag table to configure production deployments correctly
- **QA**: Understand which features require specific hardware or configuration for testing
- **Auditors**: Identify which components use software fallbacks vs hardware-backed security

### General Pattern

Stubs in AdapterOS follow a consistent pattern:

```rust
// Feature-gated real implementation
#[cfg(feature = "real-mlx")]
fn operation() -> Result<T> {
    // Real implementation using actual hardware/library
}

// Stub fallback
#[cfg(not(feature = "real-mlx"))]
fn operation() -> Result<T> {
    // Stub implementation with predictable behavior
}
```

---

## MLX Backend Stubs

### Overview

The MLX (Apple ML eXtension) backend provides GPU-accelerated inference on Apple Silicon.
When the `real-mlx` feature is disabled, stub implementations provide a compatible API
without actual GPU acceleration.

### Feature Flag: `real-mlx`

| Feature | Stub Behavior | Real Behavior |
|---------|--------------|---------------|
| Model Loading | Returns dummy model struct | Loads actual safetensors weights |
| Forward Pass | Returns constant 0.5 values | Performs real matrix operations |
| Text Generation | Returns empty/placeholder text | Generates actual text |
| Memory Stats | Returns 1MB usage | Reports actual GPU memory |
| K-Sparse Routing | Simulates gate application | Performs real Q15 gate computations |

### Files

**Primary Stub Implementation:**
- `crates/adapteros-lora-mlx-ffi/src/mlx_cpp_wrapper.cpp`

**Real Implementation:**
- `crates/adapteros-lora-mlx-ffi/src/mlx_cpp_wrapper_real.cpp`

### Build Configuration

```bash
# Build with stub (default)
cargo build -p adapteros-lora-mlx-ffi

# Build with real MLX (requires mlx C++ library installed)
cargo build -p adapteros-lora-mlx-ffi --features real-mlx
```

### Link-Time Selection

The Rust code uses conditional linking:

```rust
#[cfg_attr(feature = "real-mlx", link(name = "mlx_wrapper"))]
#[cfg_attr(not(feature = "real-mlx"), link(name = "mlx_wrapper_stub"))]
extern "C" {
    // FFI declarations
}
```

### Stub Function Behaviors

| Function | Stub Behavior |
|----------|--------------|
| `mlx_model_load()` | Creates `StubModel` with dummy 1000-element weights |
| `mlx_model_forward()` | Returns array filled with 0.5f values |
| `mlx_model_forward_with_hidden_states()` | Returns 4 hidden states (q/k/v/o_proj) with dummy values |
| `mlx_multi_lora_forward()` | Applies simplified Q15 gate scaling without real matmul |
| `mlx_matmul()` | Returns input scaled by 0.5f (no real matrix multiply) |
| `mlx_softmax()` | Implements real softmax algorithm |
| `mlx_relu/gelu/sigmoid/tanh()` | Implement real activation functions |
| `mlx_set_seed()` | No-op (no RNG state to set) |
| `mlx_memory_usage()` | Returns 1MB constant |
| `mlx_gc_collect()` | No-op |
| `mlx_quantize/dequantize()` | Returns copy of input (no actual quantization) |
| `mlx_kv_cache_*()` | Maintains simple seq_len counter, no actual caching |

### Detection

Check if stub is active at runtime:

```rust
// The stub version returns "stub-0.1.0"
let version = unsafe { mlx_get_version() };
if version.starts_with("stub") {
    warn!("MLX stub implementation active - no GPU acceleration");
}
```

---

## Platform-Specific Stubs

### CoreML (macOS Only)

**Location:** `crates/adapteros-lora-kernel-coreml/`

CoreML backends are macOS-only. On non-macOS platforms:

| Component | macOS Behavior | Other Platforms |
|-----------|---------------|-----------------|
| Model Loading | Uses CoreML framework | Compile-time error or feature gate |
| ANE Detection | Queries actual hardware | Returns `false` |
| Swift Bridge | Compiles MLTensor bridge | Not compiled |

**Build Detection:**
```rust
#[cfg(target_os = "macos")]
fn use_coreml() -> bool { true }

#[cfg(not(target_os = "macos"))]
fn use_coreml() -> bool { false }
```

### Metal Kernels (macOS Only)

**Location:** `crates/adapteros-lora-kernel-mtl/`

Metal GPU kernels are macOS-specific:

| Component | macOS Behavior | Other Platforms |
|-----------|---------------|-----------------|
| Shader Compilation | Uses Metal compiler | Stub returns errors |
| Memory Integration | Uses Metal heaps | Falls back to CPU memory |
| GPU Commands | Executes on GPU | CPU fallback or error |

**Platform Guard Pattern:**
```rust
#[cfg(target_os = "macos")]
impl MetalBackend {
    pub fn new() -> Result<Self> {
        // Real Metal initialization
    }
}

#[cfg(not(target_os = "macos"))]
impl MetalBackend {
    pub fn new() -> Result<Self> {
        Err(AosError::Platform("Metal not available on this platform"))
    }
}
```

### Memory/IOKit (macOS Only)

**Location:** `crates/adapteros-memory/src/page_migration_iokit.rs`

IOKit-based memory monitoring is macOS-specific:

| Feature | macOS Behavior | Other Platforms |
|---------|---------------|-----------------|
| Page Migration Tracking | Real VM statistics | Returns zeros/defaults |
| Memory Pressure Events | IOKit callbacks | No monitoring |
| Unified Memory Info | GPU/CPU migration stats | Empty stats |
| ANE Memory Tracking | Apple Neural Engine stats | Returns 0 |

**FFI Structures:**
```c
// These FFI structures are populated on macOS, zeroed on other platforms
struct FFIPageMigrationInfo { ... };
struct FFIVMStats { ... };
struct FFIUnifiedMemoryInfo { ... };
```

### Heap Observer (macOS Only)

**Location:** `crates/adapteros-memory/src/heap_observer.rs`

Objective-C++ implementation for Metal heap observation:

| Feature | macOS Behavior | Other Platforms |
|---------|---------------|-----------------|
| Heap Callbacks | Receives Metal allocations | No-op stubs |
| Memory Warnings | System notifications | Manual polling |
| GPU Memory Stats | Direct Metal queries | Estimated values |

### Keychain Provider (Platform-Specific)

**Location:** `crates/adapteros-crypto/src/providers/keychain.rs`

| Platform | Backend | Implementation |
|----------|---------|----------------|
| macOS | Security Framework | Native keychain via `security-framework` crate |
| Linux | Secret Service / Kernel Keyring | D-Bus or `keyutils` syscalls |
| Windows | Not supported | Returns error |
| Other | Password Fallback | Encrypted JSON file (opt-in) |

**Backend Detection:**
```rust
pub enum KeychainBackend {
    MacOS,           // Security Framework
    SecretService,   // Linux D-Bus
    KernelKeyring,   // Linux keyutils
    PasswordFallback, // Argon2id + AES-256-GCM file
}
```

**Password Fallback Activation:**
```bash
# For CI/headless environments (NOT for production)
export ADAPTEROS_KEYCHAIN_FALLBACK=pass:your-secure-password
```

---

## Enclave/Security Stubs

### Software Enclave Fallback

**Location:** `crates/adapteros-secd/src/enclave/stub.rs`

When hardware Secure Enclave is unavailable, a software fallback provides
cryptographic operations with reduced security guarantees.

### Hardware vs Software Comparison

| Property | Hardware Enclave | Software Fallback |
|----------|-----------------|-------------------|
| Key Storage | Tamper-resistant chip | Process memory |
| Boot Attestation | Secure boot chain | None |
| Side-Channel Resistance | Hardware protection | Software mitigations only |
| Key Extraction | Impossible | Possible with memory access |
| Use Case | Production | Development/Testing |

### Software Fallback Implementation

The `EnclaveManager` in stub mode:

```rust
pub struct EnclaveManager {
    key_cache: HashMap<String, Vec<u8>>,          // In-memory key storage
    signing_key_cache: HashMap<String, SigningKey>, // Ed25519 keys
    root_key: [u8; 32],                           // HKDF root (from OS entropy)
    is_software_fallback: bool,                   // Always true in stub
}
```

### Security Model

- **Key Derivation:** HKDF-SHA256 with domain separation
- **Encryption:** ChaCha20-Poly1305 (AEAD)
- **Signing:** Ed25519 (not ECDSA like hardware enclave)
- **Key Lifecycle:** Ephemeral (cleared on process exit)

### API Compatibility

The software fallback implements the same API as hardware enclave:

| Method | Stub Behavior |
|--------|--------------|
| `seal_lora_delta()` | ChaCha20-Poly1305 encryption with HKDF-derived key |
| `unseal_lora_delta()` | ChaCha20-Poly1305 decryption |
| `sign_bundle()` | Ed25519 signature with derived signing key |
| `seal_with_label()` | Generic encryption with label-derived key |
| `get_public_key()` | Returns Ed25519 verifying key bytes |

### Detection

```rust
let enclave = EnclaveManager::new()?;
if enclave.is_software_fallback() {
    warn!("Using software enclave fallback - reduced security");
}
```

### Log Messages

Software fallback logs warnings at startup:

```
WARN  Secure Enclave not available: using software-based fallback (development/testing only)
INFO  Software fallback initialized with HKDF-derived keys (ChaCha20-Poly1305 + Ed25519)
```

---

## KMS Stubs

### Overview

**Location:** `crates/adapteros-crypto/src/providers/kms.rs`

The KMS (Key Management Service) provider supports multiple cloud and hardware backends.
When real backends are unavailable, a `MockKmsBackend` provides testing functionality.

### Supported Backends

| Backend | Feature Flag | Status |
|---------|-------------|--------|
| AWS KMS | `aws-kms` | Real implementation |
| GCP Cloud KMS | `gcp-kms` | Real implementation |
| Azure Key Vault | `azure-keyvault` | Stub (mock implementation) |
| HashiCorp Vault | - | Stub (mock implementation) |
| PKCS#11 HSM | - | Stub (mock implementation) |
| Mock | (default) | Always available |

### Mock Backend Behavior

```rust
pub struct MockKmsBackend {
    keys: Arc<RwLock<HashMap<String, MockKey>>>,
}
```

| Operation | Mock Behavior |
|-----------|--------------|
| `generate_key()` | Creates random 32-byte key, stores in memory |
| `sign()` | HMAC-like construction (NOT cryptographically secure) |
| `encrypt()` | XOR with key (NOT cryptographically secure) |
| `decrypt()` | XOR with key (self-inverse) |
| `rotate_key()` | Increments version counter |
| `get_public_key()` | Returns derived public key bytes |
| `delete_key()` | Removes from in-memory HashMap |

### Feature Fallback Behavior

When a feature is not enabled, the system falls back to mock:

```rust
match config.backend_type {
    KmsBackendType::AwsKms => {
        #[cfg(feature = "aws-kms")]
        return Arc::new(AwsKmsBackend::new_async(config).await?);

        #[cfg(not(feature = "aws-kms"))]
        {
            warn!("AWS KMS backend not available (feature not enabled), using mock");
            Arc::new(MockKmsBackend::new())
        }
    }
    // Similar for GCP, Azure, Vault, HSM...
}
```

### Configuration

Default configuration uses mock backend:

```rust
impl Default for KmsConfig {
    fn default() -> Self {
        Self {
            backend_type: KmsBackendType::Mock,
            endpoint: "http://localhost:8200".to_string(),
            region: None,
            credentials: KmsCredentials::None,
            timeout_secs: 30,
            max_retries: 3,
            key_namespace: None,
        }
    }
}
```

### Security Warning

Mock KMS operations are **NOT cryptographically secure**:

```
WARN  Mock KMS: XOR encryption is NOT secure - for testing only
```

---

## API Handler Stubs

### Overview

**Location:** `crates/adapteros-server-api/src/handlers.rs`

Some API endpoints are defined but return `NOT_IMPLEMENTED` responses.
These are placeholders for future functionality.

### NOT_IMPLEMENTED Endpoints

The following endpoints return `StatusCode::NOT_IMPLEMENTED`:

| Endpoint Pattern | Purpose |
|-----------------|---------|
| Policy comparison endpoints | Compare policy versions |
| Advanced telemetry endpoints | Export/verify/purge telemetry bundles |
| Dry-run promotion endpoints | Preview promotion effects |
| Some streaming endpoints | Real-time event streams |

### Response Format

```rust
(
    StatusCode::NOT_IMPLEMENTED,
    Json(ErrorResponse::new("Endpoint not yet implemented")
        .with_code("NOT_IMPLEMENTED")),
)
```

### Stub Handler Functions

Many handlers contain stub logic with comments:

```rust
// Stub - would query database
// Stub - would validate, sign, and store policy
// Stub - would fetch bundle from telemetry store
// Stub - would apply retention policy and delete old bundles
// Stub - would compute from telemetry
```

These indicate the handler exists but returns placeholder data.

### Identifying Stub Handlers

Search for stub markers in handlers:

```bash
grep -n "Stub" crates/adapteros-server-api/src/handlers.rs
grep -n "NOT_IMPLEMENTED" crates/adapteros-server-api/src/handlers.rs
```

---

## Feature Flag Quick Reference Table

| Feature Flag | Crate | Stub Behavior | Real Behavior |
|-------------|-------|---------------|---------------|
| `real-mlx` | `adapteros-lora-mlx-ffi` | C++ stub with dummy ops | Real MLX GPU acceleration |
| `aws-kms` | `adapteros-crypto` | Falls back to MockKmsBackend | Real AWS KMS integration |
| `gcp-kms` | `adapteros-crypto` | Falls back to MockKmsBackend | Real GCP Cloud KMS |
| `azure-keyvault` | `adapteros-crypto` | Falls back to MockKmsBackend | Real Azure Key Vault |
| `linux-keychain` | `adapteros-crypto` | Password fallback | Linux Secret Service/Keyring |
| `password-fallback` | `adapteros-crypto` | N/A (enables fallback) | Encrypted JSON keystore |
| `test-utils` | Various | Enables mock implementations | N/A |

### Platform Conditionals

| Condition | Components Affected |
|-----------|-------------------|
| `target_os = "macos"` | CoreML, Metal, IOKit, Keychain, Secure Enclave |
| `target_os = "linux"` | Secret Service, Kernel Keyring |
| `target_os = "windows"` | Windows crypto (limited support) |

### Build Examples

```bash
# Minimal build (all stubs)
cargo build --release

# Production macOS build with real MLX
cargo build --release --features real-mlx

# Build with AWS KMS support
cargo build --release --features aws-kms

# Build with all KMS providers
cargo build --release --features "aws-kms,gcp-kms,azure-keyvault"

# Development build with test utilities
cargo build --features test-utils
```

---

## Agent Guidance

### How to Identify if a Stub is Active

1. **Check Feature Flags:**
   ```bash
   # List active features for a crate
   cargo tree -p adapteros-lora-mlx-ffi -f "{p} {f}"
   ```

2. **Check Log Output:**
   - Look for "stub", "fallback", "mock", "development" warnings
   - Software enclave logs: "using software-based fallback"
   - Mock KMS logs: "using mock"

3. **Check Runtime Detection:**
   ```rust
   // MLX version check
   let version = mlx_get_version();

   // Enclave fallback check
   enclave.is_software_fallback()

   // KMS backend type
   provider.backend_type() == KmsBackendType::Mock
   ```

4. **Check Platform:**
   ```rust
   // CoreML/Metal only on macOS
   #[cfg(target_os = "macos")]
   ```

### Production Readiness Checklist

Before deploying to production, verify:

- [ ] **MLX Backend:** `real-mlx` feature enabled if GPU inference required
- [ ] **Secure Enclave:** Hardware enclave available (not software fallback)
- [ ] **KMS Provider:** Real cloud KMS configured (not MockKmsBackend)
- [ ] **Keychain:** Native keychain available (not password fallback)
- [ ] **Platform:** Correct OS for platform-specific features

### Common Issues

| Symptom | Likely Cause | Solution |
|---------|-------------|----------|
| No GPU acceleration | `real-mlx` not enabled | Build with `--features real-mlx` |
| "Mock KMS" in logs | Feature not enabled | Add `aws-kms`, `gcp-kms`, etc. |
| "Software fallback" warning | No Secure Enclave | Use hardware with SEP |
| Zero memory stats | IOKit stubs on non-macOS | Expected on Linux/Windows |
| Keychain errors on Linux | D-Bus not available | Install gnome-keyring |

### Adding New Stubs

When implementing new platform-specific features:

1. **Define Feature Flag:**
   ```toml
   [features]
   real-feature = []  # Enable real implementation
   ```

2. **Create Stub Module:**
   ```rust
   #[cfg(feature = "real-feature")]
   mod real_impl;

   #[cfg(not(feature = "real-feature"))]
   mod stub_impl;
   ```

3. **Log Stub Activation:**
   ```rust
   warn!("Feature X not available: using stub implementation");
   ```

4. **Document in This File:**
   Add section describing stub behavior and detection methods.

---

## References

- [docs/COREML_ACTIVATION.md](COREML_ACTIVATION.md) - CoreML operational status
- [docs/MLX_INTEGRATION.md](MLX_INTEGRATION.md) - MLX setup guide
- [docs/ENCLAVE_FALLBACK.md](ENCLAVE_FALLBACK.md) - Enclave fallback details
- [docs/AZURE_KEYVAULT_INTEGRATION.md](AZURE_KEYVAULT_INTEGRATION.md) - Azure KMS setup
- [crates/adapteros-lora-mlx-ffi/README.md](../crates/adapteros-lora-mlx-ffi/README.md) - MLX FFI documentation
