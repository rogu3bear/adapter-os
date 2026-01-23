# Exploration Notes - January 17, 2026

## Executive Summary

A 24-30 hour agent session produced **massive** work across multiple areas. The work is distributed across:
- **15+ commits** (Jan 13-15)
- **71 uncommitted files** on main (~7000 lines changed)
- **2 worktrees** with additional uncommitted work
- **4 stashes** with valuable changes

The build is currently broken due to an **incomplete API migration** - `training_dataset_integration.rs` was refactored but the caller in `training.rs` wasn't updated.

---

## Committed Work (Jan 13-15)

### Major Features

#### 1. Patent Compliance Receipt Infrastructure (e62dc4530)
**+803 lines** - Implements Claims 6, 9-10 of patent 3535886.0002
- Receipt schema V5 with equipment profile (processor_id, mlx_version, ane_version)
- Citation ID computation using BLAKE3 hashes
- Merkle root binding for deterministic verification
- Tenant-bound receipts with HMAC-SHA256
- HKDF-SHA256 key derivation for multi-tenant isolation

Files:
- `crates/adapteros-api-types/src/inference.rs` (+308 lines)
- `crates/adapteros-core/src/receipt_digest.rs` (+204 lines)
- `crates/adapteros-crypto/src/receipt_signing.rs` (+239 lines)
- `migrations/0285_equipment_profile.sql`, `migrations/0288_tenant_binding.sql`

#### 2. Layer-Wise Routing for LoRA Adapters (cb3524264)
**+615 lines** - Per-layer adapter routing decisions
- Fine-grained control over which adapters apply at each transformer layer
- New `layer_routing` module
- Migration `0286_layer_routing.sql`

Files:
- `crates/adapteros-lora-router/src/layer_routing.rs` (+537 lines)

#### 3. Training Data Synthesis Infrastructure (a2673e7f7)
**+372,786 lines** - Full synthesis model with bootstrap data
- Synthesis engine for training data generation
- Anchor module for data anchoring operations
- Rectify module for data rectification
- CoreML export scripts
- Pre-trained synthesis model with 500 bootstrap examples

Files:
- `crates/adapteros-orchestrator/src/synthesis/` (new module)
- `crates/adapteros-orchestrator/src/anchor/` (new module)
- `crates/adapteros-orchestrator/src/rectify/` (new module)
- `training/synthesis_model/` (full model + data)

#### 4. ITAR Compliance Middleware (84f3d7ee3)
**+262 lines** - Federal audit trail support
- Tenant ITAR flag enforcement
- Event classification (access, inference, training, export)
- Compliance metadata logging

Files:
- `crates/adapteros-server-api/src/middleware/itar.rs`

#### 5. Receipt Digest Storage & Backfill (9321b9bb7)
**+1157/-1802 lines** - Phase 3 receipt infrastructure
- Migration 0291 for `crypto_receipt_digest_b3` column
- Canonical hash functions in DB layer
- `aosctl receipt backfill` CLI command
- ReceiptGenerator integration with equipment profile binding

#### 6. MLX Determinism (74dc838e5)
- Real MLX as default backend
- DeterminismViolation emission for RNG failures

### Other Commits

| Commit | Description |
|--------|-------------|
| `f6594d2d1` | Rebrand AdapterOS → adapterOS |
| `19341ffa8` | Unified run ID, boot script improvements |
| `a8967f9f0` | SAFETY comments, metrics exporter |
| `80054c433` | manifest_hash_b3 for workers, model hash lookup |
| `96e38aab8` | UI system page with state/model status views |
| `32ecad458` | Boot instrumentation, config redaction |
| `826dd8510` | Shared types from adapteros-api-types |
| `5b9ade9d1` | Extract admin, api_keys, models modules |
| `15711e45c` | Determinism replay harness on all PRs |
| `dd6eb5768` | Fail fast on PKCS#11 HSM (no silent mock) |
| `71b165cb3` | Lock dataset contract (PLAN_4) |

---

## Uncommitted Work on Main

### CRITICAL: PLAN_4 Enforcement Throughout Stack

The uncommitted work is a **comprehensive enforcement of PLAN_4** across multiple layers - I initially underestimated the scope.

### Training Dataset Integration Refactor (~636 lines changed)

