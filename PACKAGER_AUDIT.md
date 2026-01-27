# Packager.rs Module Split Audit Report

**File:** `crates/adapteros-lora-worker/src/training/packager.rs`  
**Total Lines:** 3,346  
**Date:** Audit completed

## Executive Summary

The `packager.rs` file is a large, monolithic module (3,346 lines) that handles adapter packaging, manifest generation, metadata parsing, CoreML placement, and AOS archive creation. The proposed 6-module split is **viable** but requires careful dependency management to avoid circular dependencies.

## 1. Complete Structure Inventory

### 1.1 Public Structs (with line numbers)

| Struct                   | Lines   | Purpose                       | Visibility |
| ------------------------ | ------- | ----------------------------- | ---------- |
| `AdapterPackager`        | 38-40   | Main packager struct          | `pub`      |
| `PackagedAdapter`        | 44-49   | Result of packaging operation | `pub`      |
| `AdapterManifest`        | 53-145  | Complete manifest structure   | `pub`      |
| `LayerHash`              | 152-156 | Per-layer hash entry          | `pub`      |
| `CoremlTrainingMetadata` | 160-168 | CoreML-specific metadata      | `pub`      |
| `AdapterPlacement`       | 172-175 | Placement metadata            | `pub`      |
| `PlacementRecord`        | 179-185 | Individual placement record   | `pub`      |
| `ScanRootMetadata`       | 194-216 | Scan root metadata            | `pub`      |
| `BranchMetadata`         | 585-610 | Git branch/commit metadata    | `pub`      |

### 1.2 Private Structs

| Struct                  | Lines   | Purpose                              |
| ----------------------- | ------- | ------------------------------------ |
| `ScopeMetadataExtract`  | 479-490 | Extracted scope metadata (internal)  |
| `ManifestFieldsExtract` | 495-509 | Extracted manifest fields (internal) |

### 1.3 Type Aliases

| Type                  | Line | Purpose                         |
| --------------------- | ---- | ------------------------------- |
| `CoremlPlacementSpec` | 148  | Alias for `CoreMLPlacementSpec` |

### 1.4 Constants

| Constant                            | Line | Value  |
| ----------------------------------- | ---- | ------ |
| `DEFAULT_ARTIFACT_HARD_QUOTA_BYTES` | 31   | 10 GiB |
| `DEFAULT_ARTIFACT_SOFT_PCT`         | 32   | 0.8    |

### 1.5 Impl Blocks

| Impl                   | Lines     | Methods Count                                                                 |
| ---------------------- | --------- | ----------------------------------------------------------------------------- |
| `impl BranchMetadata`  | 612-755   | 9 methods                                                                     |
| `impl AdapterManifest` | 1052-1400 | 3 methods (validate, validate_scope_metadata, validate_quantization_metadata) |
| `impl AdapterPackager` | 1402-2667 | ~25 methods                                                                   |

### 1.6 Helper Functions (Private)

#### Default Value Functions

- `default_determinism_mode()` - 218-224
- `default_category()` - 226-228
- `default_tier()` - 230-232
- `default_scope()` - 234-236
- `default_recommended_for_moe()` - 238-240

#### Metadata Parsing Functions

- `normalize_optional_str()` - 242-247
- `normalize_scan_roots()` - 249-258
- `parse_lora_tier()` - 260-267
- `parse_metadata_bool()` - 269-273
- `default_strength_for_tier()` - 275-282
- `parse_scan_roots_from_metadata()` - 290-337
- `resolve_scan_root_from_metadata()` - 339-356
- `parse_bool_strict()` - 358-364
- `metadata_has_scan_root_keys()` - 366-381
- `metadata_indicates_codebase()` - 383-400
- `parse_scan_roots_strict()` - 402-444
- `expected_repo_slug_from_metadata()` - 446-475
- `extract_manifest_fields()` - 513-578
- `apply_branch_metadata_defaults()` - 757-778
- `normalize_commit_metadata()` - 780-802
- `apply_codebase_scope_defaults()` - 804-835
- `extract_scope_metadata()` - 838-892
- `persist_scope_metadata()` - 894-958
- `parse_session_tags()` - 960-987
- `normalize_session_tags()` - 989-1004

#### CoreML Helper Functions

