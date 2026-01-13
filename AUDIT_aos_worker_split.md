# Audit Report: aos_worker.rs Module Split Verification

**File:** `crates/adapteros-lora-worker/src/bin/aos_worker.rs`  
**Lines:** 2,876  
**Date:** Audit performed

---

## 1. Major Sections Inventory

### 1.1 Struct Definitions

| Line      | Name                                   | Purpose                             |
| --------- | -------------------------------------- | ----------------------------------- |
| 77-81     | `RegistrationResult`                   | Control plane registration response |
| 83-95     | `RegistrationParams<'a>`               | Parameters for CP registration      |
| 436-440   | `CoremlVerifyMode` (cfg-gated)         | CoreML verification mode enum       |
| 587-593   | `CoremlVerificationStatus` (cfg-gated) | CoreML verification status enum     |
| 697-701   | `LoadedManifest`                       | Manifest loading result wrapper     |
| 1039-1043 | `WorkerIdentity`                       | Worker identity for panic hook      |
| 1164-1228 | `Args`                                 | CLI argument parsing (clap)         |

### 1.2 Enum Definitions

| Line    | Name                       | Purpose                                    |
| ------- | -------------------------- | ------------------------------------------ |
| 436-440 | `CoremlVerifyMode`         | CoreML verification mode (Off/Warn/Strict) |
| 587-593 | `CoremlVerificationStatus` | CoreML verification result status          |

### 1.3 Impl Blocks

| Line    | Type                       | Methods                     |
| ------- | -------------------------- | --------------------------- |
| 596-610 | `CoremlVerificationStatus` | `as_str()`, `is_mismatch()` |

### 1.4 Public Functions

| Line | Name     | Signature                 | Purpose     |
| ---- | -------- | ------------------------- | ----------- |
| 1266 | `main()` | `fn main() -> Result<()>` | Entry point |

### 1.5 Private Helper Functions

#### Registration Functions

| Line    | Name                            | Purpose                               |
| ------- | ------------------------------- | ------------------------------------- |
| 101-171 | `register_with_cp()`            | Single registration attempt           |
| 191-303 | `register_with_cp_with_retry()` | Registration with exponential backoff |
| 306-342 | `notify_cp_status()`            | Status notification to CP             |

#### Manifest Functions

| Line    | Name                       | Purpose                        |
| ------- | -------------------------- | ------------------------------ |
| 345-354 | `parse_manifest()`         | Parse YAML/JSON manifest       |
| 357-403 | `fetch_manifest_from_cp()` | Fetch manifest from CP by hash |
| 406-432 | `cache_manifest()`         | Cache manifest locally         |

#### CoreML Functions (cfg-gated for macOS)

| Line      | Name                                   | Purpose                                    |
| --------- | -------------------------------------- | ------------------------------------------ |
| 443-453   | `resolve_coreml_verify_mode()`         | Parse CoreML verify mode from env          |
| 456-470   | `coreml_manifest_path()`               | Resolve CoreML manifest path               |
| 473-478   | `compute_coreml_package_hash()`        | Compute hash of CoreML package             |
| 481-496   | `expected_coreml_hash_from_metadata()` | Extract expected hash from metadata        |
| 499-519   | `resolve_fusion_ids()`                 | Extract base/adapter IDs from manifest     |
| 522-583   | `resolve_expected_coreml_hash()`       | Resolve expected hash (async, DB-aware)    |
| 613-696   | `log_coreml_verification_result()`     | Log verification result with mode handling |
| 894-901   | `render_coreml_compute_units()`        | Format compute units as string             |
| 904-910   | `coreml_effective_compute_units()`     | Determine effective compute units          |
| 913-932   | `coreml_telemetry_from_settings()`     | Build telemetry from settings              |
| 935-943   | `coreml_device_label()`                | Get device label (ane/gpu/cpu)             |
| 946-981   | `coreml_fallback_reason()`             | Determine fallback reason                  |
| 984-1008  | `log_coreml_runtime()`                 | Log CoreML runtime selection               |
| 1011-1036 | `run_coreml_boot_smoke()`              | Run smoke test for CoreML backend          |

#### Backend Functions

