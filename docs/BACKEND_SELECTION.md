# Backend Selection Guide

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.
**Last Updated:** 2026-01-02
**Purpose:** Complete guide to backend selection strategies in adapterOS

---

## Table of Contents

1. [Overview](#overview)
2. [BackendKind Enum](#backendkind-enum)
3. [MLX-First Priority Chain](#mlx-first-priority-chain)
4. [Backend Aliases](#backend-aliases)
5. [Available Backends](#available-backends)
6. [Selection Strategies](#selection-strategies)
7. [Capability Detection](#capability-detection)
8. [Configuration & Environment Variables](#configuration--environment-variables)
9. [Decision Flowchart](#decision-flowchart)
10. [Performance Characteristics](#performance-characteristics)
11. [Use Case Recommendations](#use-case-recommendations)
12. [Troubleshooting](#troubleshooting)

---

## Overview

adapterOS implements a **multi-backend architecture** that dynamically selects the optimal inference kernel based on:

- **Hardware capabilities** (Apple Neural Engine, Metal GPU, MLX support)
- **Model requirements** (size, architecture, quantization)
- **User preferences** (explicit backend selection or auto-detection)
- **Execution profile** (power efficiency vs. performance)

The backend selection system is implemented in `/Users/mln-dev/Dev/adapter-os/crates/adapteros-lora-worker/src/backend_factory.rs` and uses the canonical `BackendKind` enum from `adapteros-core`.

### Key Design Principles

1. **MLX-first priority**: Native macOS inference with unified memory and HKDF-seeded determinism
2. **CoreML as acceleration layer**: ANE provides power-efficient acceleration (50% savings) for specific operations
3. **Graceful fallbacks**: Automatic degradation when preferred backends are unavailable
4. **Deterministic selection**: Same inputs always produce the same backend choice
5. **Model caching**: Backends share a per-worker model cache to deduplicate loaded models

### Backend Selection Logic

```rust
// Backend selection follows MLX-first priority
// CoreML is used as an acceleration layer, not a standalone backend
return auto_select_backend(capabilities);  // Returns: MLX → CoreML → MlxBridge → Metal → CPU
```

**Rationale**: MLX is the primary backend for all inference and training. CoreML provides ANE acceleration for specific operations but is not a standalone backend due to its compiled/immutable package format.

---

## BackendKind Enum

The canonical `BackendKind` enum is defined in `adapteros-core/src/backend.rs` and serves as the single source of truth for all backend selection across adapterOS (inference + training).

### Enum Definition

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum BackendKind {
    #[default]
    Auto,      // Deterministic auto-selection
    CoreML,    // CoreML / ANE acceleration (macOS)
    Mlx,       // MLX FFI backend (macOS, HKDF-seeded determinism)
    MlxBridge, // MLX subprocess bridge (Python mlx-lm for MoE models)
    Metal,     // Metal GPU backend (macOS)
    CPU,       // CPU-only execution
}
```

### Backend Types

| Backend       | Description                                                 | Platform                    | Determinism      | Primary Use Case                                         |
| ------------- | ----------------------------------------------------------- | --------------------------- | ---------------- | -------------------------------------------------------- |
| **Auto**      | Deterministic auto-selection based on hardware capabilities | All                         | Inherited        | Default behavior - picks best available backend          |
| **Mlx**       | MLX FFI backend with C++ bindings                           | macOS (Apple Silicon)       | HKDF-seeded      | Primary inference/training, research workloads           |
| **MlxBridge** | MLX subprocess bridge via Python mlx-lm                     | macOS (Apple Silicon)       | Best-effort      | MoE (Mixture of Experts) models not supported by MLX FFI |
| **CoreML**    | Apple Neural Engine acceleration                            | macOS (Apple Silicon)       | Guaranteed (ANE) | Production inference with audit trails, power efficiency |
| **Metal**     | Metal GPU compute kernels                                   | macOS (Intel/Apple Silicon) | Guaranteed       | Legacy hardware, development/testing                     |
| **CPU**       | CPU-only execution                                          | All                         | N/A              | Not implemented for inference (observability only)       |

### Key Methods

```rust
impl BackendKind {
    /// Canonical string for logging/config surface
    pub fn as_str(&self) -> &'static str;

    /// List of canonical variants for error reporting
    pub fn variants() -> &'static [&'static str];

    /// Canonical MLX-first priority list for inference backends
    /// Order: MLX -> CoreML -> MlxBridge -> Metal -> CPU
    pub fn inference_priority() -> &'static [BackendKind];

    /// Check if this backend is an MLX variant (FFI or Bridge)
    pub fn is_mlx_variant(&self) -> bool;

    /// MLX-first default backend when capabilities allow it
    pub fn default_inference_backend() -> BackendKind;
}
```

---

## MLX-First Priority Chain

adapterOS implements an MLX-first priority chain for backend selection. This ensures consistent, deterministic backend selection across the control plane, worker selection, and UI hints.

### Priority Order

```
MLX -> CoreML -> MlxBridge -> Metal -> CPU
```

| Priority | Backend       | Rationale                                                   |
| -------- | ------------- | ----------------------------------------------------------- |
| 1        | **MLX**       | Primary backend for flexibility and HKDF-seeded determinism |
| 2        | **CoreML**    | First fallback for ANE acceleration when MLX unavailable    |
| 3        | **MlxBridge** | MoE models that MLX FFI doesn't support                     |
| 4        | **Metal**     | GPU fallback when MLX/CoreML unavailable                    |
| 5        | **CPU**       | Terminal entry (observability only - not implemented)       |

### Why MLX is Primary

1. **HKDF-Seeded Determinism**: MLX uses HKDF-SHA256 seed derivation from the manifest hash, ensuring reproducible inference and training across runs.

2. **Flexibility**: Native C++ bindings via FFI provide full feature set including:

   - Multi-adapter routing with K-sparse selection
   - Hot-swap adapter loading/unloading
   - Circuit breaker pattern for resilience
   - Unified memory tracking and GC hints

3. **Training Support**: MLX is the only backend that supports training workflows.

4. **Unified Memory**: Leverages Apple Silicon's unified memory architecture for efficient GPU/CPU data sharing.

### Selection Logic

```rust
// From BackendKind::inference_priority()
pub fn inference_priority() -> &'static [BackendKind] {
    static ORDER: [BackendKind; 5] = [
        BackendKind::Mlx,       // Primary: HKDF-seeded determinism
        BackendKind::CoreML,    // Fallback: ANE acceleration
        BackendKind::MlxBridge, // MoE fallback: Python subprocess
        BackendKind::Metal,     // GPU fallback
        BackendKind::CPU,       // Observability only
    ];
    &ORDER
}
```

---

## Backend Aliases

The `BackendKind` enum supports multiple string aliases for flexible configuration. All aliases are normalized (lowercased, hyphens/underscores removed) before matching.

### Alias Table

| Canonical   | Aliases                                  | Notes                         |
| ----------- | ---------------------------------------- | ----------------------------- |
| `auto`      | `autodev`, `auto_dev`, `default`         | Default behavior              |
| `coreml`    | `core-ml`, `ane`                         | ANE acceleration              |
| `mlx`       | `mlx-ffi`, `mlx_ffi`                     | Primary MLX backend           |
| `mlxbridge` | `mlx-bridge`, `mlx_bridge`, `subprocess` | Python subprocess bridge      |
| `metal`     | (none)                                   | GPU compute                   |
| `cpu`       | `cpu_only`, `cpu-only`                   | Not implemented for inference |

### Parsing Examples

```rust
use adapteros_core::backend::BackendKind;
use std::str::FromStr;

// All of these parse to BackendKind::Mlx
BackendKind::from_str("mlx")?;        // Canonical
BackendKind::from_str("mlx-ffi")?;    // Hyphenated alias
BackendKind::from_str("mlx_ffi")?;    // Underscored alias
BackendKind::from_str("MLX-FFI")?;    // Case insensitive

// All of these parse to BackendKind::MlxBridge
BackendKind::from_str("mlxbridge")?;    // Canonical
BackendKind::from_str("mlx-bridge")?;   // Hyphenated alias
BackendKind::from_str("mlx_bridge")?;   // Underscored alias
BackendKind::from_str("subprocess")?;   // Semantic alias

// All of these parse to BackendKind::Auto
BackendKind::from_str("auto")?;       // Canonical
BackendKind::from_str("autodev")?;    // Development alias
BackendKind::from_str("auto_dev")?;   // Underscored alias
BackendKind::from_str("default")?;    // Semantic alias
```

### Environment Variable Usage

```bash
# All equivalent - set MLX as backend
export AOS_MODEL_BACKEND=mlx
export AOS_MODEL_BACKEND=mlx-ffi
export AOS_MODEL_BACKEND=mlx_ffi

# All equivalent - set MLX Bridge for MoE models
export AOS_MODEL_BACKEND=mlxbridge
export AOS_MODEL_BACKEND=mlx-bridge
export AOS_MODEL_BACKEND=subprocess

# All equivalent - auto-selection
export AOS_MODEL_BACKEND=auto
export AOS_MODEL_BACKEND=default
```

### Error Handling

Invalid backend strings produce helpful error messages:

```
Error: Invalid backend 'unknown-backend'. Expected one of: auto, coreml, mlx, mlxbridge, metal, cpu
```

---

## Available Backends

### 1. CoreML (Apple Neural Engine)

**Status:** ✅ Production Ready
**Platform:** macOS only
**Features:** `coreml-backend` build flag required

**Characteristics:**

- **Hardware:** Apple Neural Engine (M1/M2/M3/M4)
- **Determinism:** Guaranteed deterministic when ANE is available
- **Performance:** 15.8 TOPS (M1), 17.0 TOPS (M2/M3/M4)
- **Power Efficiency:** 50% power reduction vs GPU
- **Use Case:** Production inference with audit trails, power-constrained deployments

**Compute Units:**

- `CpuOnly`: CPU fallback (not recommended)
- `CpuAndGpu`: GPU acceleration without ANE
- `CpuAndNeuralEngine`: ANE acceleration (recommended)
- `All`: Uses all available compute units

**Configuration:**

```toml
# configs/cp.toml
[coreml]
compute_preference = "cpu_and_ne"  # cpu_only, cpu_and_gpu, cpu_and_ne, all
production_mode = false  # Disable fallbacks in production
```

**Environment Variables:**

- `AOS_COREML_COMPUTE_PREFERENCE`: Override compute preference
- `AOS_COREML_PRODUCTION_MODE`: Enable production mode (no fallbacks)

**Documentation:** See [COREML_BACKEND.md](COREML_BACKEND.md)

---

### 2. MLX (Apple MLX Framework)

**Status:** ✅ Production Ready (Feature-Gated)
**Platform:** macOS (Apple Silicon)
**Features:** `multi-backend` + `mlx` build flags required

**Characteristics:**

- **Hardware:** Unified memory on Apple Silicon
- **Determinism:** HKDF-seeded RNG for reproducible inference/training
- **Performance:** GPU-accelerated, flexible framework
- **Power Efficiency:** Good (GPU-based)
- **Use Case:** Production inference, training, research workloads

**Features:**

- Multi-adapter routing with K-sparse selection
- Hot-swap adapter loading/unloading
- Circuit breaker pattern for resilience
- Unified memory tracking and GC hints
- Tokenizer integration
- MoE (Mixture of Experts) support via subprocess bridge

**Implementation:**

- **C++ FFI:** Uses the C++ MLX library with full feature set including LoRA adapters, hot-swap, and GPU sampling

**Memory Management:**

```bash
# Model cache budget (shared across backends)
export AOS_MODEL_CACHE_MAX_MB=8192  # 8GB cache
```

**Environment Variables:**

- `AOS_MODEL_PATH`: Path to MLX model directory (must contain config.json)
- `AOS_FUSION_INTERVAL_MODE`: Fusion strategy (`per_request`, `per_token`, `per_segment:N`)

**Documentation:** See [MLX_GUIDE.md](MLX_GUIDE.md)

---

### 2b. MLX Bridge (Subprocess Backend for MoE Models)

**Status:** ✅ Production Ready (Feature-Gated)
**Platform:** macOS (Apple Silicon) with Python 3.9+ and mlx-lm
**Features:** `mlx-bridge` build flag required

**Characteristics:**

- **Hardware:** Apple Silicon with Python/mlx-lm subprocess
- **Determinism:** Best-effort (Python subprocess has weaker determinism guarantees)
- **Performance:** Slightly slower than native MLX FFI due to subprocess overhead
- **Power Efficiency:** Moderate (GPU-based via Python)
- **Use Case:** MoE (Mixture of Experts) models that aren't supported by MLX FFI

**When to Use:**

- Models with `num_experts > 0` in config.json (e.g., Qwen3-30B MoE, Mixtral)
- Models with architecture names containing "Moe" or "Mixtral"
- When explicit `--backend mlx-bridge` is requested

**Auto-Selection:**
The backend factory automatically selects MLX Bridge for MoE models:

```rust
fn is_moe_model(model_path: &Path) -> bool {
    // Checks for num_experts, num_local_experts, or MoE architecture
}
```

**Configuration:**

```bash
# Environment variables for MLX Bridge
export MLX_BRIDGE_PYTHON_PATH=python3      # Custom Python path
export MLX_BRIDGE_TIMEOUT=300              # Request timeout in seconds
export MLX_BRIDGE_MAX_RESTARTS=3           # Max restart attempts
```

**Features:**

- Streaming token generation
- Automatic subprocess lifecycle management (spawn, health check, restart on failure)
- JSON protocol for request/response serialization
- Fallback to MLX FFI for non-MoE models when Python unavailable

**Python Requirements:**

```bash
pip install mlx-lm
```

**Health Check:**
The bridge performs periodic health checks and can automatically restart the Python subprocess on failure.

**Documentation:** See [MLX_GUIDE.md](MLX_GUIDE.md#mlx-bridge-subprocess-backend)

---

### 3. Metal (Apple Metal GPU)

**Status:** ✅ Implemented
**Platform:** macOS only
**Features:** Built-in (no feature flag required)

**Characteristics:**

- **Hardware:** Any macOS device with Metal GPU (Intel or Apple Silicon)
- **Determinism:** Guaranteed with precompiled shaders
- **Performance:** GPU-accelerated, parallel processing
- **Power Efficiency:** Moderate (GPU-based)
- **Use Case:** Legacy hardware (pre-M1), development/testing

**Features:**

- Precompiled Metal shaders (`.metallib`)
- Zero-copy unified memory on Apple Silicon
- Grouped Query Attention (GQA) support
- Model integrity verification

**Shader Compilation:**

```bash
# Metal shaders compiled at build time
xcrun -sdk macosx metal -c -std=metal3.1 kernels.metal -o kernels.air
xcrun -sdk macosx metallib kernels.air -o kernels.metallib
```

**Documentation:** See [METAL_BACKEND.md](METAL_BACKEND.md)

---

### 4. CPU (Fallback)

**Status:** ⚠️ Not Implemented for Inference
**Platform:** All platforms
**Features:** Built-in (observability only)

**Characteristics:**

- **Hardware:** CPU-only execution
- **Determinism:** N/A (not implemented)
- **Performance:** N/A (not implemented)
- **Use Case:** Training fallback when `require_gpu=false`

**Current Status:**

- CPU backend is listed in the fallback chain for observability
- Inference kernels are NOT implemented for CPU
- Selecting CPU explicitly returns an error: "CPU backend is not supported for inference kernels"

---

## Selection Strategies

The backend factory implements several selection strategies defined in `BackendStrategy`:

### 1. Auto Selection (Default)

**Priority Order:** MLX → CoreML → MlxBridge → Metal → CPU

```rust
pub fn auto_select_backend(capabilities: &BackendCapabilities) -> Result<BackendChoice>
```

**Selection Logic:**

1. **MLX**: If `multi-backend` feature enabled and `has_mlx`, select MLX
2. **CoreML**: If `has_coreml && has_ane`, select CoreML
3. **MlxBridge**: If `mlx-bridge` enabled and `has_mlx_bridge`, select MLX Bridge
4. **Metal**: If `has_metal`, select Metal
5. **CPU**: Terminal entry (returns error - not implemented)

**Usage:**

```rust
let backend = create_backend_with_model(BackendChoice::Auto, model_path)?;
```

**Environment Variable:**

```bash
# Auto-selection uses hardware detection
export AOS_BACKEND=auto  # or omit for default
```

---

### 2. MetalWithCoreMLFallback

**Primary:** Metal
**Fallback:** CoreML (if ANE available)

```rust
BackendStrategy::MetalWithCoreMLFallback => {
    if capabilities.has_metal {
        Ok(BackendChoice::Metal)
    } else if capabilities.has_coreml && capabilities.has_ane {
        Ok(BackendChoice::CoreML)
    } else {
        Err(AosError::Config("No suitable backend available".to_string()))
    }
}
```

**Use Case:** Prioritize GPU for maximum performance, fallback to ANE

---

### 3. CoreMLWithMetalFallback

**Primary:** CoreML
**Fallback:** Metal

```rust
BackendStrategy::CoreMLWithMetalFallback => {
    if capabilities.has_coreml && capabilities.has_ane {
        Ok(BackendChoice::CoreML)
    } else if capabilities.has_metal {
        Ok(BackendChoice::Metal)
    } else {
        Err(AosError::Config("No suitable backend available".to_string()))
    }
}
```

**Use Case:** Prioritize power efficiency (ANE), fallback to GPU

---

### 4. MlxPrimary

**Primary:** MLX (no fallback)

```rust
BackendStrategy::MlxPrimary => {
    if capabilities.has_mlx {
        Ok(BackendChoice::Mlx)
    } else {
        Err(AosError::Config("MLX backend not available".to_string()))
    }
}
```

**Use Case:** Force MLX for training/research workloads

---

### 5. MetalOnly

**Primary:** Metal (no fallback)

```rust
BackendStrategy::MetalOnly => {
    if capabilities.has_metal {
        Ok(BackendChoice::Metal)
    } else {
        Err(AosError::Config("Metal backend not available".to_string()))
    }
}
```

**Use Case:** Force Metal for development/testing

---

## Capability Detection

The backend factory performs runtime hardware detection via `detect_capabilities()` in `adapteros-lora-worker/src/backend_factory/capabilities.rs`.

### Detection Logic

```rust
pub struct BackendCapabilities {
    pub has_metal: bool,           // Metal GPU detected
    pub metal_device_name: Option<String>,
    pub has_ane: bool,             // Apple Neural Engine detected
    pub has_coreml: bool,          // CoreML framework available
    pub has_mlx: bool,             // MLX runtime initialized
    pub has_mlx_bridge: bool,      // Python/mlx-lm subprocess available
    pub gpu_memory_bytes: Option<u64>,
}
```

### Selection Context

Backend selection uses a `SelectionContext` that combines capabilities with execution profile:

```rust
pub struct SelectionContext {
    pub profile: ExecutionProfile,
    pub capabilities: BackendCapabilities,
}

pub struct BackendSelection {
    pub backend: BackendKind,
    pub override_reason: Option<String>,  // Audit trail for selection overrides
}
```

### Metal Detection

**Platform:** macOS only
**Method:** Query `metal::Device::system_default()`

```rust
#[cfg(target_os = "macos")]
fn detect_metal_device(caps: &mut BackendCapabilities) -> bool {
    if let Some(device) = Device::system_default() {
        caps.metal_device_name = Some(device.name().to_string());
        caps.gpu_memory_bytes = Some(device.recommended_max_working_set_size());
        true
    } else {
        false
    }
}
```

**Capabilities Detected:**

- Device name (e.g., "Apple M2")
- Recommended max working set size (GPU memory)

---

### CoreML/ANE Detection

**Platform:** macOS with `coreml-backend` feature
**Method:** Query CoreML framework for ANE availability

```rust
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
{
    caps.has_coreml = true;
    caps.has_ane = detect_neural_engine();
}
```

**ANE Detection:**

```rust
fn detect_neural_engine() -> bool {
    use adapteros_lora_kernel_coreml::is_neural_engine_available;
    is_neural_engine_available()
}
```

**Apple Silicon Detection:**

```rust
#[cfg(target_arch = "aarch64")]
fn is_apple_silicon() -> bool {
    true
}
```

---

### MLX Detection

**Platform:** macOS (Apple Silicon) with `multi-backend` + `mlx` features
**Method:** Initialize MLX runtime

```rust
#[cfg(feature = "multi-backend")]
{
    #[cfg(feature = "mlx")]
    {
        use adapteros_lora_mlx_ffi::{mlx_runtime_init, mlx_runtime_is_initialized};
        caps.has_mlx = mlx_runtime_is_initialized() || mlx_runtime_init().is_ok();
    }
}
```

**Note:** Without `mlx` feature flag, `has_mlx = false` (stub mode)

---

### MLX Bridge Detection

**Platform:** macOS (Apple Silicon) with `mlx-bridge` feature
**Method:** Check Python 3 and mlx-lm availability

```rust
#[cfg(feature = "mlx-bridge")]
fn detect_mlx_bridge_availability() -> bool {
    use std::process::Command;

    // Try to run python3 with a quick mlx-lm import check
    let result = Command::new("python3")
        .args(["-c", "import mlx_lm; print('ok')"])
        .output();

    match result {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}
```

**Requirements:**

- Python 3.9+
- `mlx-lm` package installed (`pip install mlx-lm`)
- `mlx-bridge` feature flag enabled at build time

**Environment Variables for Bridge:**

```bash
# Custom Python executable (default: python3)
export MLX_BRIDGE_PYTHON_PATH=/usr/bin/python3

# Path to bridge script (auto-detected if not set)
export MLX_BRIDGE_SCRIPT_PATH=/opt/aos/scripts/mlx_bridge_server.py
```

**Auto-Detection Locations:**
The bridge script is searched in the following order:

1. `MLX_BRIDGE_SCRIPT_PATH` environment variable
2. `scripts/mlx_bridge_server.py` relative to executable
3. `scripts/mlx_bridge_server.py` relative to working directory
4. `../scripts/mlx_bridge_server.py` and `../../scripts/mlx_bridge_server.py`

---

### Detection Logging

All capability detection results are logged:

```rust
debug!(
    has_metal = caps.has_metal,
    metal_device = ?caps.metal_device_name,
    has_ane = caps.has_ane,
    has_coreml = caps.has_coreml,
    has_mlx = caps.has_mlx,
    gpu_memory_mb = caps.gpu_memory_bytes.map(|b| b / BYTES_PER_MB),
    "Backend capabilities detected"
);
```

---

## Configuration & Environment Variables

### Configuration Precedence

Backend selection follows a strict precedence order:

```
CLI argument > Environment variable > Config file > Auto-selection
```

| Priority    | Source               | Example                       |
| ----------- | -------------------- | ----------------------------- |
| 1 (highest) | CLI argument         | `--backend mlx`               |
| 2           | Environment variable | `AOS_MODEL_BACKEND=mlx`       |
| 3           | Config file          | `[mlx] backend = "mlx"`       |
| 4 (lowest)  | Auto-selection       | Hardware capability detection |

**Example Precedence:**

```bash
# Config file sets CoreML
# configs/cp.toml: [model] backend = "coreml"

# Environment overrides to MLX
export AOS_MODEL_BACKEND=mlx

# CLI overrides to Metal (highest priority)
./aosctl serve --backend metal
# Result: Metal backend selected
```

---

### Backend Selection Environment Variable

#### AOS_MODEL_BACKEND

**Primary:** Preferred backend selection variable

```bash
export AOS_MODEL_BACKEND=mlx  # auto, coreml, mlx, mlxbridge, metal, cpu
```

**Parsed Values (with aliases):**

- `auto`, `autodev`, `auto_dev`, `default` -> `BackendKind::Auto`
- `coreml`, `core-ml`, `ane` -> `BackendKind::CoreML`
- `mlx`, `mlx-ffi`, `mlx_ffi` -> `BackendKind::Mlx`
- `mlxbridge`, `mlx-bridge`, `mlx_bridge`, `subprocess` -> `BackendKind::MlxBridge`
- `metal` -> `BackendKind::Metal`
- `cpu`, `cpu_only`, `cpu-only` -> `BackendKind::CPU`

---

### Config File Backend Section

The `[mlx]` section in `configs/cp.toml` configures MLX-specific settings:

```toml
# configs/cp.toml
[mlx]
# Model path (required for MLX)
model_path = "/var/model-cache/models/qwen2.5-7b-instruct-bf16"

# Fusion interval mode
fusion_mode = "per_request"  # per_request, per_token, per_segment:N

# Memory management
gc_threshold_mb = 1024
```

---

### Model Cache Configuration

**Required:** Model cache budget must be configured before any backend creation

```bash
# Environment variable (highest priority)
export AOS_MODEL_CACHE_MAX_MB=8192  # 8GB cache
```

```toml
# configs/cp.toml (fallback)
[model.cache]
max.mb = 8192  # 8GB cache
```

**Validation:**

```rust
pub fn validate_model_cache_budget() -> Result<u64>
```

**Error if Missing:**

```
Model cache budget not configured.

Configuration Status:
  - AOS_MODEL_CACHE_MAX_MB: not set
  - model.cache.max.mb (config): not set

How to fix:
  1. Set environment variable:
     export AOS_MODEL_CACHE_MAX_MB=8192  # For 8GB cache

  2. Or add to config file (configs/cp.toml or configs/aos.toml):
     [model.cache]
     max.mb = 8192  # For 8GB cache

Recommended minimums by model size:
  - 7B models (4-bit):   4096 MB (4GB)
  - 7B models (fp16):    16384 MB (16GB)
  - 13B models (4-bit):  8192 MB (8GB)
  - 32B+ models:         24576+ MB (24GB+)
```

---

### Backend Selection Environment Variables

#### AOS_BACKEND (Legacy)

**Deprecated:** Use `AOS_MODEL_BACKEND` instead (see [Configuration Precedence](#configuration-precedence))

```bash
export AOS_BACKEND=coreml  # auto, coreml, mlx, metal, cpu
```

**Note:** This variable is legacy and may be removed in future versions. Prefer `AOS_MODEL_BACKEND` for new deployments.

---

### CoreML-Specific Variables

```bash
# Compute preference
export AOS_COREML_COMPUTE_PREFERENCE=cpu_and_ne  # cpu_only, cpu_and_gpu, cpu_and_ne, all

# Production mode (disable fallbacks)
export AOS_COREML_PRODUCTION_MODE=true
```

---

### MLX-Specific Variables

```bash
# Model path (required for MLX)
export AOS_MODEL_PATH=/var/model-cache/models/qwen2.5-7b-instruct-bf16

# Fusion interval mode
export AOS_FUSION_INTERVAL_MODE=per_request  # per_request, per_token, per_segment:N
export AOS_FUSION_MODE=per_token  # Alias for AOS_FUSION_INTERVAL_MODE
```

---

### MLX Bridge-Specific Variables

```bash
# Python executable path (default: python3)
export MLX_BRIDGE_PYTHON_PATH=/usr/bin/python3

# Request timeout in seconds (default: 300)
export MLX_BRIDGE_TIMEOUT=300

# Maximum restart attempts on failure (default: 3)
export MLX_BRIDGE_MAX_RESTARTS=3

# Path to bridge script (auto-detected if not set)
export MLX_BRIDGE_SCRIPT_PATH=/opt/aos/scripts/mlx_bridge_server.py
```

**Note:** The MLX Bridge requires Python 3.9+ with `mlx-lm` package installed:

```bash
pip install mlx-lm
```

---

### Model Integrity Verification

```bash
# Skip model hash verification (development only)
export AOS_SKIP_MODEL_HASH_VERIFY=1

# Force model bytes verification (debug)
export AOS_VERIFY_MODEL_BYTES=1
```

---

### Config File Path

```bash
# Override config file location
export AOS_CONFIG_TOML=/path/to/custom/config.toml
```

**Default:** `configs/cp.toml`

---

## Decision Flowchart

```mermaid
flowchart TD
    Start([Backend Selection Request]) --> CheckExplicit{Explicit Backend<br/>Requested?}

    CheckExplicit -->|Yes| ValidateExplicit[Validate Requested Backend]
    CheckExplicit -->|No - Auto| AutoSelect[Auto Selection Logic]

    ValidateExplicit --> IsCoreML{CoreML<br/>Requested?}
    ValidateExplicit --> IsMetal{Metal<br/>Requested?}
    ValidateExplicit --> IsMLX{MLX<br/>Requested?}
    ValidateExplicit --> IsCPU{CPU<br/>Requested?}

    %% CoreML Path
    IsCoreML -->|Yes| CheckCoreMLAvail{CoreML Available?<br/>has_coreml &&<br/>has_ane}
    CheckCoreMLAvail -->|Yes| CoreMLBackend[✅ CoreML Backend]
    CheckCoreMLAvail -->|No| AutoFallback1[Try Auto Fallback Chain]
    AutoFallback1 --> CheckMLX1{MLX Available?}
    CheckMLX1 -->|Yes| MLXFallback1[✅ MLX Backend<br/>fallback_coreml_unavailable]
    CheckMLX1 -->|No| CheckMetal1{Metal Available?}
    CheckMetal1 -->|Yes| MetalFallback1[✅ Metal Backend<br/>fallback_coreml_unavailable]
    CheckMetal1 -->|No| ErrorCoreML[❌ Error: CoreML not available]

    %% Metal Path
    IsMetal -->|Yes| CheckMetalAvail{Metal Available?<br/>has_metal}
    CheckMetalAvail -->|Yes| MetalBackend[✅ Metal Backend]
    CheckMetalAvail -->|No| ErrorMetal[❌ Error: Metal not available]

    %% MLX Path
    IsMLX -->|Yes| CheckMLXAvail{MLX Available?<br/>multi-backend feature<br/>&& has_mlx}
    CheckMLXAvail -->|Yes| MLXBackend[✅ MLX Backend]
    CheckMLXAvail -->|No| ErrorMLX[❌ Error: MLX not available<br/>enable multi-backend]

    %% CPU Path
    IsCPU -->|Yes| ErrorCPU[❌ Error: CPU backend<br/>not supported for inference]

    %% Auto Selection Path
    AutoSelect --> Priority1{MLX Available?<br/>multi-backend &&<br/>has_mlx}
    Priority1 -->|Yes| MLXAuto[✅ MLX Backend<br/>GPU acceleration]
    Priority1 -->|No| Priority2{CoreML Available?<br/>has_coreml &&<br/>has_ane}
    Priority2 -->|Yes| CoreMLAuto[✅ CoreML Backend<br/>ANE acceleration]
    Priority2 -->|No| Priority3{MlxBridge Available?<br/>mlx-bridge &&<br/>has_mlx_bridge}
    Priority3 -->|Yes| MlxBridgeAuto[✅ MLX Bridge Backend<br/>MoE fallback]
    Priority3 -->|No| Priority4{Metal Available?<br/>has_metal}
    Priority4 -->|Yes| MetalAuto[✅ Metal Backend<br/>GPU acceleration]
    Priority4 -->|No| ErrorAuto[❌ Error: No suitable backend<br/>MLX → CoreML → MlxBridge → Metal → CPU]

    %% Backend Creation
    CoreMLBackend --> CreateCoreML[Create CoreML Backend]
    MLXBackend --> CreateMLX[Create MLX Backend]
    MLXFallback1 --> CreateMLX
    MetalBackend --> CreateMetal[Create Metal Backend]
    MetalFallback1 --> CreateMetal
    CoreMLAuto --> CreateCoreML
    MLXAuto --> CreateMLX
    MlxBridgeAuto --> CreateMLXBridge[Create MLX Bridge Backend]
    MetalAuto --> CreateMetal

    %% Cache Check
    CreateCoreML --> CheckCache1{Model in Cache?<br/>key: backend + manifest_hash<br/>+ quantization + fusion}
    CreateMLX --> CheckCache2{Model in Cache?}
    CreateMetal --> CheckCache3{Model in Cache?}

    CheckCache1 -->|Hit| ReuseModel1[Reuse Cached Model]
    CheckCache1 -->|Miss| LoadCoreML[Load CoreML Model<br/>+ Verify Integrity]

    CheckCache2 -->|Hit| ReuseModel2[Reuse Cached Model]
    CheckCache2 -->|Miss| LoadMLX[Load MLX Model<br/>+ Verify Integrity<br/>+ Init HKDF RNG]

    CheckCache3 -->|Hit| ReuseModel3[Reuse Cached Model]
    CheckCache3 -->|Miss| LoadMetal[Load Metal Model<br/>+ Verify Integrity<br/>+ Compile Shaders]

    ReuseModel1 --> Success[✅ Backend Ready]
    ReuseModel2 --> Success
    ReuseModel3 --> Success
    LoadCoreML --> CacheModel1[Cache Model]
    LoadMLX --> CacheModel2[Cache Model]
    LoadMetal --> CacheModel3[Cache Model]
    CacheModel1 --> Success
    CacheModel2 --> Success
    CacheModel3 --> Success

    %% Styling
    style CoreMLBackend fill:#d4edda
    style MLXBackend fill:#d4edda
    style MetalBackend fill:#fff3cd
    style CoreMLAuto fill:#d4edda
    style MLXAuto fill:#d4edda
    style MetalAuto fill:#fff3cd
    style Success fill:#d4edda
    style ErrorCoreML fill:#f8d7da
    style ErrorMetal fill:#f8d7da
    style ErrorMLX fill:#f8d7da
    style ErrorCPU fill:#f8d7da
    style ErrorAuto fill:#f8d7da
```

### Decision Tree Key

**Capability Checks:**

- `has_coreml && has_ane`: CoreML framework available AND Apple Neural Engine detected
- `has_mlx`: MLX runtime initialized (requires `multi-backend` + `mlx` features)
- `has_mlx_bridge`: Python/mlx-lm subprocess available (requires `mlx-bridge` feature)
- `has_metal`: Metal GPU device detected

**Cache Key Components:**

- `backend_type`: CoreML, MLX, MlxBridge, Metal
- `manifest_hash`: B3 hash of manifest JSON
- `quantization_mode`: Detected from config.json or backend-specific tag
- `fusion_mode`: Fusion interval strategy (`per_request`, `per_token`, `per_segment:N`)
- `kernel_version_id`: adapterOS version string
- `build_id`: adapterOS version (optional)

**Fallback Reasons:**

- `fallback_coreml_unavailable`: CoreML requested but not available, falling back
- `fallback_metal`: Generic Metal fallback

---

## Performance Characteristics

### Throughput Comparison

| Backend        | Hardware       | Typical Throughput | Latency (7B fp16) | Power Draw              |
| -------------- | -------------- | ------------------ | ----------------- | ----------------------- |
| **CoreML**     | M1 ANE         | 15.8 TOPS          | 40-60ms           | **Low** (50% reduction) |
| **CoreML**     | M2/M3/M4 ANE   | 17.0 TOPS          | 35-55ms           | **Low** (50% reduction) |
| **MLX**        | M1/M2 Unified  | Variable (GPU)     | 50-80ms           | Moderate                |
| **MLX Bridge** | M1/M2 + Python | Variable (GPU)     | 60-100ms          | Moderate                |
| **Metal**      | M1/M2 GPU      | Variable (GPU)     | 50-80ms           | Moderate                |
| **Metal**      | Intel GPU      | Variable (GPU)     | 80-120ms          | Moderate-High           |

**Note:** Throughput varies significantly based on model architecture, quantization, and batch size. MLX Bridge has ~10-20% higher latency due to subprocess overhead.

---

### Memory Footprint

| Backend        | Overhead      | Sharing                 | Notes                               |
| -------------- | ------------- | ----------------------- | ----------------------------------- |
| **CoreML**     | Low           | Per-model cache         | Compiled `.mlmodelc` cached on disk |
| **MLX**        | Moderate      | Unified memory          | Shares system RAM/GPU memory        |
| **MLX Bridge** | Moderate-High | Separate Python process | Extra overhead for subprocess + IPC |
| **Metal**      | Low-Moderate  | Arc-backed buffers      | Zero-copy on Apple Silicon          |

**Model Cache Budget:**

- Shared across all backends
- Cache key: `(backend_type, manifest_hash, quantization, fusion, kernel_version)`
- Eviction policy: LRU with memory budget enforcement

**Recommended Cache Budgets:**

```
7B models (4-bit):   4096 MB (4GB)
7B models (fp16):   16384 MB (16GB)
13B models (4-bit):  8192 MB (8GB)
32B+ models:        24576+ MB (24GB+)
```

---

### Determinism Guarantees

| Backend        | Determinism Level    | Seeding Method           | Use Case                        |
| -------------- | -------------------- | ------------------------ | ------------------------------- |
| **CoreML**     | ✅ Guaranteed (ANE)  | ANE hardware             | Audit trails, compliance        |
| **CoreML**     | ⚠️ Conditional (GPU) | GPU non-deterministic    | Fallback only                   |
| **MLX**        | ✅ HKDF-seeded       | Manifest hash → RNG seed | Training, reproducible research |
| **MLX Bridge** | ⚠️ Best-effort       | Python mlx-lm seeding    | MoE models                      |
| **Metal**      | ✅ Guaranteed        | Precompiled shaders      | Development, testing            |
| **CPU**        | ❌ N/A               | Not implemented          | N/A                             |

**HKDF-Seeded Determinism (MLX):**

```rust
// Manifest hash used as HKDF input key material (IKM)
let backend = MLXFFIBackend::with_manifest_hash_arc(model_arc, manifest_hash)?;
```

**Attestation:**

- All backends report determinism status via attestation API
- CoreML reports ANE usage flag
- MLX reports HKDF seed derivation
- MLX Bridge reports `deterministic: false` (Python subprocess has weaker guarantees)

---

### Startup Time

| Backend        | Cold Start | Warm Start (Cached) | Notes                              |
| -------------- | ---------- | ------------------- | ---------------------------------- |
| **CoreML**     | 2-5s       | 500ms-1s            | `.mlmodelc` compilation + ANE load |
| **MLX**        | 1-3s       | 200ms-500ms         | Model load + HKDF seed derivation  |
| **MLX Bridge** | 3-5s       | 1-2s                | Python subprocess + model load     |
| **Metal**      | 500ms-1s   | 100ms-300ms         | Shader compilation (precompiled)   |

**Cache Benefits:**

- Model cache eliminates redundant model loads
- Subsequent requests reuse cached models (O(1) lookup)
- Cache key identity ensures consistent reuse

---

## Use Case Recommendations

### Production Inference (Audit Trails)

**Recommended Backend:** CoreML
**Fallback:** MLX → Metal

**Rationale:**

- Guaranteed determinism with ANE
- 50% power savings
- Audit trail compatibility

**Configuration:**

```bash
export AOS_COREML_COMPUTE_PREFERENCE=cpu_and_ne
export AOS_COREML_PRODUCTION_MODE=true  # No fallbacks
export AOS_MODEL_CACHE_MAX_MB=16384     # 16GB cache
```

```toml
[coreml]
compute_preference = "cpu_and_ne"
production_mode = true

[model.cache]
max.mb = 16384
```

---

### Training & Research

**Recommended Backend:** MLX
**Fallback:** None (fail if MLX unavailable)

**Rationale:**

- HKDF-seeded determinism for reproducible training
- Unified memory architecture
- Circuit breaker resilience
- Multi-adapter routing

**Configuration:**

```bash
export AOS_BACKEND=mlx
export AOS_MODEL_PATH=/var/model-cache/models/qwen2.5-7b-instruct-bf16
export AOS_FUSION_INTERVAL_MODE=per_token
export AOS_MODEL_CACHE_MAX_MB=24576  # 24GB cache
```

**Build Flags:**

```bash
cargo build --features multi-backend,mlx
```

---

### Codebase Adapter Workflows

Codebase adapters require different backends depending on their state:

| Codebase Adapter State          | Recommended Backend | Rationale                                       |
| ------------------------------- | ------------------- | ----------------------------------------------- |
| **Live** (bound to session)     | MLX or Metal        | Receives incremental updates; requires hot-swap |
| **Frozen** (versioned, unbound) | CoreML (pre-fused)  | Stable state; benefits from ANE                 |

**Live Codebase Session:**

```bash
# Force MLX for live codebase sessions
export AOS_MODEL_BACKEND=mlx
export AOS_FORCE_BACKEND_FOR_CODEBASE=true
```

**Frozen Codebase Deployment:**

```bash
# Use CoreML for frozen codebase adapters
export AOS_MODEL_BACKEND=coreml
# Pre-fused package includes frozen codebase adapter
export AOS_MODEL_PATH=./fused-codebase.mlpackage
```

**Note:** The system automatically selects MLX/Metal when a live codebase adapter is detected, even if `AOS_MODEL_BACKEND=coreml` is set. See [Codebase Adapter Backend Override](#codebase-adapter-backend-override).

---

### Power-Constrained Deployment

**Recommended Backend:** CoreML
**Fallback:** None (fail if ANE unavailable)

**Rationale:**

- ANE provides 50% power reduction vs GPU
- Lower thermal footprint
- Longer battery life on mobile deployments

**Configuration:**

```bash
export AOS_COREML_COMPUTE_PREFERENCE=cpu_and_ne
export AOS_COREML_PRODUCTION_MODE=true
```

---

### Legacy Hardware (Intel Macs)

**Recommended Backend:** Metal
**Fallback:** None

**Rationale:**

- Only GPU backend available on Intel Macs
- No ANE support
- CoreML/MLX require Apple Silicon

**Configuration:**

```bash
export AOS_BACKEND=metal
export AOS_MODEL_CACHE_MAX_MB=8192  # 8GB cache
```

---

### Development & Testing

**Recommended Backend:** Metal or Auto
**Fallback:** Auto selection chain

**Rationale:**

- Fast iteration
- Precompiled shaders for consistent results
- Auto-selection tests fallback logic

**Configuration:**

```bash
export AOS_BACKEND=auto  # Test auto-selection
export AOS_MODEL_CACHE_MAX_MB=4096  # 4GB cache (smaller for testing)
```

---

### MoE Models (Mixture of Experts)

**Recommended Backend:** MLX (subprocess bridge)
**Fallback:** None

**Rationale:**

- MoE models detected from `config.json` (`num_experts > 1`)
- Automatic subprocess bridge creation
- Python bridge for complex routing logic

**Detection:**

```rust
fn is_moe_model(model_path: &Path) -> bool {
    // Checks for num_experts field in config.json
}
```

**Configuration:**

```bash
export AOS_BACKEND=mlx
export AOS_MODEL_PATH=/var/model-cache/models/qwen3-30b-moe
```

---

## Troubleshooting

### Backend Selection Errors

#### Error: "No suitable backend available"

**Cause:** Auto-selection exhausted all options (MLX → CoreML → MlxBridge → Metal → CPU)

**Solution:**

1. Check hardware capabilities:

   ```bash
   # Look for capability detection logs
   grep "Backend capabilities detected" logs/worker.log
   ```

2. Verify build features:

   ```bash
   cargo build --features coreml-backend,multi-backend,mlx
   ```

3. Check macOS version:
   - CoreML: macOS 13+ (MLTensor API requires macOS 15+)
   - Metal: macOS 10.11+
   - MLX: macOS 13+ (Apple Silicon only)

---

#### Error: "CoreML backend not available (ANE/CoreML missing)"

**Cause:** CoreML requested but ANE not detected or `coreml-backend` feature not enabled

**Solution:**

1. Check for Apple Silicon:

   ```bash
   uname -m  # Should show "arm64"
   ```

2. Verify build features:

   ```bash
   cargo build --features coreml-backend
   ```

3. Check ANE availability:
   ```bash
   # Check system_profiler for Neural Engine
   system_profiler SPHardwareDataType | grep "Chip"
   ```

---

#### Error: "MLX backend not available (enable multi-backend)"

**Cause:** MLX requested but `multi-backend` or `mlx` feature not enabled

**Solution:**

```bash
cargo build --features multi-backend,mlx
```

**Verify MLX runtime:**

```bash
# Check MLX initialization logs
grep "MLX runtime" logs/worker.log
```

---

#### Error: "Metal backend not available"

**Cause:** Metal requested but no Metal GPU detected

**Solution:**

1. Check for Metal support:

   ```bash
   # macOS only
   system_profiler SPDisplaysDataType | grep "Metal"
   ```

2. Verify macOS version (Metal requires 10.11+)

---

#### Error: "Model cache budget not configured"

**Cause:** Neither `AOS_MODEL_CACHE_MAX_MB` nor `model.cache.max.mb` is set

**Solution:**

```bash
# Option 1: Environment variable
export AOS_MODEL_CACHE_MAX_MB=8192

# Option 2: Config file
cat >> configs/cp.toml <<EOF
[model.cache]
max.mb = 8192
EOF
```

**Recommended Budgets:**

- 7B models (4-bit): 4096 MB
- 7B models (fp16): 16384 MB
- 13B models (4-bit): 8192 MB
- 32B+ models: 24576+ MB

---

### Model Loading Errors

#### Error: "Failed to load MLX model from '...': ..."

**Cause:** MLX model path invalid or missing `config.json`

**Solution:**

1. Verify model directory structure:

   ```bash
   ls -la $AOS_MODEL_PATH
   # Should contain: config.json, model-*.safetensors or model.safetensors.index.json
   ```

2. Validate config.json:

   ```bash
   jq . $AOS_MODEL_PATH/config.json
   ```

3. Check model path security:
   ```bash
   # Model path must NOT be in /tmp (rejected by security policy)
   realpath $AOS_MODEL_PATH
   ```

---

#### Error: "Model config validation failed: GQA heads mismatch"

**Cause:** Grouped Query Attention (GQA) configuration invalid (`num_attention_heads` not divisible by `num_key_value_heads`)

**Solution:**

1. Verify config.json:

   ```bash
   jq '{num_attention_heads, num_key_value_heads}' $AOS_MODEL_PATH/config.json
   ```

2. Expected invariant:

   ```
   num_attention_heads % num_key_value_heads == 0
   ```

3. **This is a FATAL error** (BREAKING CHANGE from prior behavior)
   - Invalid GQA config prevents backend creation
   - Fix the model config or use a different model

---

#### Error: "Cache corruption detected"

**Cause:** Model integrity verification failed (hash mismatch)

**Solution:**

1. Re-download the model:

   ```bash
   rm -rf $AOS_MODEL_PATH
   # Re-download from source
   ```

2. Skip verification (development only):

   ```bash
   export AOS_SKIP_MODEL_HASH_VERIFY=1
   ```

3. Check disk corruption:
   ```bash
   # macOS: Disk Utility → First Aid
   diskutil verifyVolume /
   ```

---

### Performance Issues

#### High Memory Usage

**Cause:** Model cache budget too large or too many models cached

**Solution:**

1. Reduce cache budget:

   ```bash
   export AOS_MODEL_CACHE_MAX_MB=4096  # Reduce to 4GB
   ```

2. Check cache statistics:

   ```bash
   grep "Model cache" logs/worker.log
   ```

3. Clear cache and restart worker:
   ```bash
   # Cache is in-memory per worker - restart to clear
   pkill aos_worker
   ```

---

#### Slow Inference (First Request)

**Cause:** Cold start overhead (model loading, compilation)

**Solution:**

1. **CoreML:** Pre-compile `.mlmodelc`:

   ```bash
   # CoreML compiles on first use
   # Subsequent requests use cached .mlmodelc
   ```

2. **Metal:** Shaders are precompiled at build time (no action needed)

3. **MLX:** Pre-load model at startup:
   ```bash
   # Model cache persists across requests
   # First request loads, subsequent requests reuse
   ```

---

#### Slow Inference (All Requests)

**Cause:** Backend fallback to CPU, insufficient GPU memory, or quantization mismatch

**Solution:**

1. Check backend selection:

   ```bash
   grep "selected" logs/worker.log
   # Should show: "Auto-selected CoreML backend with Neural Engine"
   # NOT: "fallback_coreml_unavailable"
   ```

2. Verify ANE usage (CoreML):

   ```bash
   grep "ane_used" logs/worker.log
   # Should show: ane_used=true
   ```

3. Check GPU memory:
   ```bash
   # macOS Activity Monitor → GPU Memory tab
   ```

---

### Debugging Tips

#### Enable Verbose Logging

```bash
export RUST_LOG=debug
export RUST_BACKTRACE=1
```

#### Check Capability Detection

```bash
grep "Backend capabilities detected" logs/worker.log
```

**Expected Output:**

```
has_metal=true, metal_device="Apple M2", has_ane=true, has_coreml=true, has_mlx=true, gpu_memory_mb=8192
```

#### Trace Backend Creation

```bash
grep "Creating.*kernel backend" logs/worker.log
```

**Expected Output:**

```
Creating CoreML kernel backend: compute_preference=cpu_and_ne, production_mode=false, gpu_available=true, ane_available=true, gpu_used=false, ane_used=true
```

#### Verify Model Cache

```bash
grep "Model cache" logs/worker.log
```

**Expected Output:**

```
Initializing per-worker model cache with explicit budget: max_memory_mb=8192
Model cache budget validated: budget_mb=8192, source=AOS_MODEL_CACHE_MAX_MB
```

---

## Related Documentation

- [COREML_BACKEND.md](COREML_BACKEND.md) - CoreML/ANE backend guide
- [METAL_BACKEND.md](METAL_BACKEND.md) - Metal GPU backend guide
- [MLX_GUIDE.md](MLX_GUIDE.md) - MLX backend guide
- [ARCHITECTURE.md](ARCHITECTURE.md) - System architecture overview
- [CONFIGURATION.md](CONFIGURATION.md) - Configuration reference
- [DETERMINISM.md](DETERMINISM.md) - Determinism guarantees
- [TROUBLESHOOTING.md](TROUBLESHOOTING.md) - General troubleshooting

---

## Implementation Reference

**Primary Module:**

- `/Users/mln-dev/Dev/adapter-os/crates/adapteros-lora-worker/src/backend_factory.rs`

**Key Types:**

```rust
// Backend choice (canonical)
pub type BackendChoice = adapteros_core::backend::BackendKind;

// Selection strategy
pub enum BackendStrategy {
    MetalWithCoreMLFallback,
    CoreMLWithMetalFallback,
    MlxPrimary,
    MetalOnly,
}

// Capability detection
pub struct BackendCapabilities {
    pub has_metal: bool,
    pub metal_device_name: Option<String>,
    pub has_ane: bool,
    pub has_coreml: bool,
    pub has_mlx: bool,
    pub has_mlx_bridge: bool,
    pub gpu_memory_bytes: Option<u64>,
}

// Selection context
pub struct SelectionContext {
    pub profile: ExecutionProfile,
    pub capabilities: BackendCapabilities,
}
```

**Key Functions:**

```rust
// Capability detection
pub fn detect_capabilities() -> BackendCapabilities

// Auto-selection
pub fn auto_select_backend(capabilities: &BackendCapabilities) -> Result<BackendChoice>

// Context-aware selection
pub fn select_backend_from_execution_profile(
    context: &SelectionContext,
) -> Result<BackendSelection>

// Backend creation
pub fn create_backend_from_config(config: &ModelConfig) -> Result<KernelBox>
pub fn create_backend_with_model(choice: BackendChoice, model_path: &Path) -> Result<KernelBox>
pub fn create_backend_with_model_hashes(
    choice: BackendChoice,
    model_path: &Path,
    manifest_hash: Option<&B3Hash>,
    model_weights_hash: Option<&B3Hash>,
) -> Result<KernelBox>
```

---

**End of Backend Selection Guide**
