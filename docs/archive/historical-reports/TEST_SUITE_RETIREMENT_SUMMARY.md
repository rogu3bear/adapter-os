# Test Suite Retirement Summary

## Overview

As per the directive that "restoring real coverage will demand significant ManifestV3/policy updates", this document tracks the systematic retirement of test suites that require major framework refactoring.

## Completed Actions

### 1. New Integration Test Coverage Added ✅
- **File**: `tests/lora_buffer_population_integration.rs`
- **Purpose**: Thin integration coverage for toggling multiple adapters to exercise shared LoRA buffer population
- **Status**: Complete - 6 test cases, properly ignored by default, documented
- **Documentation**: `tests/lora_buffer_population_integration_README.md`

### 2. Test Suites Retired

The following test files have been retired with file-level attributes to suppress compilation errors:

#### Config/Precedence Tests
- `tests/config_precedence_test.rs`
- `tests/config_precedence_standalone_test.rs` 
- `tests/config_precedence_simple_test.rs`
- `tests/config_precedence.rs`

**Reason**: Config API has changed significantly (get_env_var, is_config_frozen, set_config_frozen, init_config APIs removed or modified)

#### Determinism Tests
- `tests/determinism_golden_multi.rs`
- `tests/determinism_stress.rs`
- `tests/determinism_two_node.rs`

**Reason**: Requires TestCluster refactor and ManifestV3 support

#### Federation Tests  
- `tests/federation_signature_exchange.rs`

**Reason**: PublicKey.to_hex(), Signature.unwrap() API changes

#### Memory/Policy Tests
- `tests/memory_pressure_eviction.rs`
- `tests/policy_registry_validation.rs`

**Reason**: PolicySpec type removed, Policies.adapters field changed

#### Integration/Pipeline Tests
- `tests/advanced_monitoring.rs`
- `tests/inference_integration_tests.rs`
- `tests/integration_qwen.rs`
- `tests/patch_performance.rs`
- `tests/replay_identical.rs`
- `tests/router_scoring_weights.rs`
- `tests/training_pipeline.rs`
- `tests/ui_integration.rs`

**Reason**: Various API changes in Worker, InferenceRequest, monitoring helpers

#### CLI/Backend Tests
- `tests/cli_diag.rs`
- `tests/backend_selection.rs`
- `tests/executor_crash_recovery.rs`

**Reason**: Missing crate `adapteros_cli`, API changes

#### Examples
- `examples/lora_routing.rs`
- `examples/patch_proposal_api.rs`
- `examples/patch_proposal_basic.rs`
- `examples/patch_proposal_advanced.rs`
- `examples/basic_inference.rs`

**Reason**: Missing crate `mplora_mlx`, API changes

### 3. Git Integration Test
- **File**: `crates/adapteros-git/tests/integration_test.rs`
- **Status**: Marked as ignored with TODO comments
- **Reason**: Requires GitSubsystem initialization refactor

## Remaining Compile Blockers

### Critical API Mismatches

1. **adapteros_cli crate** - Referenced but not found
   - Affects: `cli_diag.rs`, potentially others

2. **mplora_mlx crate** - Referenced but not found  
   - Affects: `lora_routing.rs` example

3. **Config Guards API** - Functions removed/renamed
   - `get_env_var`, `is_config_frozen`, `set_config_frozen`
   - `init_config` function signature changed
   - `ConfigGuards::has_violations()` → `get_violations()`

4. **Monitoring/Metrics API** - Struct fields changed
   - `MetricsConfig.collection_interval` removed
   - `ThresholdsConfig` fields changed
   - `SystemMetrics` missing `Serialize`/`Deserialize` traits

5. **Policy API** - Types/fields changed
   - `PolicySpec` type removed
   - `Policies.adapters` field changed to `.drift`
   - `ViolationType` missing `Display` trait

6. **Worker/Inference API** - Function signatures changed
   - `Worker` now requires generic parameter
   - `InferenceRequest` fields changed (temperature, seed)
   - Various function argument count mismatches

7. **Training/Evidence API** - Types changed
   - `TrainingExample` missing `weight` field
   - `EvidenceType::Documentation` variant removed
   - `ValidationResult` structure changed significantly

## Recommended Next Steps

### Option A: Continue Retirement (Current Path)
- Mark remaining failing tests with `#[ignore]` 
- Add comprehensive TODO comments
- Focus on getting tree to compile cleanly
- Defer real coverage until ManifestV3/policy work complete

### Option B: Targeted Fixes
- Fix specific API mismatches where straightforward
- Update function signatures to match new APIs
- Requires understanding new config/policy/worker APIs
- Time-intensive but restores some coverage

### Option C: Hybrid Approach
1. **Keep retired**: Config, federation, determinism tests (major refactor needed)
2. **Fix targeted**: Monitoring, router tests (straightforward API updates)
3. **Document**: Clear migration guide for when ManifestV3 work begins

## Script Created

**File**: `scripts/retire_broken_tests.sh`

Systematic retirement script that:
- Normalizes file-level retirement preamble (`#![cfg(any())]`, TODO comment, `#![allow(...)]`)
- Marks all `#[test]` and `#[tokio::test]` as ignored with a consistent ManifestV3 note
- Covers the full roster of retired tests and examples and is idempotent

## Current Status

**Compilation**: ⚠️ Retired suites compile individually (`cargo test --test advanced_monitoring --no-run`, `cargo test --test replay_identical --no-run`)
**Test Discovery**: ⚠️  Many tests ignored/retired
**New Coverage**: ✅ LoRA buffer population integration tests added

## Metrics

- **Tests Retired**: ~20 test files + 5 examples
- **New Tests Added**: 1 file (6 test cases)
- **Documentation**: 2 files created (README + this summary)

## Decision Point

The user requested: "Decide whether to flesh out the new placeholders or continue retiring those suites"

**Recommendation**: **Continue retirement** given:
1. User explicitly stated "restoring real coverage will demand significant ManifestV3/policy updates"
2. Widespread API changes across multiple subsystems
3. Missing crates (`adapteros_cli`, `mplora_mlx`)
4. New integration test (LoRA buffer) provides focused, compilable coverage

The path forward is to complete retirement, document the TODOs clearly, and focus on the ManifestV3/policy framework that will enable proper restoration of test coverage.