**This is PLAN_4 implementation** - the dataset contract.

Key changes in `crates/adapteros-orchestrator/src/training_dataset_integration.rs`:

1. **New `LoadedDatasetExamples` struct**:
```rust
pub struct LoadedDatasetExamples {
    pub examples: Vec<WorkerTrainingExample>,
    pub dataset_hash_b3: String,
    pub dataset_id: String,
    pub framing_policy: TrainingFramingPolicy,
}
```

2. **Training Framing Policy enum**:
```rust
pub enum TrainingFramingPolicy {
    Supervised,       // { "prompt": "...", "completion": "..." }
    RawContinuationV1 // { "text": "..." } with sliding window
}
```

3. **Locked constants** (per PLAN_4):
```rust
const MAX_INPUT_TOKENS: usize = 256;
const MAX_TARGET_TOKENS: usize = 128;
const STRIDE_TOKENS: usize = 256;
```

4. **Document ingestion simplified**: Now extracts deterministic text chunks → saves JSONL `{ "text": "..." }` rows

### Training Example Contract Update

In `crates/adapteros-types/src/training/example.rs`:

- `source_id` → `dataset_id` (with alias for back-compat)
- Added `source_hash` field (hash of raw row payload)
- New validation: `MissingDatasetId`, `MissingSourceHash` errors

Schema updated in `docs/contracts/training-example.schema.json`.

### Trainer Improvements

In `crates/adapteros-lora-worker/src/training/trainer.rs`:

1. **Cross-entropy loss support** (was only MSE):
   - `use_cross_entropy_loss` field
   - Env vars: `AOS_TRAIN_FORCE_CE`, `AOS_TRAIN_LEGACY_LOSS`
   - Large vocab fallback to MSE

2. **CPU proxy training path**:
   - When `use_gpu_backward=false`: skips base model, uses scaled-token MSE
   - Useful for fast iteration without GPU

3. **Conditional base model loading**:
   - Only required when: `use_gpu_backward=true`, `validation_split>0`, or `multi_module_training=true`

### DB Layer: Schema Enforcement (MISSED IN INITIAL REVIEW)

In `crates/adapteros-db/src/training_datasets/db.rs` - **Major refactor**:

The JSONL parser was completely rewritten:
```rust
// OLD: Flexible field mappings
let prompt = object.get("prompt")
    .or_else(|| object.get("input"))
    .or_else(|| object.get("question"))
    .or_else(|| object.get("text"))...

// NEW: Strict PLAN_4 schemas only
let is_supervised = object.len() == 2
    && object.contains_key("prompt")
    && object.contains_key("completion");
let is_raw = object.len() == 1
    && object.contains_key("text");
```

**Removed support for**:
- Flexible field names (input, question, response, output, answer)
- Custom weights
- Custom splits
- Custom sample_role
- Arbitrary metadata_json

**Now only accepts**:
- `{"prompt": "...", "completion": "..."}` (supervised)
- `{"text": "..."}` (raw)

### Dataset Builder: PLAN_4 Constraints (MISSED IN INITIAL REVIEW)

In `crates/adapteros-lora-worker/src/training/builder.rs`:

1. **Locked constants duplicated** (same as orchestrator):
```rust
const MAX_INPUT_TOKENS: usize = 256;
const MAX_TARGET_TOKENS: usize = 128;
const STRIDE_TOKENS: usize = 256;
const SCHEMA_RAW_CONTINUATION: &str = "raw_continuation_v1";
```

2. **New validation**:
```rust
fn ensure_plan4_constraints(&self) -> Result<()> {
    if format != DatasetFormat::Jsonl {
        return Err("Only JSONL datasets are supported by PLAN_4");
    }
    if self.column_mapping.is_some() {
        return Err("Column mapping is not supported by PLAN_4");
    }
}
```

3. **New manifest fields**: `dataset_hash_b3`

### Other Uncommitted Changes

| Area | Files | Notes |
|------|-------|-------|
| CLI | `commands/train*.rs`, `datasets.rs` | Training command updates |
| Orchestrator | `code_ingestion.rs`, `codebase_ingestion.rs` | Ingestion updates |
| Worker | `training/` directory | Multiple trainer modules |
| Server API | `handlers/training.rs`, `datasets/*` | API updates |
| Metal | `manifests/`, `shaders/` | Kernel updates |
| MLX FFI | `mlx_cpp_wrapper_real.cpp` | FFI changes |
| Tests | `preprocessing.rs` tests | Updated for `source_hash` field |

