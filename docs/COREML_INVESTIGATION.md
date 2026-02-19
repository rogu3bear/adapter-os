# CoreML Investigation Summary

> **Snapshot** — Investigation from 2026-02-02. Build/test status may have changed. Re-verify.

**Date:** 2026-02-02  
**Platform:** macOS 26.2 (Build 25C56)
**Hardware**: Apple M4 Max
**Rust toolchain**: stable
**Feature flags**: `coreml-backend` (default enabled)

## Investigation Goal

Find why CoreML isn't loading or running correctly (inference, training, or export).

## Build Status

**PASS** - CoreML components compile successfully:
- `adapteros-lora-kernel-coreml`: Compiles with Objective-C++ and Swift bridges
- Swift bridge: `swift/CoreMLBridge.swift` compiles and links
- C++ bridge: `src/coreml_bridge.mm` compiles with `-fno-fast-math` flag

Build output confirms:
```
CoreML bridge compiled successfully
Swift CoreML bridge compiled successfully
```

## Test Results

All CoreML-related tests pass:

| Test Suite | Tests | Status |
|------------|-------|--------|
| `adapteros-lora-kernel-coreml` lib | 4 tests | PASS |
| `adapteros-lora-kernel-coreml` integration | 1 test | PASS |
| `adapteros-lora-kernel-coreml` aos_loader | 49 tests | PASS |
| `adapteros-lora-worker` CoreML tests | 37 tests | PASS |

## CoreML Initialization Path

The initialization follows this flow:

1. **Availability Check** (`lib.rs:2478-2486`):
   ```rust
   pub fn is_coreml_available() -> bool {
       #[cfg(target_os = "macos")]
       { unsafe { ffi::coreml_is_available() } }
   }
   ```

2. **FFI Bridge** (`coreml_bridge.mm:27-32`):
   ```objc
   bool coreml_is_available() {
       if (@available(macOS 10.13, *)) { return true; }
       return false;
   }
   ```

3. **Runtime Init** (`lib.rs:2489-2495`):
   ```rust
   pub fn init_coreml() -> Result<()> {
       if !is_coreml_available() {
           return Err(AosError::Kernel("CoreML not available".to_string()));
       }
       tracing::info!("CoreML runtime initialized");
       Ok(())
   }
   ```

## Backend Factory Integration

`backend_factory.rs:252-292` handles CoreML backend creation:

1. Calls `init_coreml()` to verify availability
2. Resolves settings via `resolve_coreml_backend_settings()`
3. Loads model config from `config.json`
4. Creates `CoreMLBackend` with compute units and production mode

**Production mode constraint** (lines 287-292):
```rust
if settings.production_mode && !settings.ane_used {
    return Err(AosError::Config(
        "CoreML production mode requires deterministic ANE-only compute units; ANE unavailable or not selected"
    ));
}
```

## Failure Points Identified

### 1. Init Phase (`init_coreml()`)
- **Error**: `AosError::Kernel("CoreML not available")`
- **Cause**: FFI `coreml_is_available()` returns false
- **Check**: Requires macOS 10.13+. Current system is macOS 26.2 (PASS)

### 2. Backend Factory Config
- **Error**: `"CoreML production mode requires deterministic ANE-only compute units"`
- **Cause**: `production_mode=true` but ANE not available or compute units mismatch
- **Current config**: No `[coreml]` section in `configs/cp.toml`, defaults to:
  - `compute_preference = "cpu_and_gpu"`
  - `production_mode = false`

### 3. Model Load (`coreml_load_model`)
- **Error**: Null handle returned, error in `g_last_error`
- **Cause**: Invalid model path, missing `.mlpackage`/`.mlmodelc`, or model format issue
- **Location**: `coreml_bridge.mm:157-198`

### 4. Training Preprocessing
- **Error**: `"Failed to load CoreML model: {message}"`
- **Location**: `preprocessing.rs:1336-1357`
- **Cause**: Model path doesn't point to valid CoreML model

### 5. Inference Runtime
- **Error codes**: -1 (null handle), -2 (input array), -3 (feature provider), -4 (prediction), -5 (output not found)
- **Location**: `coreml_bridge.mm:216-283`

## System Capabilities

The system has full CoreML support:
- **CPU**: Apple M4 Max (ANE generation 7)
- **GPU**: Metal 4 support
- **ANE**: 38 TOPS
- **macOS SDK**: 26.0+ (Tahoe) - supports `MLComputePolicy`

Swift bridge detects capabilities via:
```swift
// coreml_check_ane() returns generation=7 for M4
// get_system_capabilities() returns bitmask with GPU + ANE bits set
```

## Current Status

**CoreML framework availability: WORKING**

The issue is likely NOT at the init/availability level. Based on investigation:

1. FFI compiles and links correctly
2. Swift bridge compiles
3. Unit tests pass
4. System has Apple M4 Max with full CoreML/ANE support

### Potential Issues for Actual Inference/Training

1. **Missing CoreML model**: The system requires a pre-exported `.mlpackage` or `.mlmodelc` model
2. **Model path resolution**: `resolve_coreml_model_path()` in preprocessing needs valid path
3. **Config mismatch**: No `[coreml]` section in config - using defaults

## Configuration Options

Add to `configs/cp.toml`:
```toml
[coreml]
# Options: cpu_only, cpu_and_gpu, cpu_and_ne, all
compute_preference = "cpu_and_ne"
# Enable for production (enforces ANE-only)
production_mode = false
```

Or via environment:
```bash
AOS_COREML_COMPUTE_PREFERENCE=cpu_and_ne
AOS_COREML_PRODUCTION_MODE=false
```

## Likely Cause Summary

**No actual failure found at CoreML framework level.**

The CoreML subsystem appears functional. Any runtime failures would be due to:
1. Missing or invalid CoreML model files (`.mlpackage`/`.mlmodelc`)
2. Incorrect model path configuration
3. Production mode enabled without ANE-compatible compute units

## Next Steps to Reproduce Failure

1. Start server: `./start backend`
2. Trigger training with CoreML preprocessing
3. Check logs for specific error messages
4. Verify model path contains valid CoreML model

## Files Examined

- `crates/adapteros-lora-kernel-coreml/src/lib.rs` - Main CoreML API
- `crates/adapteros-lora-kernel-coreml/src/ffi.rs` - FFI declarations
- `crates/adapteros-lora-kernel-coreml/src/coreml_bridge.mm` - Obj-C++ bridge
- `crates/adapteros-lora-kernel-coreml/swift/CoreMLBridge.swift` - Swift bridge
- `crates/adapteros-lora-kernel-coreml/build.rs` - Build configuration
- `crates/adapteros-lora-worker/src/backend_factory.rs` - Backend creation
- `crates/adapteros-lora-worker/src/training/preprocessing.rs` - Training preprocessing
- `crates/adapteros-lora-kernel-mtl/src/coreml_backend.rs` - CoreML backend in MTL crate
- `configs/cp.toml` - Server configuration