- `canonicalize_backend_label()` - 1008-1021
- `is_valid_graph_target()` - 1023-1029
- `infer_op_kind_from_target()` - 1031-1050

### 1.7 Public Methods (AdapterPackager)

| Method                               | Lines     | Purpose                        |
| ------------------------------------ | --------- | ------------------------------ |
| `new()`                              | 1404-1408 | Constructor                    |
| `with_default_path()`                | 1415-1421 | Default path constructor       |
| `from_config()`                      | 1424-1430 | Config-based constructor       |
| `package()`                          | 1876-1893 | Package adapter (basic)        |
| `package_with_metadata()`            | 1896-2011 | Package with metadata          |
| `package_aos()`                      | 2040-2049 | Package as .aos (legacy)       |
| `package_aos_for_tenant()`           | 2017-2034 | Package as .aos (tenant-aware) |
| `package_aos_with_metadata()`        | 2052-2197 | Package as .aos with metadata  |
| `package_aos_with_branch_metadata()` | 2235-2272 | Package with branch metadata   |
| `verify_signature()`                 | 2582-2624 | Verify adapter signature       |
| `load()`                             | 2627-2666 | Load packaged adapter          |

### 1.8 Private Methods (AdapterPackager)

| Method                                   | Lines     | Purpose                  |
| ---------------------------------------- | --------- | ------------------------ |
| `artifact_quota_limits()`                | 1432-1442 | Get quota limits         |
| `current_artifact_usage()`               | 1444-1465 | Check current usage      |
| `enforce_artifact_quota()`               | 1467-1486 | Enforce quota            |
| `build_manifest_metadata()`              | 1489-1593 | Build manifest metadata  |
| `validate_quantized_shapes()`            | 1595-1633 | Validate weight shapes   |
| `parse_coreml_placement_from_metadata()` | 1635-1645 | Parse CoreML placement   |
| `default_coreml_placement_spec()`        | 1647-1676 | Default placement spec   |
| `validate_coreml_placement_spec()`       | 1678-1734 | Validate placement spec  |
| `resolve_coreml_placement_spec()`        | 1736-1750 | Resolve placement spec   |
| `build_coreml_sections()`                | 1752-1836 | Build CoreML sections    |
| `adapter_dir()`                          | 1838-1844 | Get adapter directory    |
| `artifact_usage_for_tenant()`            | 1846-1849 | Get tenant usage         |
| `dir_size()`                             | 1851-1873 | Calculate directory size |
| `save_weights_safetensors()`             | 2275-2287 | Save weights             |
| `compute_hash()`                         | 2289-2297 | Compute file hash        |
| `canonical_layer_id()`                   | 2300-2351 | Canonical layer ID       |
| `build_safetensors_bytes()`              | 2353-2461 | Build safetensors bytes  |
| `compute_per_layer_hashes_from_bytes()`  | 2463-2494 | Compute per-layer hashes |
| `sign_manifest()`                        | 2497-2521 | Sign manifest            |
| `sign_archive()`                         | 2524-2551 | Sign archive             |
| `deterministic_keypair()`                | 2553-2560 | Generate keypair         |
| `load_signing_keypair()`                 | 2562-2579 | Load signing keypair     |

### 1.9 Test Module

**Location:** Lines 2669-3346 (677 lines)

**Test Functions:**