| Line    | Name                            | Purpose                        |
| ------- | ------------------------------- | ------------------------------ |
| 703-712 | `validate_backend_feature()`    | Validate backend feature flags |
| 715-725 | `parse_backend_choice()`        | Parse backend string to enum   |
| 727-729 | `is_mock_backend()`             | Check if backend is mock       |
| 749-789 | `detect_capabilities()`         | Detect backend capabilities    |
| 791-829 | `build_capabilities_detail()`   | Build detailed capabilities    |
| 831-842 | `mock_capabilities_detail()`    | Build mock capabilities        |
| 844-891 | `setup_mock_base_model_cache()` | Setup mock backend cache       |

#### Helper/Utility Functions

| Line      | Name                          | Purpose                                |
| --------- | ----------------------------- | -------------------------------------- |
| 731-746   | `dev_no_auth_enabled()`       | Check dev no-auth flag                 |
| 1046-1145 | `setup_panic_hook()`          | Install panic hook for fatal reporting |
| 1147-1158 | `shutdown_worker_telemetry()` | Shutdown telemetry writer              |
| 1243-1257 | `error_to_exit_code()`        | Map error to exit code                 |
| 1259-1264 | `is_prod_runtime()`           | Check production runtime mode          |
| 2368-2391 | `join_task_with_timeout()`    | Join task with timeout                 |

#### Main Initialization Functions

| Line      | Name           | Purpose                                 |
| --------- | -------------- | --------------------------------------- |
| 1266-1346 | `main()`       | Entry point, tokio runtime setup        |
| 1348-2366 | `run_worker()` | Main worker initialization and run loop |

### 1.6 Constants

| Line | Name                   | Value | Purpose                     |
| ---- | ---------------------- | ----- | --------------------------- |
| 68   | `SCHEMA_VERSION`       | "1.0" | Registration schema version |
| 69   | `API_VERSION`          | "1.0" | Registration API version    |
| 1237 | `EXIT_SUCCESS`         | 0     | Success exit code           |
| 1238 | `EXIT_CONFIG_ERROR`    | 1     | Config error exit code      |
| 1239 | `EXIT_TRANSIENT_ERROR` | 2     | Transient error exit code   |
| 1240 | `EXIT_FATAL_ERROR`     | 3     | Fatal error exit code       |

### 1.7 Static Variables

| Line | Name               | Type                        | Purpose                                |
| ---- | ------------------ | --------------------------- | -------------------------------------- |
| 73   | `WORKER_IDENTITY`  | `OnceLock<WorkerIdentity>`  | Global worker identity for panic hook  |
| 74   | `WORKER_TELEMETRY` | `OnceLock<TelemetryWriter>` | Global telemetry writer for panic hook |

---

## 2. Proposed Module Split Verification

### 2.1 `mod.rs` - Re-exports

**Status:** ✅ Valid  
**Contents:**

- Re-export all public items from submodules
- Keep `main()` function here (or re-export from `init.rs`)

### 2.2 `cli.rs` - CLI Argument Parsing

**Status:** ✅ Valid  
**Contents:**

- `Args` struct (lines 1164-1228)
- All clap-related code
- Exit code constants (lines 1237-1240)
- `error_to_exit_code()` function (lines 1243-1257)

**Dependencies:**

- `clap::Parser`
- Various config functions from `adapteros_config`

### 2.3 `init.rs` - Main Initialization Flow

**Status:** ✅ Valid  
**Contents:**

- `main()` function (lines 1266-1346)
- `run_worker()` function (lines 1348-2366)
- `join_task_with_timeout()` helper (lines 2368-2391)
- `is_prod_runtime()` helper (lines 1259-1264)

**Dependencies:**

- All other modules
- Orchestrates the initialization sequence

**Initialization Order (from `run_worker()`):**

1. Initialize tracing (line 1350)
2. Parse CLI args (line 1358)
3. Load .env (line 1361)
4. Validate cache budget (line 1367)
5. Setup panic hook (lines 1394-1408)
6. Resolve UDS path (line 1411)
7. Resolve model/tokenizer paths (lines 1441-1454)
8. Load manifest (lines 1457-1560) → **manifest.rs**
9. Configure backend (lines 1625-1798) → **backend.rs**
10. CoreML verification (lines 1800-1891) → **coreml.rs**
11. Initialize telemetry (lines 1893-1913) → **helpers.rs**
12. Register with CP (lines 1927-2010) → **registration.rs**
13. Create worker instance (lines 2026-2069)
14. Start UDS server (lines 2118-2201)
15. Start health monitor (lines 2218-2262)
16. Run server loop (lines 2276-2315)
17. Shutdown (lines 2323-2365)