---

## Worktree: cleanup (chore/cleanup-post-v0.13-unstable)

**311 files changed, ~57k lines**

### Primary Work: Rebrand Continuation

- References: `AGENTS.md` → `README.md` throughout
- Case changes: `AdapterOS` → `adapterOS`
- Lint rules updated to reference README.md

### Config Security

In `configs/cp*.toml`:
- JWT secret now via env var `AOS_SECURITY_JWT_SECRET`
- Empty placeholder in config files (was hardcoded dev string)

### Notable Changes

| File | Change |
|------|--------|
| `crates/adapteros-lint/src/architectural.rs` | AGENTS.md → README.md refs |
| `crates/adapteros-lora-mlx-ffi/src/lib.rs` | +11 lines |
| `crates/adapteros-ui/dist/components.css` | +184 lines new styles |
| `tests/security_regression_suite.rs` | +148 lines |
| `tests/executor_crash_recovery.rs` | Refactored (~612 lines) |

---

## Worktree: v0.13-unstable (release/v0.13-unstable)

**20 files changed, ~905 lines**

### Training Execution Improvements

In `crates/adapteros-orchestrator/src/training/execution.rs` (+75 lines):

1. **Base model path resolution from DB**:
```rust
async fn resolve_base_model_path(
    base_model_path: Option<PathBuf>,
    db: Option<&Db>,
    tenant_id: Option<&str>,
    base_model_id: Option<&str>,
) -> Option<PathBuf>
```

2. **Automatic dimension alignment**:
   - Reads `config.json` from base model
   - Aligns `hidden_dim` and `vocab_size` with model config
   - Logs alignment changes

### Training Handler Updates

In `crates/adapteros-server-api/src/handlers/training.rs`:
- Return type: `Result<impl IntoResponse>` with `StatusCode::CREATED`
- Effective trust state resolution from DB
- Better error handling

---

## Stashes

### stash@{0} - Swarm Session WIP
- AOS hash edge cases tests improvements
- Import command with SBOM hash and signature handling
- Evidence envelope enhancements

### stash@{1} - Harmony Restoration (MUCH LARGER THAN INITIALLY NOTED)
**99 files, +2082/-908 lines** - This is substantial work I understated.

**Security improvements**:
- `.gitignore`: Added `.codex/`, `.harmony/`
- Config security (jwt_secret → env var)
- `crates/adapteros-crypto/src/providers/kms.rs`: Added `Zeroize`/`ZeroizeOnDrop` for KmsCredentials
- `crates/adapteros-crypto/src/providers/keychain.rs`: +107 lines refactored

**CLI enhancements**:
- `crates/adapteros-cli/src/commands/registry.rs`: +114 lines - public key signature verification
- `crates/adapteros-cli/src/commands/chat.rs`: +73 lines
- `crates/adapteros-cli/src/commands/policy.rs`: +42 lines
- `crates/adapteros-cli/src/commands/register_adapter.rs`: +34 lines
- `crates/adapteros-cli/src/commands/import.rs`: +49 lines

**Code parsers refactored**:
- `crates/adapteros-codegraph/src/parsers/rust.rs`: +123 lines
- `crates/adapteros-codegraph/src/parsers/python.rs`: 89 lines changed
- `crates/adapteros-codegraph/src/parsers/javascript.rs`: +16 lines
- `crates/adapteros-codegraph/src/parsers/typescript.rs`: +16 lines

**Tests expanded**:
- `tests/security_regression_suite.rs`: +148 lines
- `tests/executor_crash_recovery.rs`: 612 lines refactored
- `tests/config_loading_tests.rs`: +130 lines
- Multiple other test files updated

**UI**:
- `crates/adapteros-ui/dist/components.css`: +184 lines new styles

**Docs**:
- `docs/CLI_GUIDE.md`: +91 lines
- `docs/CONFIGURATION.md`: +11 lines
- Stubbed CLI commands documented in CLAUDE.md