- `test_compute_hash()` - 2674-2685
- `test_save_load_manifest()` - 2688-2751
- `artifact_quota_enforces_hard_limit()` - 2754-2769
- `test_per_layer_hashes_use_canonical_ids()` - 2772-2797
- `manifest_prefers_actual_backend_metadata()` - 2800-2815
- `manifest_keeps_backend_reason_metadata()` - 2818-2833
- `derives_domain_group_operation_from_defaults()` - 2836-2852
- `respects_provided_hierarchy_overrides()` - 2855-2871
- `invalid_coreml_placement_is_rejected()` - 2874-2916
- `default_coreml_placement_covers_modules()` - 2919-2929
- `artifact_quota_limits_respect_env()` - 2932-2940
- `parse_scan_roots_from_json_array()` - 2943-2960
- `parse_scan_roots_from_scope_scan_root_fallback()` - 2963-2976
- `parse_scan_roots_prefers_relative_paths()` - 2979-2993
- `parse_scan_roots_returns_empty_for_no_data()` - 2996-3000
- `extract_scope_metadata_from_canonical_keys()` - 3003-3032
- `extract_scope_metadata_falls_back_to_repo_keys()` - 3035-3058
- `extract_scope_metadata_falls_back_to_commit_sha()` - 3061-3067
- `branch_metadata_from_metadata_commit_sha_fallback()` - 3070-3077
- `extract_scope_metadata_prefers_canonical_over_fallback()` - 3080-3087
- `scan_root_metadata_serialization_roundtrip()` - 3090-3104
- `branch_metadata_new_creates_basic_instance()` - 3107-3112
- `branch_metadata_builder_pattern()` - 3115-3138
- `branch_metadata_to_metadata_entries()` - 3141-3156
- `branch_metadata_from_metadata_canonical_keys()` - 3159-3181
- `branch_metadata_from_metadata_fallback_keys()` - 3184-3196
- `branch_metadata_prefers_canonical_over_fallback()` - 3199-3207
- `branch_metadata_is_present_checks_branch_or_commit()` - 3210-3225
- `branch_metadata_serialization_roundtrip()` - 3228-3248
- `branch_metadata_entries_exclude_none_values()` - 3251-3262
- `extract_manifest_fields_includes_scope_metadata()` - 3264-3319
- `extract_manifest_fields_with_scan_roots_json()` - 3322-3345

## 2. Proposed Module Split Verification

### 2.1 Module: `mod.rs` (Re-exports & Coordination)

**Purpose:** Module coordination and public API re-exports

**Contents:**

- Module declarations (`mod types;`, `mod manifest;`, etc.)
- Public re-exports matching current `mod.rs` exports:
  - `pub use types::{AdapterManifest, PackagedAdapter, ...};`
  - `pub use manifest::AdapterManifest;` (if validation stays in manifest)
  - `pub use coreml::CoremlTrainingMetadata, AdapterPlacement, ...`
  - `pub use aos::AdapterPackager;`

**Dependencies:**

- All other modules

**Risk Level:** Low - Pure coordination layer

---

### 2.2 Module: `types.rs` (Shared Types)

**Purpose:** All shared type definitions that multiple modules need

**Contents:**

#### Public Types (MUST be extracted first):

- `AdapterManifest` struct (53-145) - **CRITICAL: Used everywhere**
- `PackagedAdapter` struct (44-49)
- `LayerHash` struct (152-156)
- `CoremlTrainingMetadata` struct (160-168)
- `AdapterPlacement` struct (172-175)
- `PlacementRecord` struct (179-185)
- `ScanRootMetadata` struct (194-216)
- `BranchMetadata` struct (585-610)
- `CoremlPlacementSpec` type alias (148)

#### Private Types:

- `ScopeMetadataExtract` struct (479-490) - Used by metadata extraction
- `ManifestFieldsExtract` struct (495-509) - Used by metadata extraction

#### Constants:

- `DEFAULT_ARTIFACT_HARD_QUOTA_BYTES` (31)
- `DEFAULT_ARTIFACT_SOFT_PCT` (32)

#### Default Functions (may stay here or move to metadata):

- `default_determinism_mode()` (218-224)
- `default_category()` (226-228)
- `default_tier()` (230-232)
- `default_scope()` (234-236)
- `default_recommended_for_moe()` (238-240)

**Dependencies:**

- External crates: `serde`, `adapteros_types`, `adapteros_core`
- **NO internal dependencies** - This is the foundation

**Risk Level:** Low - Pure type definitions

**Extraction Order:** **FIRST** - All other modules depend on these types

---

### 2.3 Module: `metadata.rs` (Metadata Parsing/Normalization)

**Purpose:** Extract, parse, and normalize metadata from HashMap

**Contents:**

#### Extraction Functions:

- `extract_manifest_fields()` (513-578) - **CRITICAL**
- `extract_scope_metadata()` (838-892) - **CRITICAL**
- `persist_scope_metadata()` (894-958)

#### Parsing Functions:

- `parse_lora_tier()` (260-267)
- `parse_metadata_bool()` (269-273)
- `default_strength_for_tier()` (275-282)
- `parse_scan_roots_from_metadata()` (290-337)
- `resolve_scan_root_from_metadata()` (339-356)
- `parse_scan_roots_strict()` (402-444)
- `expected_repo_slug_from_metadata()` (446-475)
- `parse_session_tags()` (960-987)
- `normalize_session_tags()` (989-1004)

#### Normalization Functions:

- `normalize_optional_str()` (242-247)
- `normalize_scan_roots()` (249-258)
- `apply_branch_metadata_defaults()` (757-778)
- `normalize_commit_metadata()` (780-802)
- `apply_codebase_scope_defaults()` (804-835)

#### Helper Functions:

- `parse_bool_strict()` (358-364)
- `metadata_has_scan_root_keys()` (366-381)
- `metadata_indicates_codebase()` (383-400)

**Dependencies:**

- `types.rs` - Uses `ScopeMetadataExtract`, `ManifestFieldsExtract`, `ScanRootMetadata`, `BranchMetadata`
- External: `HashMap`, `adapteros_core` (includes normalization utilities via `adapteros_infra_common`)

**Risk Level:** Medium - Many helper functions, but clear boundaries

**Extraction Order:** **SECOND** - After types, before manifest (which uses these)

---

### 2.4 Module: `manifest.rs` (Manifest Validation)

**Purpose:** Manifest validation logic

**Contents:**

#### Validation Methods:

- `AdapterManifest::validate()` (1053-1212) - **CRITICAL**
- `AdapterManifest::validate_scope_metadata()` (1214-1319)
- `AdapterManifest::validate_quantization_metadata()` (1321-1399)

**Dependencies:**

- `types.rs` - Uses `AdapterManifest`
- `metadata.rs` - Uses `extract_scope_metadata()`, `parse_scan_roots_strict()`, `expected_repo_slug_from_metadata()`, `metadata_indicates_codebase()`, `metadata_has_scan_root_keys()`, `resolve_scan_root_from_metadata()`, `parse_bool_strict()`
- External: Constants from parent module (`LORA_Q15_QUANTIZATION`, etc.)

**Risk Level:** Medium - Depends on metadata parsing functions

**Extraction Order:** **THIRD** - After types and metadata

---

### 2.5 Module: `coreml.rs` (CoreML Placement Handling)

**Purpose:** CoreML-specific placement logic

\*\*Contents:

#### CoreML Functions:

- `canonicalize_backend_label()` (1008-1021)
- `is_valid_graph_target()` (1023-1029)
- `infer_op_kind_from_target()` (1031-1050)
- `AdapterPackager::parse_coreml_placement_from_metadata()` (1635-1645)
- `AdapterPackager::default_coreml_placement_spec()` (1647-1676)
- `AdapterPackager::validate_coreml_placement_spec()` (1678-1734)
- `AdapterPackager::resolve_coreml_placement_spec()` (1736-1750)
- `AdapterPackager::build_coreml_sections()` (1752-1836)

**Dependencies:**

- `types.rs` - Uses `CoremlTrainingMetadata`, `AdapterPlacement`, `PlacementRecord`, `CoremlPlacementSpec`
- External: `adapteros_types::coreml::*`

**Risk Level:** Low - Self-contained, clear boundaries

**Extraction Order:** **FOURTH** - After types, independent of metadata/manifest

---

### 2.6 Module: `aos.rs` (AOS Format Packaging)

**Purpose:** AOS archive creation and packaging operations

**Contents:**

#### Public Methods:

- `AdapterPackager::new()` (1404-1408)
- `AdapterPackager::with_default_path()` (1415-1421)
- `AdapterPackager::from_config()` (1424-1430)
- `AdapterPackager::package()` (1876-1893)
- `AdapterPackager::package_with_metadata()` (1896-2011)
- `AdapterPackager::package_aos()` (2040-2049)
- `AdapterPackager::package_aos_for_tenant()` (2017-2034)
- `AdapterPackager::package_aos_with_metadata()` (2052-2197)
- `AdapterPackager::package_aos_with_branch_metadata()` (2235-2272)
- `AdapterPackager::load()` (2627-2666)
- `AdapterPackager::verify_signature()` (2582-2624)

#### Private Methods:

- `AdapterPackager::artifact_quota_limits()` (1432-1442)
- `AdapterPackager::current_artifact_usage()` (1444-1465)
- `AdapterPackager::enforce_artifact_quota()` (1467-1486)
- `AdapterPackager::build_manifest_metadata()` (1489-1593) - **Uses metadata functions**
- `AdapterPackager::validate_quantized_shapes()` (1595-1633)
- `AdapterPackager::adapter_dir()` (1838-1844)
- `AdapterPackager::artifact_usage_for_tenant()` (1846-1849)
- `AdapterPackager::dir_size()` (1851-1873)
- `AdapterPackager::save_weights_safetensors()` (2275-2287)
- `AdapterPackager::compute_hash()` (2289-2297)
- `AdapterPackager::canonical_layer_id()` (2300-2351)
- `AdapterPackager::build_safetensors_bytes()` (2353-2461)
- `AdapterPackager::compute_per_layer_hashes_from_bytes()` (2463-2494)
- `AdapterPackager::sign_manifest()` (2497-2521)
- `AdapterPackager::sign_archive()` (2524-2551)
- `AdapterPackager::deterministic_keypair()` (2553-2560)
- `AdapterPackager::load_signing_keypair()` (2562-2579)

#### Struct:

- `AdapterPackager` struct (38-40) - **MUST move here**

**Dependencies:**

- `types.rs` - Uses `AdapterManifest`, `PackagedAdapter`, `LayerHash`, `BranchMetadata`
- `metadata.rs` - Uses `extract_manifest_fields()`, `persist_scope_metadata()`
- `manifest.rs` - Uses `AdapterManifest::validate()`
- `coreml.rs` - Uses `resolve_coreml_placement_spec()`, `build_coreml_sections()`
- External: Many (safetensors, blake3, adapteros_aos, etc.)

**Risk Level:** **HIGH** - Most complex module, orchestrates everything

**Extraction Order:** **LAST** - Depends on all other modules

---

## 3. Dependency Analysis

### 3.1 External Dependencies (Imports)

```rust
use super::quantizer::{LoRAQuantizer, QuantizedLoRAWeights};
use super::trainer::{MoETrainingConfig, TrainingConfig};
use super::{LORA_Q15_DENOM, LORA_Q15_QUANTIZATION, ...}; // Constants
use adapteros_aos::{AosWriter, BackendTag};
use adapteros_core::{AosError, RepoAdapterPaths, Result};
use adapteros_crypto::Keypair;
use adapteros_lora_router::ROUTER_GATE_Q15_DENOM;
use adapteros_core::normalize_repo_slug;
use adapteros_storage::{ByteStorage, FsByteStorage, StorageKey};
use adapteros_types::coreml::{...};
use adapteros_types::training::LoraTier;
use safetensors::{...};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};
use walkdir::WalkDir;
```

### 3.2 Internal Dependencies (What imports packager)

**From `mod.rs`:**

```rust
pub use packager::{
    AdapterManifest, AdapterPackager, BranchMetadata, PackagedAdapter, ScanRootMetadata,
};
```

**No other files directly import from packager** - All access goes through `mod.rs` re-exports.

### 3.3 Dependency Graph

```
mod.rs (coordination)
  ├── types.rs (foundation - no internal deps)
  ├── metadata.rs
  │     └── depends on: types.rs
  ├── manifest.rs
  │     ├── depends on: types.rs
  │     └── depends on: metadata.rs
  ├── coreml.rs
  │     └── depends on: types.rs
  └── aos.rs
        ├── depends on: types.rs
        ├── depends on: metadata.rs
        ├── depends on: manifest.rs
        └── depends on: coreml.rs
```

**No circular dependencies detected** ✅

---

## 4. Shared Types & Helpers That Must Be Extracted First

### 4.1 Critical Shared Types (Extract FIRST)

1. **`AdapterManifest`** (53-145)

   - Used by: manifest.rs (validation), aos.rs (creation), types.rs (definition)
   - **MUST be in types.rs**

2. **`PackagedAdapter`** (44-49)

   - Used by: aos.rs (return type)
   - **MUST be in types.rs**

3. **`ScopeMetadataExtract`** (479-490) - Private

   - Used by: metadata.rs (extraction), aos.rs (via ManifestFieldsExtract)
   - **MUST be in types.rs** (or metadata.rs if kept private)