### 2.4 `registration.rs` - Control Plane Registration

**Status:** ✅ Valid  
**Contents:**

- `RegistrationResult` struct (lines 77-81)
- `RegistrationParams` struct (lines 83-95)
- `register_with_cp()` function (lines 101-171)
- `register_with_cp_with_retry()` function (lines 191-303)
- `notify_cp_status()` function (lines 306-342)
- Constants: `SCHEMA_VERSION`, `API_VERSION` (lines 68-69)

**Dependencies:**

- `adapteros_api_types::workers::WorkerCapabilities`
- `ureq` for HTTP client
- `serde_json` for JSON handling

**Notes:**

- Retry logic with exponential backoff (lines 196-201)
- Circuit breaker after 5 consecutive failures (line 201)
- Transient vs non-transient error classification (lines 248-254)

### 2.5 `manifest.rs` - Manifest Loading/Parsing

**Status:** ✅ Valid  
**Contents:**

- `LoadedManifest` struct (lines 697-701)
- `parse_manifest()` function (lines 345-354)
- `fetch_manifest_from_cp()` function (lines 357-403)
- `cache_manifest()` function (lines 406-432)

**Dependencies:**

- `adapteros_manifest::ManifestV3`
- `adapteros_core::B3Hash`
- `serde_yaml`, `serde_json`
- `adapteros_config::resolve_manifest_cache_dir()`

**Notes:**

- Supports both YAML and JSON parsing
- Hash verification after fetch
- Local caching for reuse

### 2.6 `backend.rs` - Backend Selection & Init

**Status:** ✅ Valid  
**Contents:**

- `validate_backend_feature()` function (lines 703-712)
- `parse_backend_choice()` function (lines 715-725)
- `is_mock_backend()` function (lines 727-729)
- `detect_capabilities()` function (lines 749-789)
- `build_capabilities_detail()` function (lines 791-829)
- `mock_capabilities_detail()` function (lines 831-842)
- `setup_mock_base_model_cache()` function (lines 844-891)
- `dev_no_auth_enabled()` helper (lines 731-746)

**Dependencies:**

- `adapteros_lora_worker::backend_factory::*`
- `adapteros_lora_kernel_api::BackendType`
- `adapteros_api_types::workers::WorkerCapabilities`
- Feature flags: `#[cfg(feature = "multi-backend")]`

**Notes:**

- Handles mock backend setup
- Backend capability detection
- Feature flag validation

### 2.7 `coreml.rs` - CoreML Verification (cfg-gated)

**Status:** ✅ Valid (with caveats)  
**Contents:**

- `CoremlVerifyMode` enum (lines 436-440)
- `CoremlVerificationStatus` enum (lines 587-593)
- `CoremlVerificationStatus` impl (lines 596-610)
- `resolve_coreml_verify_mode()` function (lines 443-453)
- `coreml_manifest_path()` function (lines 456-470)
- `compute_coreml_package_hash()` function (lines 473-478)
- `expected_coreml_hash_from_metadata()` function (lines 481-496)
- `resolve_fusion_ids()` function (lines 499-519)
- `resolve_expected_coreml_hash()` async function (lines 522-583)
- `log_coreml_verification_result()` function (lines 613-696)
- `render_coreml_compute_units()` function (lines 894-901)
- `coreml_effective_compute_units()` function (lines 904-910)
- `coreml_telemetry_from_settings()` function (lines 913-932)
- `coreml_device_label()` function (lines 935-943)
- `coreml_fallback_reason()` function (lines 946-981)
- `log_coreml_runtime()` function (lines 984-1008)
- `run_coreml_boot_smoke()` function (lines 1011-1036)

**Dependencies:**

- `#[cfg(all(target_os = "macos", feature = "coreml-backend"))]`
- `adapteros_db::Db` (for fusion pair lookup)
- `adapteros_lora_kernel_coreml::*`
- `adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing}`

**Notes:**

- Entire module is cfg-gated for macOS + coreml-backend feature
- Some functions are used in `init.rs` during backend setup (lines 1686-1726, 1749-1761, 1805-1891)
- DB integration for fusion pair registry
- Hash verification with multiple fallback sources

### 2.8 `helpers.rs` - Panic Hook, Telemetry, Capabilities