### stash@{2} - Pre-Maintenance
- Makefile dev-no-auth improvements
- Boot config var path rebasing (+77 lines in config.rs)

### stash@{3} - Pre Training Datasets
- Agent CLI command added to main.rs
- Config types: `disable_sec_015_signature_bypass`
- Signature bypass: debug-only (release always returns false)
- Refusal policy pack enhancements (+182 lines)

---

## Build Break Analysis

### Root Cause

**Incomplete API migration**: `training_dataset_integration.rs` was refactored to return `LoadedDatasetExamples` struct, but `training.rs` still uses tuple destructuring.

### Location

`crates/adapteros-server-api/src/handlers/training.rs:1473`:
```rust
// Current (broken):
let (examples, _hash_b3, resolved_id) = dataset_manager
    .load_dataset_version_examples(version_id)
    .await...

// Should be:
let loaded = dataset_manager
    .load_dataset_version_examples(version_id)
    .await...?;
let (examples, resolved_id) = (loaded.examples, loaded.dataset_id);
```

### Secondary Issue

`tests/benchmark/src/main.rs` has `#![cfg(all(test, feature = "extended-tests"))]` but the feature doesn't exist in `tests/benchmark/Cargo.toml`.

---

## Alignment Recommendations

### Priority 1: Fix Build
1. Update `training.rs` to use `LoadedDatasetExamples` struct
2. Add `extended-tests` feature to benchmark Cargo.toml (or remove cfg)

### Priority 2: Merge Cleanup Worktree
The rebrand work is comprehensive and should be merged:
- Config security improvements are valuable
- Reference updates are necessary

### Priority 3: Reconcile v0.13-unstable
The training execution improvements should be cherry-picked:
- Base model path resolution from DB
- Automatic dimension alignment

### Priority 4: Review Stashes
- stash@{1} has valuable security and documentation
- stash@{2} has boot config improvements
- stash@{3} has agent CLI and policy enhancements

---

## Summary Statistics

| Location | Files | Lines Changed |
|----------|-------|---------------|
| Commits (Jan 13-15) | ~50 | ~375,000+ |
| Uncommitted (main) | 71 | ~7,000 |
| Cleanup worktree | 311 | ~57,000 |
| v0.13-unstable worktree | 20 | ~900 |
| Stashes (total) | ~120 | ~3,500 |

**Total estimated work**: ~443,000 lines of changes across the session.

---

## Additional Committed Features

### Training Wizard UI (32d5b2d12)
**+930 lines** - Guided adapter training UX
- `crates/adapteros-ui/src/pages/training/wizard.rs` (+866 lines)
- Enhanced training page components

### Sealed Adapter Containers (ceeb1067b)
**+1398 lines** - Crypto-verified adapter format
- `crates/adapteros-aos/src/sealed.rs` (+602 lines) - Sealed container format
- `crates/adapteros-lora-worker/src/prefix_kv_cache.rs` (+532 lines) - KV cache management
- `xtask/src/convert_mlx_adapter.rs` - Adapter conversion tool

### AARA Lifecycle Inference Audit Trail (53cfb4015)
- API implementation for inference audit trails

---

## Uncommitted: Loss Function Improvements

In `crates/adapteros-lora-worker/src/training/loss.rs`:

New loss types:
```rust
pub enum LossKind {
    CrossEntropy,
    LegacyMse,  // NEW
}

pub enum LossLogitsSource {
    HiddenPlusLoraProjection,
    HiddenPlusLora,  // NEW - for legacy MSE
}
```

New functions:
- `legacy_training_loss_spec()` - For CPU proxy training
- `legacy_validation_loss_spec()`

---

## Key Decisions Made (per PLAN files)

1. **Bootstrap**: `scripts/dev-up.sh` (locked)
2. **Model default**: Smallest available (0.5B) for first success
3. **Dataset contract**: PLAN_4 supervised + raw_continuation_v1 (locked)
4. **Framing constants**: MAX_INPUT=256, MAX_TARGET=128, STRIDE=256 (locked)

---

## Architecture Notes

### Training Pipeline Flow (PLAN_4)