4. **`ManifestFieldsExtract`** (495-509) - Private
   - Used by: metadata.rs (extraction), aos.rs (via extract_manifest_fields)
   - **MUST be in types.rs** (or metadata.rs if kept private)

### 4.2 Helper Functions Used Across Modules

1. **`extract_manifest_fields()`** (513-578)

   - Used by: aos.rs (package methods)
   - **MUST be in metadata.rs**

2. **`extract_scope_metadata()`** (838-892)

   - Used by: metadata.rs (via extract_manifest_fields), manifest.rs (validation)
   - **MUST be in metadata.rs**

3. **`persist_scope_metadata()`** (894-958)

   - Used by: aos.rs (package methods)
   - **MUST be in metadata.rs**

4. **`build_manifest_metadata()`** (1489-1593)
   - Used by: aos.rs (package methods)
   - **MUST be in aos.rs** (or metadata.rs if we want to separate concerns)

---

## 5. Risks & Challenges

### 5.1 Circular Dependencies

**Status:** ✅ **NO CIRCULAR DEPENDENCIES DETECTED**

The dependency graph is acyclic:

- `types.rs` → no internal deps
- `metadata.rs` → `types.rs` only
- `manifest.rs` → `types.rs` + `metadata.rs`
- `coreml.rs` → `types.rs` only
- `aos.rs` → all others (leaf node)

### 5.2 Complex Coupling

**Risk Areas:**

1. **`build_manifest_metadata()`** (1489-1593)

   - Calls: `apply_branch_metadata_defaults()`, `normalize_commit_metadata()`, `apply_codebase_scope_defaults()`
   - These are in metadata.rs
   - **Solution:** Keep in aos.rs, import from metadata.rs

2. **`extract_manifest_fields()`** calls `extract_scope_metadata()`

   - Both should be in metadata.rs
   - **No issue** - same module

3. **Manifest validation** calls metadata parsing functions
   - `validate_scope_metadata()` calls `extract_scope_metadata()`, `parse_scan_roots_strict()`, etc.
   - **Solution:** Import from metadata.rs

### 5.3 Test Dependencies

**Current State:**

- All tests in single `#[cfg(test)]` module (2669-3346)
- Tests import `super::*` (all types/functions)

**After Split:**

- Tests can stay in `packager/tests.rs` or split per module
- Each test module imports from its parent module + shared types
- **Recommendation:** Keep tests in parent `packager/` directory, import from submodules

### 5.4 Public API Surface

**Current Exports (from `mod.rs`):**

```rust
pub use packager::{
    AdapterManifest, AdapterPackager, BranchMetadata, PackagedAdapter, ScanRootMetadata,
};
```

**After Split:**

- `mod.rs` must re-export all public types
- **No change to external API** - consumers still use `crate::training::AdapterPackager`

### 5.5 Constants Dependencies

**Constants used across modules:**

- `LORA_Q15_QUANTIZATION`, `LORA_Q15_VERSION`, `LORA_Q15_DENOM` - from parent `mod.rs`
- `LORA_STRENGTH_DEFAULTS_VERSION`, `LORA_STRENGTH_DEFAULT_*` - from parent `mod.rs`
- `ROUTER_GATE_Q15_DENOM` - from `adapteros_lora_router`

**Solution:** These remain accessible via `super::` or explicit paths.

---

## 6. Step-by-Step Extraction Plan

### Phase 1: Extract Foundation Types (LOW RISK)

1. Create `packager/types.rs`
2. Move all struct definitions (AdapterManifest, PackagedAdapter, etc.)
3. Move type aliases
4. Move constants
5. Move default value functions
6. Update `packager.rs` to `pub use types::*;`
7. Run tests - should pass

### Phase 2: Extract Metadata Parsing (MEDIUM RISK)

1. Create `packager/metadata.rs`
2. Move all metadata parsing/normalization functions
3. Move `ScopeMetadataExtract` and `ManifestFieldsExtract` (or keep in types.rs)
4. Update `packager.rs` to import from metadata
5. Update `types.rs` if needed for shared structs
6. Run tests - should pass

### Phase 3: Extract Manifest Validation (MEDIUM RISK)

1. Create `packager/manifest.rs`
2. Move `impl AdapterManifest` validation methods
3. Import types and metadata functions
4. Update `packager.rs` to import from manifest
5. Run tests - should pass