**Status:** ✅ Valid  
**Contents:**

- `WorkerIdentity` struct (lines 1039-1043)
- Static: `WORKER_IDENTITY` (line 73)
- Static: `WORKER_TELEMETRY` (line 74)
- `setup_panic_hook()` function (lines 1046-1145)
- `shutdown_worker_telemetry()` function (lines 1147-1158)

**Dependencies:**

- `adapteros_telemetry::TelemetryWriter`
- `adapteros_core::identity::IdentityEnvelope`
- `adapteros_telemetry::unified_events::*`
- `adapteros_boot::panic_utils::*`

**Notes:**

- Panic hook uses static `OnceLock` for global state
- Telemetry shutdown with timeout
- Fatal error reporting to control plane

---

## 3. Cfg-Gated Code Sections

### 3.1 macOS + CoreML Backend (`#[cfg(all(target_os = "macos", feature = "coreml-backend"))]`)

**Imports (lines 56-65):**

- `adapteros_db::{CreateCoremlFusionPairParams, Db}`
- `adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing}`
- `adapteros_lora_kernel_coreml::export::validate_coreml_fusion`
- `adapteros_lora_kernel_coreml::ComputeUnits`
- `adapteros_lora_worker::backend_factory::CoreMLBackendSettings`

**Functions:**

- All CoreML-related functions (see section 2.7)
- Used in `run_worker()` at:
  - Lines 1686-1691: CoreML primary settings
  - Lines 1718-1726: CoreML boot smoke test
  - Lines 1749-1761: CoreML fallback boot smoke
  - Lines 1805-1891: CoreML verification block

**Tests:**

- Lines 2430-2452: CoreML verification status tests
- Lines 2562-2875: CoreML expected hash resolution tests

### 3.2 Multi-Backend Feature (`#[cfg(feature = "multi-backend")]`)

**Used in:**

- Line 704: `validate_backend_feature()` checks for MLX support
- Lines 753-756: `detect_capabilities()` checks MLX feature flag
- Line 1270: Early MLX initialization in `main()`

### 3.3 MLX Feature (`#[cfg(any(feature = "mlx", feature = "mlx-rs-backend"))]`)

**Used in:**

- Lines 1270-1286: Early MLX runtime initialization before tokio

---

## 4. Initialization Order in main()

### 4.1 `main()` Function (lines 1266-1346)

1. **Early MLX Init** (lines 1270-1286)

   - Initialize MLX runtime BEFORE tokio starts
   - Critical for Metal device initialization order

2. **Tokio Runtime Setup** (lines 1289-1293)

   - Multi-threaded runtime
   - Enable all features

3. **Async Block** (lines 1293-1345)
   - Call `run_worker()` (line 1295)
   - Map errors to exit codes (lines 1296-1307)
   - Log error telemetry (lines 1309-1342)
   - Shutdown telemetry (line 1343)
   - Exit with code (line 1344)

### 4.2 `run_worker()` Function (lines 1348-2366)

**Phase 1: Setup & Validation (lines 1349-1430)**

- Initialize tracing (1350)
- Parse args (1358)
- Load .env (1361)
- Validate cache budget (1367)
- Setup panic hook (1394-1408)
- Resolve UDS path (1411)

**Phase 2: Path Resolution (lines 1441-1454)**

- Resolve model path (1441)
- Resolve tokenizer path (1454)

**Phase 3: Manifest Loading (lines 1457-1560)**

- Load manifest (hash-first or file fallback)
- Verify hash
- Cache manifest

**Phase 4: Backend Configuration (lines 1564-1798)**

- Configure base model pinning (1564-1614)
- Select backend (1625-1670)
- Create primary kernels (1712-1726)
- Create fallback kernels (if coordinator enabled) (1731-1780)
- Wrap kernels (1782-1797)

**Phase 5: CoreML Verification (lines 1800-1891)**

- Compute CoreML hash (if CoreML in use)
- Verify against expected hash
- Update DB registry (if match)

**Phase 6: Telemetry Setup (lines 1893-1913)**

- Create telemetry writer
- Store in static for panic hook
- Configure model cache telemetry

**Phase 7: Registration (lines 1927-2010)**

- Detect capabilities
- Register with CP (with retry)
- Get quota allocation

**Phase 8: Worker Creation (lines 2012-2096)**