```
Dataset Upload → Validation → Framing → Training → Adapter
     │               │            │          │         │
     │               │            │          │         └── .aos sealed container
     │               │            │          └── MicroLoRATrainer (CE or MSE loss)
     │               │            └── Supervised or RawContinuationV1
     │               └── Schema check, hash verification
     └── JSONL { "prompt", "completion" } or { "text" }
```

### Receipt Compliance Flow (Patent 3535886.0002)

```
Inference Request → Equipment Profile → Citation ID → Merkle Root → Receipt
       │                   │                │             │           │
       │                   │                │             │           └── Tenant-bound via HMAC
       │                   │                │             └── BLAKE3 tree
       │                   │                └── BLAKE3(request+profile)
       │                   └── processor_id, mlx_version, ane_version
       └── /v1/infer
```

---

## Corners I Cut (Honest Assessment)

### What I Initially Missed

1. **Stash@{1} scope**: I said "config security, CLI docs" but it's actually 99 files with security hardening, CLI enhancements, parser refactors, and test expansions.

2. **DB layer PLAN_4 enforcement**: I focused on orchestrator changes but missed that the JSONL parser in `training_datasets/db.rs` was completely rewritten to reject non-PLAN_4 schemas.

3. **Builder PLAN_4 constraints**: The worker's `builder.rs` also has PLAN_4 constants and validation, meaning enforcement is at multiple layers.

4. **The synthesis model size**: I noted 372k lines but didn't examine what that actually contains (pre-trained weights, bootstrap data).

### What I Still Haven't Fully Examined

1. **All 71 uncommitted files on main** - I looked at key ones but not all
2. **The CLAUDE.md memory files** - These contain session context that might explain agent decisions
3. **Metal shader changes** - `manifests/metallib_manifest.json`, `kernel_hash.txt`
4. **MLX FFI changes** - `mlx_cpp_wrapper_real.cpp` (+55 lines in cleanup worktree, +31 in v0.13)
5. **The untracked files** - 25 new files I haven't examined
6. **Full validation changes** - `training_datasets/validation.rs`

### Untracked Files I Didn't Examine (Now Looked At)

**PLAN_4 workflow scripts** (all new):
- `scripts/make_minimal_dataset.py` - Generates deterministic arithmetic training data
- `scripts/start_minimal_training.sh` - Starts training with a dataset_version_id
- `scripts/upload_minimal_dataset.sh` - Uploads dataset to server
- `scripts/plan4_offline_reference.sh` - Offline PLAN_4 reference run
- `scripts/golden_path_adapter_chat.sh` - End-to-end golden path

**New benchmark files**:
- `tests/benchmark/src/evidence_benchmarks.rs`
- `tests/benchmark/src/isolation_benchmarks.rs`
- `tests/benchmark/src/kernel_benchmarks.rs`
- `tests/benchmark/src/memory_benchmarks.rs`
- `tests/benchmark/src/system_benchmarks.rs`
- `tests/benchmark/src/throughput_benchmarks.rs`

**Other**:
- `crates/adapteros-lora-kernel-mtl/src/layout_validator.rs` - New layout validator
- `tests/hydration_gating_test.rs` - Hydration gate test
- `docs/EXECUTION_CONTRACT.md` - Execution contract doc

### Why The Build Breaks

The v0.13-unstable worktree has the OLD API (tuple returns) that's consistent internally.
Main has the NEW API (struct returns) but the caller wasn't updated.

This suggests the PLAN_4 refactor was done on main but the migration was incomplete - possibly because the agent session ended mid-refactor.

---

## Final Summary

The agent session was **extraordinarily productive**. What initially looked like scattered work is actually a **coordinated implementation of PLAN_4** across the entire stack:

1. **Contract defined** (PLAN_4.md) - Schema, framing rules, tokenization rules
2. **DB enforced** (training_datasets/db.rs) - Strict JSONL parsing
3. **Builder enforced** (builder.rs) - PLAN_4 constraints
4. **Orchestrator enforced** (training_dataset_integration.rs) - LoadedDatasetExamples struct
5. **Trainer updated** (trainer.rs) - Cross-entropy loss, CPU proxy mode
6. **Types updated** (example.rs) - source_hash field
7. **Workflow scripts** - End-to-end PLAN_4 reference run

The build break is a **single incomplete migration** in `training.rs` - one file wasn't updated to use the new struct API.