### Phase 4: Extract CoreML Logic (LOW RISK)

1. Create `packager/coreml.rs`
2. Move CoreML-related functions
3. Move CoreML impl methods from AdapterPackager
4. Import types
5. Update `packager.rs` to import from coreml
6. Run tests - should pass

### Phase 5: Extract AOS Packaging (HIGH RISK)

1. Create `packager/aos.rs`
2. Move `AdapterPackager` struct
3. Move all remaining `impl AdapterPackager` methods
4. Import from types, metadata, manifest, coreml
5. Update `packager.rs` to `pub use aos::AdapterPackager;`
6. Run tests - should pass

### Phase 6: Create mod.rs Coordination (LOW RISK)

1. Create `packager/mod.rs`
2. Declare all submodules
3. Re-export public API matching current exports
4. Update parent `mod.rs` if needed
5. Run full test suite

### Phase 7: Cleanup & Verification

1. Remove old `packager.rs` file
2. Verify all imports work
3. Run determinism tests
4. Run full test suite
5. Check clippy/lint

---

## 7. Verification Checklist

### Pre-Split Verification

- [x] All structs identified with line numbers
- [x] All impl blocks identified
- [x] All functions identified
- [x] All tests identified
- [x] Dependencies mapped
- [x] No circular dependencies
- [x] Public API surface documented

### Post-Split Verification

- [ ] All tests pass
- [ ] No circular dependencies
- [ ] Public API unchanged (backward compatible)
- [ ] Clippy passes
- [ ] Determinism tests pass
- [ ] Integration tests pass
- [ ] Documentation updated

---

## 8. Recommendations

### 8.1 Extraction Strategy

**Recommended Order:**

1. ✅ **types.rs** - Foundation (no dependencies)
2. ✅ **metadata.rs** - Depends only on types
3. ✅ **manifest.rs** - Depends on types + metadata
4. ✅ **coreml.rs** - Depends only on types (can be parallel with manifest)
5. ✅ **aos.rs** - Depends on all others (last)
6. ✅ **mod.rs** - Coordination layer

### 8.2 Test Organization

**Option A:** Keep all tests in `packager/tests.rs`

- Pros: Single test file, easier to maintain
- Cons: Large test file

**Option B:** Split tests per module (`packager/tests/types.rs`, etc.)

- Pros: Better organization
- Cons: More files, some tests may need multiple modules

**Recommendation:** **Option A** initially, refactor later if needed

### 8.3 Constants Handling

**Current:** Constants in parent `mod.rs` (`LORA_Q15_QUANTIZATION`, etc.)

**Options:**

1. Keep in parent `mod.rs` (accessible via `super::`)
2. Move to `types.rs` (if used by types)
3. Create `packager/constants.rs`

**Recommendation:** Keep in parent `mod.rs` for now, move later if needed

### 8.4 Private Helper Structs

**`ScopeMetadataExtract`** and **`ManifestFieldsExtract`**:

- Currently private
- Used by metadata extraction functions
- **Recommendation:** Keep in `types.rs` but mark as `pub(crate)` or keep in `metadata.rs` as private

---

## 9. File Size Estimates (Post-Split)

| Module        | Estimated Lines | Complexity                 |
| ------------- | --------------- | -------------------------- |
| `types.rs`    | ~600            | Low                        |
| `metadata.rs` | ~700            | Medium                     |
| `manifest.rs` | ~350            | Medium                     |
| `coreml.rs`   | ~350            | Low                        |
| `aos.rs`      | ~1,200          | High                       |
| `mod.rs`      | ~50             | Low                        |
| `tests.rs`    | ~677            | Medium                     |
| **Total**     | ~3,927          | (includes module overhead) |

**Note:** Actual lines may be slightly higher due to module declarations and imports.

---

## 10. Conclusion

The proposed 6-module split is **viable and well-structured**. The dependency graph is acyclic, and the boundaries are clear. The main risks are:

1. **Complex coupling** in `build_manifest_metadata()` - manageable with proper imports
2. **Test organization** - recommend keeping tests together initially
3. **Constants** - keep in parent module for now

**Recommended Action:** Proceed with extraction in the order specified, testing after each phase.