- Create quota manager
- Create worker instance
- Setup pause registry
- Log model residency

**Phase 9: Server Setup (lines 2098-2201)**

- Create worker Arc/Mutex
- Setup health monitor
- Load worker verifying key
- Initialize JTI cache
- Bind UDS server

**Phase 10: Runtime Loop (lines 2218-2365)**

- Start health monitor task
- Run server with drain handling
- Handle shutdown signals
- Cleanup on exit

---

## 5. Dependencies Analysis

### 5.1 External Crates

| Crate                          | Usage                                               |
| ------------------------------ | --------------------------------------------------- |
| `adapteros_api_types`          | Worker capabilities, manifest fetch response        |
| `adapteros_boot`               | JTI cache, panic utils, key loading                 |
| `adapteros_config`             | Path resolution, config parsing, env loading        |
| `adapteros_core`               | Error types, hash, execution profile, worker status |
| `adapteros_db`                 | CoreML fusion pair registry (cfg-gated)             |
| `adapteros_lora_kernel_api`    | Backend types, mock kernels, kernel traits          |
| `adapteros_lora_kernel_coreml` | CoreML-specific types (cfg-gated)                   |
| `adapteros_lora_worker`        | Worker, backend factory, UDS server, health         |
| `adapteros_manifest`           | ManifestV3 parsing                                  |
| `adapteros_telemetry`          | Telemetry writer, unified events                    |
| `clap`                         | CLI argument parsing                                |
| `serde_json`, `serde_yaml`     | JSON/YAML parsing                                   |
| `tokio`                        | Async runtime, signals, sync primitives             |
| `tracing`                      | Logging                                             |
| `ureq`                         | HTTP client for CP communication                    |
| `uuid`                         | Worker ID generation                                |

### 5.2 Internal Module Dependencies

**Current Structure:**

- Binary file (`bin/aos_worker.rs`) - standalone, no internal modules

**After Split:**

- `mod.rs` → depends on all submodules
- `cli.rs` → minimal dependencies (clap, config)
- `init.rs` → depends on all other modules
- `registration.rs` → depends on api_types, ureq
- `manifest.rs` → depends on manifest, config, core
- `backend.rs` → depends on worker, kernel_api, api_types
- `coreml.rs` → depends on db, kernel_coreml, kernel_api (cfg-gated)
- `helpers.rs` → depends on telemetry, boot, core

**Dependency Graph:**

```
init.rs
├── cli.rs
├── registration.rs
├── manifest.rs
├── backend.rs
├── coreml.rs (optional, cfg-gated)
└── helpers.rs
```

---

## 6. Risks & Considerations

### 6.1 Initialization Order Dependencies

**CRITICAL:**

1. **MLX Early Init** (lines 1270-1286)

   - Must happen BEFORE tokio runtime starts
   - Currently in `main()`, must remain there
   - **Risk:** Moving to `init.rs` is fine, but must stay before tokio

2. **Panic Hook Setup** (lines 1394-1408)

   - Must happen early, before any operations that could panic
   - Sets static `WORKER_IDENTITY` and `WORKER_TELEMETRY`
   - **Risk:** Must be called before manifest loading, backend creation

3. **Static State** (lines 73-74)
   - `WORKER_IDENTITY` and `WORKER_TELEMETRY` are static `OnceLock`
   - Used by panic hook (which runs in panic context)
   - **Risk:** Must be accessible from panic hook, keep in `helpers.rs` or root

### 6.2 Cfg-Gated Code Spanning Multiple Sections

**Issue:**

- CoreML verification code (lines 1805-1891) is in `run_worker()` but uses functions from `coreml.rs`
- CoreML backend setup (lines 1686-1726, 1749-1761) is mixed with general backend code

**Mitigation:**

- Keep CoreML-specific calls in `init.rs` but move implementation to `coreml.rs`
- Use feature flags to conditionally compile `coreml.rs` module
- Ensure `init.rs` can handle missing `coreml` module gracefully

### 6.3 Complex State Setup

**Issues:**

1. **Backend Selection** (lines 1625-1798)

   - Mock backend path vs real backend path
   - Coordinator fallback logic
   - CoreML-specific setup mixed in
   - **Risk:** Hard to separate cleanly

2. **Worker Creation** (lines 2012-2096)

   - Depends on registration result (quota)
   - Depends on backend selection
   - Depends on manifest
   - **Risk:** Circular dependencies if not careful

3. **Server Setup** (lines 2098-2201)
   - Depends on worker instance
   - Depends on auth keys (strict mode)
   - Depends on JTI cache
   - **Risk:** Tight coupling

### 6.4 Test Module Dependencies

**Tests** (lines 2393-2876):

- Tests are at bottom of file
- Some tests are cfg-gated (CoreML tests)
- Tests import from parent module
- **Risk:** After split, tests need to import from appropriate modules

**Recommendation:**

- Keep tests in `init.rs` or create `tests/` directory
- Use `#[cfg(test)]` module in each submodule for unit tests
- Integration tests in separate `tests/` directory

### 6.5 Error Handling & Exit Codes

**Exit Code Mapping** (lines 1243-1257):

- Maps `AosError` variants to exit codes
- Used in `main()` for process exit
- **Risk:** Must be accessible from `main()` and `init.rs`

**Recommendation:**

- Keep `error_to_exit_code()` in `cli.rs` or `helpers.rs`
- Re-export from `mod.rs` for easy access

### 6.6 Async Function Dependencies

**Async Functions:**

- `resolve_expected_coreml_hash()` (line 522) - async, uses DB
- `run_worker()` (line 1348) - async, main initialization
- `shutdown_worker_telemetry()` (line 1147) - async
- `join_task_with_timeout()` (line 2368) - async

**Risk:** Async functions need tokio runtime, ensure proper async context

---

## 7. Module Split Execution Plan

### Phase 1: Extract CLI (`cli.rs`)

1. Move `Args` struct
2. Move exit code constants
3. Move `error_to_exit_code()`
4. Update imports

### Phase 2: Extract Helpers (`helpers.rs`)

1. Move `WorkerIdentity` struct
2. Move static `WORKER_IDENTITY` and `WORKER_TELEMETRY`
3. Move `setup_panic_hook()` and `shutdown_worker_telemetry()`
4. Update imports

### Phase 3: Extract Manifest (`manifest.rs`)

1. Move `LoadedManifest` struct
2. Move manifest parsing/fetching/caching functions
3. Update imports

### Phase 4: Extract Registration (`registration.rs`)

1. Move `RegistrationResult` and `RegistrationParams` structs
2. Move registration functions
3. Move constants (`SCHEMA_VERSION`, `API_VERSION`)
4. Update imports

### Phase 5: Extract Backend (`backend.rs`)

1. Move backend selection/validation functions
2. Move capability detection functions
3. Move mock backend setup
4. Update imports

### Phase 6: Extract CoreML (`coreml.rs`)

1. Move all CoreML-related types and functions
2. Add `#[cfg(all(target_os = "macos", feature = "coreml-backend"))]` to entire module
3. Update imports (cfg-gated)
4. Ensure `init.rs` handles missing module gracefully

### Phase 7: Extract Init (`init.rs`)

1. Move `main()` function (keep MLX early init)
2. Move `run_worker()` function
3. Move `join_task_with_timeout()` helper
4. Move `is_prod_runtime()` helper
5. Update all function calls to use module paths

### Phase 8: Create `mod.rs`

1. Declare all modules (with cfg-gate for `coreml`)
2. Re-export public items
3. Re-export `main()` from `init.rs`

### Phase 9: Update Tests

1. Move tests to appropriate modules
2. Update imports
3. Ensure cfg-gated tests are properly gated

### Phase 10: Verify Build

1. Test with `cargo check`
2. Test with `cargo build`
3. Test with `cargo test`
4. Test with/without `coreml-backend` feature
5. Test with/without `multi-backend` feature

---

## 8. Verification Checklist

- [x] All structs identified and mapped
- [x] All functions identified and mapped
- [x] All cfg-gated sections identified
- [x] Initialization order documented
- [x] Dependencies analyzed
- [x] Risks identified
- [x] Execution plan created

---

## 9. Recommendations

1. **Keep MLX early init in `main()`** - Critical for Metal initialization order
2. **Keep static state in `helpers.rs`** - Needed for panic hook access
3. **Use feature flags for `coreml.rs`** - Entire module should be cfg-gated
4. **Keep tests close to code** - Use `#[cfg(test)]` modules in each file
5. **Maintain initialization order** - Document dependencies clearly
6. **Test all feature combinations** - Ensure cfg-gated code works correctly

---

**End of Audit Report**
