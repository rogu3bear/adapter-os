# Comprehensive Patch Plan - Full Test Suite Restoration

**Date:** October 20, 2025  
**Status:** 📋 **PLAN** - Ready for execution  
**Priority:** Follow phases sequentially (P1 → P2 → P3 → P4)

## Executive Summary

This document provides a complete, citation-backed plan to restore all 21 retired test files and 5 examples to full functionality. The plan follows codebase best practices: minimal changes, strict typing, proper documentation, and phased resurrection aligned with API stabilization.

**References:**
- Main codebase in `/Users/star/Dev/adapter-os/`
- Manifest structure: `crates/adapteros-manifest/src/lib.rs:1-687`【1†crates/adapteros-manifest/src/lib.rs†L59-L69】
- Worker API: `crates/adapteros-lora-worker/src/lib.rs:1-273`【2†crates/adapteros-lora-worker/src/lib.rs†L243-L270】
- Current retirement status: `TEST_SUITE_RETIREMENT_SUMMARY.md:1-164`

## Current State Analysis

### Retired Files Count
```bash
$ find tests examples -name "*.rs" -exec grep -l "cfg(any())" {} \; | wc -l
21 test files + examples retired
```

### TODOs Identified
```bash
$ grep -r "TODO:\|FIXME:\|XXX:\|HACK:" tests/ | wc -l
26 TODO markers across 20 files
```

### Core API Dependencies

**✅ Available:**
- `ManifestV3` structure complete【1†crates/adapteros-manifest/src/lib.rs†L61-L69】
- `Worker<K: FusedKernels>` generic API【2†crates/adapteros-lora-worker/src/lib.rs†L244】
- `InferenceRequest` with new fields【2†crates/adapteros-lora-worker/src/lib.rs†L131-L141】
- Policy framework (11 policy structs implemented)【1†crates/adapteros-manifest/src/lib.rs†L369-L532】
- `Adapter` with complete metadata【1†crates/adapteros-manifest/src/lib.rs†L98-L137】

**⚠️ Needs Completion:**
- `PolicySpec.severity` field not implemented【3†tests/policy_registry_validation.rs†L128】
- `adapteros_cli` crate missing (referenced but not built)
- `mplora_mlx` crate missing (Python MLX integration)
- Config API (`get_env_var`, `is_config_frozen`) deprecated
- Some Worker methods private or async-only

---

## Phase 1: Core Policy Framework (Priority 1)

**Goal:** Restore policy registry and validation tests  
**Estimated Effort:** 4-6 hours  
**Dependencies:** None (self-contained)

### 1.1 Add Severity Field to PolicySpec

**File:** `crates/adapteros-policy/src/registry.rs:186-193`

**Current State:**
```rust:186-193
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySpec {
    pub id: PolicyId,
    pub name: &'static str,
    pub description: &'static str,
    pub enforcement_point: &'static str,
    pub implemented: bool,
}
```

**Required Change:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySpec {
    pub id: PolicyId,
    pub name: &'static str,
    pub description: &'static str,
    pub enforcement_point: &'static str,
    pub implemented: bool,
    pub severity: Severity,  // ← ADD THIS FIELD
}

// Add Severity enum if not present
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
}
```

**Update POLICY_INDEX initialization** in `crates/adapteros-policy/src/registry.rs` around line 200+:
```rust
impl PolicySpec {
    pub fn from_id(id: PolicyId) -> Self {
        Self {
            id,
            name: id.name(),
            description: id.description(),
            enforcement_point: id.enforcement_point(),
            implemented: id.is_implemented(),
            severity: id.severity(),  // ← ADD THIS
        }
    }
}

// Add to PolicyId impl
impl PolicyId {
    pub fn severity(&self) -> Severity {
        match self {
            PolicyId::Egress => Severity::Critical,
            PolicyId::Determinism => Severity::Critical,
            PolicyId::Evidence => Severity::High,
            PolicyId::Refusal => Severity::High,
            PolicyId::Router => Severity::Medium,
            PolicyId::Numeric => Severity::Medium,
            // ... map all 22 policies
        }
    }
}
```

### 1.2 Restore Policy Registry Tests

**File:** `tests/policy_registry_validation.rs`

**Current State:** Gated with `#![cfg(any())]` at line 1

**Required Changes:**

1. **Remove retirement gate** (line 1):
   ```diff
   - #![cfg(any())]
   ```

2. **Remove ignore attributes** from all tests:
   ```diff
   - #[ignore = "requires ManifestV3/policy updates"]
   #[test]
   fn test_policy_registry_count() {
   ```

3. **Re-enable severity test** (lines 128-146):
   ```diff
   - #[ignore = "severity field not yet implemented in PolicySpec"]
   #[test]
   fn test_policy_severities_valid() {
   -    // TODO: Re-enable when severity field is added to PolicySpec
   -    // use adapteros_policy::registry::Severity;
   +    use adapteros_policy::registry::Severity;
       
   -    // for policy in POLICY_INDEX.iter() {
   +    for policy in POLICY_INDEX.iter() {
   ```

4. **Fix serialization test** (lines 180-217):
   ```diff
   - #[ignore]
   #[test]
   fn test_policy_registry_serialization() {
   ```

5. **Fix production readiness test** (lines 267-298):
   ```diff
   - #[ignore]
   #[test]
   fn test_policy_registry_production_ready() {
       // Remove NOTE comment about severity not implemented
   -    // NOTE: Severity field not yet implemented in PolicySpec
   ```

**Verification:**
```bash
cargo test --test policy_registry_validation
# Expected: All 13 tests pass
```

---

## Phase 2: Worker & Inference Pipeline (Priority 2)

**Goal:** Restore determinism, integration, and inference tests  
**Estimated Effort:** 12-16 hours  
**Dependencies:** Phase 1 complete

### 2.1 Fix InferenceRequest Struct Usage

**Files Affected:**
- `tests/determinism_stress.rs:30-40`
- `tests/integration_qwen.rs` (multiple locations)
- `tests/inference_integration_tests.rs`
- `tests/replay_identical.rs`

**Current InferenceRequest API:**【2†crates/adapteros-lora-worker/src/lib.rs†L131-L141】
```rust:131-141
pub struct InferenceRequest {
    pub cpid: String,
    pub prompt: String,
    pub max_tokens: usize,
    #[serde(default)]
    pub require_evidence: bool,
    #[serde(default)]
    pub request_type: RequestType,
}
```

**Old API (tests still use):**
```rust
// INCORRECT - outdated
InferenceRequest {
    prompt: "test".to_string(),
    max_tokens: Some(50),        // ❌ Option type removed
    temperature: Some(0.0),      // ❌ Field doesn't exist
    seed: Some(42),              // ❌ Field doesn't exist
    tenant_id: "test".to_string(), // ❌ Now called 'cpid'
    request_id: "req-001".to_string(), // ❌ Doesn't exist
}
```

**Required Changes:**
```rust
// CORRECT - current API
InferenceRequest {
    cpid: "test-tenant".to_string(),
    prompt: "test".to_string(),
    max_tokens: 50,  // Direct usize, not Option
    require_evidence: true,
    request_type: RequestType::Normal,
}
```

### 2.2 Restore determinism_stress.rs

**File:** `tests/determinism_stress.rs`

**Current State:** Complete stub with `#![cfg(any())]` gate

**Required Changes:**

1. **Remove retirement gate** (line 1):
   ```diff
   - #![cfg(any())]
   ```

2. **Implement proper Worker setup** (lines 16-27):
   ```rust
   use adapteros_lora_kernel_mtl::MetalKernels;
   use adapteros_lora_worker::{InferenceRequest, Worker};
   use std::sync::Arc;

   async fn setup_worker() -> Worker<MetalKernels> {
       let manifest_path = "manifests/qwen7b.yaml";
       let manifest_content = std::fs::read_to_string(manifest_path)
           .expect("Failed to read manifest");
       
       // Note: Manifest is JSON now, not YAML
       let manifest: ManifestV3 = serde_json::from_str(&manifest_content)
           .expect("Failed to parse manifest JSON");
       
       Worker::new(Arc::new(manifest))
           .await
           .expect("Failed to create worker")
   }
   ```

3. **Create proper test request** (lines 29-40):
   ```rust
   fn create_test_request() -> InferenceRequest {
       InferenceRequest {
           cpid: "determinism-test".to_string(),
           prompt: "What is the capital of France?".to_string(),
           max_tokens: 50,
           require_evidence: false,
           request_type: RequestType::Normal,
       }
   }
   ```

4. **Implement actual test logic** (lines 42-57):
   ```rust
   #[tokio::test]
   #[ignore = "Requires Metal GPU hardware"]
   async fn test_10k_inference_determinism() {
       let mut worker = setup_worker().await;
       let request = create_test_request();
       
       let mut hashes = Vec::new();
       
       for i in 0..10_000 {
           if i % 1000 == 0 {
               println!("Progress: {}/10,000", i);
               // Simulate restart
               if i > 0 {
                   drop(worker);
                   worker = setup_worker().await;
               }
           }
           
           let response = worker.infer(&request).await
               .expect("Inference failed");
           
           let hash = B3Hash::hash(response.text.as_ref().unwrap().as_bytes());
           hashes.push(hash);
       }
       
       // Verify all hashes identical
       let first = &hashes[0];
       for (i, hash) in hashes.iter().enumerate().skip(1) {
           assert_eq!(hash, first, 
               "Hash mismatch at iteration {}: {} != {}", 
               i, hash.to_hex(), first.to_hex());
       }
       
       println!("✅ All 10,000 outputs identical!");
   }
   ```

### 2.3 Restore integration_qwen.rs

**File:** `tests/integration_qwen.rs`

**Current State:** Retired with outdated struct usage

**Key Issues:**【4†tests/integration_qwen.rs†L68-L75】
1. `SpecialTokens` field names wrong
2. `ChatTemplate` missing `template_hash`
3. `ModelConfig` API changes
4. Manifest initialization outdated

**Required Changes:**

1. **Remove retirement gate** (lines 1-4):
   ```diff
   - #![cfg(any())]
   - //! TODO: Requires ManifestV3/policy framework updates - retired pending refactor
   - #![cfg(feature = "integration_tests_v3")]
   - #![allow(dead_code, unused_imports, ...)]
   ```

2. **Fix ChatTemplate usage** (lines 66-75):
   ```rust
   // CORRECT structure
   let template = ChatTemplate {
       name: "qwen".to_string(),
       template: "qwen_template".to_string(),
       special_tokens: SpecialTokens {
           bos: "<|im_start|>".to_string(),  // ✅ Correct field name
           eos: "<|im_end|>".to_string(),    // ✅ Correct field name
           unk: "<|unk|>".to_string(),       // ✅ Correct field name
           pad: "<|pad|>".to_string(),       // ✅ Correct field name
       },
   };
   ```

3. **Fix ModelConfig API** (line 126):
   ```diff
   - let dims = config.calculate_dimensions();
   + let dims = config.dimensions();  // ✅ Method renamed
   ```

4. **Fix ManifestV3 initialization** (lines 223-254):
   
   Use complete ManifestV3 structure from【1†crates/adapteros-manifest/src/lib.rs†L61-L69】:
   
   ```rust
   let manifest = ManifestV3 {
       schema: "adapteros.manifest.v3".to_string(),
       base: Base {
           model_id: "Qwen2.5-7B-Instruct".to_string(),
           model_hash: B3Hash::hash(b"test"),
           arch: "qwen2".to_string(),
           vocab_size: 32000,
           hidden_dim: 4096,
           n_layers: 32,
           n_heads: 32,
           config_hash: B3Hash::hash(b"config"),
           tokenizer_hash: B3Hash::hash(b"tokenizer"),
           tokenizer_cfg_hash: B3Hash::hash(b"tokenizer_cfg"),
           license_hash: None,
           rope_scaling_override: None,
       },
       adapters: vec![],
       router: RouterCfg {
           k_sparse: 3,
           gate_quant: "q15".to_string(),
           entropy_floor: 0.02,
           tau: 1.0,
           sample_tokens_full: 128,
           warmup: false,
           algorithm: "weighted".to_string(),
           orthogonal_penalty: 0.1,
           shared_downsample: false,
           compression_ratio: 0.8,
           multi_path_enabled: false,
           diversity_threshold: 0.05,
           orthogonal_constraints: false,
       },
       telemetry: TelemetryCfg {
           schema_hash: B3Hash::hash(b"telemetry"),
           sampling: Sampling {
               token: 0.05,
               router: 1.0,
               inference: 1.0,
           },
           router_full_tokens: 128,
           bundle: BundleCfg {
               max_events: 500000,
               max_bytes: 268435456,
           },
       },
       policies: Policies::default(),  // Use default policies
       seeds: Seeds {
           global: B3Hash::hash(b"global"),
           manifest_hash: B3Hash::hash(b"manifest"),
           parent_cpid: None,
       },
   };
   ```

**Verification:**
```bash
cargo test --test integration_qwen -- --include-ignored
# Expected: Tests compile and pass (on systems with models)
```

### 2.4 Restore inference_integration_tests.rs

**File:** `tests/inference_integration_tests.rs`

**Similar fixes as integration_qwen.rs:**
- Update `InferenceRequest` usage
- Fix Worker initialization
- Update manifest creation

---

## Phase 3: Config & Federation (Priority 3)

**Goal:** Restore config precedence and federation tests  
**Estimated Effort:** 8-10 hours  
**Dependencies:** Phase 2 complete

### 3.1 Config API Migration

**Deprecated APIs:**
```rust
// ❌ REMOVED - do not use
get_env_var()
is_config_frozen()
set_config_frozen()
init_config(old_signature)
```

**New Config API Pattern:**

Research current config API in `crates/adapteros-config/` and update tests to use new patterns.

**Files to Fix:**
- `tests/config_precedence_test.rs`
- `tests/config_precedence_standalone_test.rs`
- `tests/config_precedence_simple_test.rs`
- `tests/config_precedence.rs`

**Strategy:**
1. Identify current config API from `crates/adapteros-config/src/lib.rs`
2. Map old API calls to new equivalents
3. Update all test files systematically
4. Remove `#![cfg(any())]` gates
5. Remove ignore attributes

### 3.2 Federation Tests

**File:** `tests/federation_signature_exchange.rs`

**Issues:**【5†tests/adapter_provenance.rs†L18-L19】
- `PublicKey.to_hex()` doesn't exist
- `Signature.unwrap()` wrong API

**Fix:**
```rust
// OLD (incorrect)
let key_hex = public_key.to_hex();
let signature = sign_bytes(&keypair, data).unwrap();

// NEW (correct)
let key_hex = hex::encode(public_key.to_bytes());
let signature = sign_bytes(&keypair, data);  // Returns Signature directly
```

**Required Changes:**
1. Remove retirement gate
2. Update all crypto API calls to use `.to_bytes()` + `hex::encode()`
3. Remove `.unwrap()` from `sign_bytes()` calls
4. Update federation protocol initialization if needed

---

## Phase 4: Advanced Features (Priority 4)

**Goal:** Restore patch, replay, monitoring, and UI tests  
**Estimated Effort:** 16-20 hours  
**Dependencies:** Phases 1-3 complete

### 4.1 Patch Proposal Tests

**Files:**
- `tests/patch_performance.rs`
- `examples/patch_proposal_api.rs`
- `examples/patch_proposal_basic.rs`
- `examples/patch_proposal_advanced.rs`

**Current Patch API:**【6†crates/adapteros-lora-worker/src/patch_generator.rs†L14-L33】
```rust
pub struct PatchGenerationRequest {
    pub repo_id: String,
    pub commit_sha: Option<String>,
    pub target_files: Vec<String>,
    pub description: String,
    pub evidence: Vec<EvidenceSpan>,
    pub context: HashMap<String, String>,
}

pub struct PatchProposal {
    pub proposal_id: String,
    pub rationale: String,
    pub patches: Vec<FilePatch>,
    pub citations: Vec<EvidenceCitation>,
    pub confidence: f32,
    pub metadata: HashMap<String, String>,
}
```

**Worker InferenceRequest for Patches:**【2†crates/adapteros-lora-worker/src/lib.rs†L139-L150】
```rust
InferenceRequest {
    cpid: "patch-test".to_string(),
    prompt: "Fix the bug in main.rs".to_string(),
    max_tokens: 1000,
    require_evidence: true,
    request_type: RequestType::PatchProposal(PatchProposalRequest {
        repo_id: "test/repo".to_string(),
        commit_sha: Some("abc123".to_string()),
        target_files: vec!["src/main.rs".to_string()],
        description: "Fix null pointer dereference".to_string(),
    }),
}
```

**Steps:**
1. Remove retirement gates from all patch-related files
2. Update to use new `RequestType::PatchProposal` enum
3. Update struct field references
4. Implement mock LLM backend if needed【6†crates/adapteros-lora-worker/src/llm_backend.rs†L184-L232】

### 4.2 Replay & Determinism Tests

**Files:**
- `tests/replay_identical.rs`
- `tests/determinism_two_node.rs`
- `tests/determinism_golden_multi.rs`

**Requirements:**
1. Update Worker initialization
2. Fix InferenceRequest usage
3. Implement replay file format matching current telemetry schema
4. Update TestCluster API if multi-node tests needed

### 4.3 Monitoring & Metrics Tests

**Files:**
- `tests/advanced_monitoring.rs`
- `tests/memory_pressure_eviction.rs`

**Issues:**
- `MetricsConfig.collection_interval` removed
- `ThresholdsConfig` fields changed
- `SystemMetrics` missing traits

**Strategy:**
1. Research current metrics API in `crates/adapteros-system-metrics/`
2. Map old field names to new equivalents
3. Add missing trait implementations if needed
4. Update test assertions

### 4.4 UI Integration Tests

**File:** `tests/ui_integration.rs`

**Requirements:**
1. Research server API in `crates/adapteros-server-api/`
2. Update endpoint paths if changed
3. Fix request/response struct definitions
4. Update client initialization

### 4.5 CLI Tests

**File:** `tests/cli_diag.rs`

**Issue:** `adapteros_cli` crate missing

**Options:**
1. **Create the crate**: Implement `crates/adapteros-cli/` with basic CLI structure
2. **Stub the test**: Keep test retired until CLI implementation
3. **Remove test**: If CLI not planned for v1.0

**Recommendation:** Stub for now, implement CLI crate as separate task.

---

## Phase 5: Examples Restoration (Priority 5)

**Goal:** Restore all examples to working state  
**Estimated Effort:** 6-8 hours  
**Dependencies:** Phases 1-4 complete

### 5.1 Basic Inference Example

**File:** `examples/basic_inference.rs`

**Current State:** Empty placeholder【7†examples/basic_inference.rs†L1-L50】

**Required Implementation:**
```rust
use adapteros_lora_kernel_mtl::MetalKernels;
use adapteros_lora_worker::{Worker, InferenceRequest, RequestType};
use adapteros_manifest::ManifestV3;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 AdapterOS Basic Inference Example\n");
    
    // Load manifest
    let manifest_json = std::fs::read_to_string("manifests/qwen7b.yaml")?;
    let manifest: ManifestV3 = serde_json::from_str(&manifest_json)?;
    
    // Initialize worker
    println!("📦 Initializing worker with Metal kernels...");
    let worker = Worker::<MetalKernels>::new(Arc::new(manifest)).await?;
    
    // Create request
    let request = InferenceRequest {
        cpid: "example-basic".to_string(),
        prompt: "Explain what AdapterOS does in one sentence.".to_string(),
        max_tokens: 100,
        require_evidence: false,
        request_type: RequestType::Normal,
    };
    
    // Run inference
    println!("🔄 Running inference...");
    let response = worker.infer(&request).await?;
    
    // Display result
    println!("\n📝 Result:");
    println!("{}", response.text.unwrap_or_default());
    
    println!("\n✅ Example complete!");
    Ok(())
}
```

### 5.2 LoRA Routing Example

**File:** `examples/lora_routing.rs`

**Issue:** `mplora_mlx` crate missing

**Options:**
1. **Implement Metal-only version**: Use `adapteros_lora_kernel_mtl` directly
2. **Wait for MLX integration**: Keep retired until Python bindings ready
3. **Create pure-Rust example**: Show routing without MLX dependency

**Recommendation:** Option 3 - Create pure-Rust example showing router logic

### 5.3 Patch Proposal Examples

**Files:**
- `examples/patch_proposal_api.rs`
- `examples/patch_proposal_basic.rs`
- `examples/patch_proposal_advanced.rs`

**Implementation:** Similar to basic_inference but with `RequestType::PatchProposal`

---

## Verification & Testing Strategy

### Per-Phase Verification

**Phase 1:**
```bash
cargo test --test policy_registry_validation
cargo test --test policy_gates
# Expected: 100% pass rate
```

**Phase 2:**
```bash
cargo test --test determinism_stress -- --include-ignored
cargo test --test integration_qwen -- --include-ignored
cargo test --test inference_integration_tests
# Expected: Pass with Metal hardware, proper ignore on others
```

**Phase 3:**
```bash
cargo test --test config_precedence_test
cargo test --test federation_signature_exchange
# Expected: 100% pass rate
```

**Phase 4:**
```bash
cargo test --test patch_performance
cargo test --test replay_identical
cargo test --test advanced_monitoring
cargo test --test ui_integration
# Expected: 100% pass rate
```

**Phase 5:**
```bash
cargo run --example basic_inference
cargo run --example lora_routing
cargo run --example patch_proposal_basic
# Expected: Execute without errors
```

### Full Suite Verification

**After all phases:**
```bash
# Compile everything
cargo test --tests --examples --no-run

# Run all non-hardware-dependent tests
cargo test --tests

# Run Metal-specific tests (on macOS with GPU)
cargo test --tests -- --include-ignored --test-threads=1

# Run examples
for example in basic_inference lora_routing patch_proposal_basic; do
    cargo run --example $example || echo "Example $example needs data files"
done

# Verify no retired tests remain
find tests examples -name "*.rs" -exec grep -l "cfg(any())" {} \; | wc -l
# Expected: 0
```

---

## Risk Management

### High-Risk Changes

1. **ManifestV3 Structure Changes**
   - **Risk:** Manifest format changes break existing files
   - **Mitigation:** Provide migration script, update all manifests/
   - **Testing:** Validate against golden manifests

2. **Worker API Breaking Changes**
   - **Risk:** Generic parameter requirements break downstream
   - **Mitigation:** Provide type aliases for common cases
   - **Testing:** Compile all workspace crates after changes

3. **Policy Field Additions**
   - **Risk:** Serialization breaks existing policy files
   - **Mitigation:** Use `#[serde(default)]` for new fields
   - **Testing:** Load old and new policy files

### Testing Strategy

**Unit Tests:** Fix and verify per-file as restored  
**Integration Tests:** Run full suite after each phase  
**Manual Testing:** Examples require data files (document requirements)  
**CI/CD:** Update .github/workflows to run resurrected tests  

---

## Documentation Requirements

### Per File Updates

**When restoring each test file:**
1. Remove `#![cfg(any())]` gate
2. Remove retirement comment
3. Update module-level documentation
4. Add examples showing new API usage
5. Update `#[ignore]` reasons if still needed

**Example:**
```rust
//! Integration tests for Qwen2.5-7B model
//!
//! These tests verify:
//! - Model configuration parsing【ModelConfig API】
//! - Chat template processing【ChatTemplate struct】
//! - GQA configuration validation
//! - LoRA memory calculation【calculate_lora_size function】
//! - RoPE configuration
//!
//! Run with:
//! ```bash
//! cargo test --test integration_qwen -- --include-ignored
//! ```
//!
//! Requirements:
//! - Model files in models/qwen2.5-7b-mlx/
//! - Metal GPU (macOS) for full tests
```

### Update Main Documentation

**Files to Update:**
- `README.md` - Remove "tests retired" warnings
- `TESTING_CHECKLIST.md` - Add all restored test suites
- `CURRENT_STATUS.md` - Update test coverage metrics
- `TEST_SUITE_RETIREMENT_SUMMARY.md` - Mark as superseded

---

## Timeline Estimates

### Optimistic (Experienced Developer, No Blockers)
- Phase 1: 4 hours
- Phase 2: 12 hours
- Phase 3: 8 hours
- Phase 4: 16 hours
- Phase 5: 6 hours
- **Total: 46 hours (~6 days)**

### Realistic (Standard Development Pace)
- Phase 1: 6 hours
- Phase 2: 16 hours
- Phase 3: 10 hours
- Phase 4: 20 hours
- Phase 5: 8 hours
- Debugging/Integration: 10 hours
- **Total: 70 hours (~9 days)**

### Conservative (Including Documentation & Testing)
- Phase 1: 8 hours
- Phase 2: 20 hours
- Phase 3: 12 hours
- Phase 4: 24 hours
- Phase 5: 10 hours
- Debugging/Integration: 16 hours
- Documentation: 10 hours
- **Total: 100 hours (~13 days)**

---

## Success Criteria

### Phase-Level Success

**Phase 1 Complete:**
- ✅ PolicySpec has severity field
- ✅ All policy registry tests pass
- ✅ POLICY_INDEX serializes/deserializes correctly
- ✅ No compilation errors in policy tests

**Phase 2 Complete:**
- ✅ determinism_stress.rs fully functional
- ✅ integration_qwen.rs passes all tests
- ✅ InferenceRequest used correctly everywhere
- ✅ Worker<K> initialization works in all tests

**Phase 3 Complete:**
- ✅ All config tests pass with new API
- ✅ Federation tests use correct crypto API
- ✅ No deprecated API usage

**Phase 4 Complete:**
- ✅ Patch proposal tests functional
- ✅ Monitoring tests updated to new metrics API
- ✅ UI tests pass with current server API
- ✅ All advanced features tested

**Phase 5 Complete:**
- ✅ All examples run without errors
- ✅ Documentation updated
- ✅ README shows correct usage

### Project-Level Success

**Final Acceptance Criteria:**
```bash
# 1. No retired tests remain
$ find tests examples -name "*.rs" -exec grep -l "cfg(any())" {} \; | wc -l
0

# 2. All tests compile
$ cargo test --tests --examples --no-run
   Finished `test` profile [optimized + debuginfo] target(s) in X.XXs

# 3. Core tests pass
$ cargo test --tests
test result: ok. 150+ passed; 0 failed; X ignored

# 4. Examples documented
$ grep -r "cargo run --example" examples/*/README.md | wc -l
5+

# 5. No TODO markers for retired tests
$ grep -r "requires ManifestV3/policy updates" tests/ | wc -l
0
```

---

## Quick Start Guide

### To Begin Restoration

1. **Create feature branch:**
   ```bash
   git checkout -b feature/restore-test-suite
   ```

2. **Start with Phase 1:**
   ```bash
   # Add severity field
   vim crates/adapteros-policy/src/registry.rs
   
   # Restore policy tests
   vim tests/policy_registry_validation.rs
   # Remove: #![cfg(any())]
   # Remove: #[ignore = "requires ManifestV3/policy updates"]
   
   # Test
   cargo test --test policy_registry_validation
   ```

3. **Document progress:**
   ```bash
   # Update this plan as you complete phases
   vim COMPREHENSIVE_PATCH_PLAN.md
   # Mark completed items with ✅
   ```

4. **Commit per phase:**
   ```bash
   git add -p
   git commit -m "Phase 1: Restore policy registry tests

   - Added PolicySpec.severity field
   - Implemented PolicyId.severity() method
   - Removed retirement gates from policy tests
   - All 13 policy tests now passing
   
   Refs: COMPREHENSIVE_PATCH_PLAN.md Phase 1"
   ```

---

## References & Citations

### File References (by number)
【1】`crates/adapteros-manifest/src/lib.rs:1-687` - ManifestV3 structure  
【2】`crates/adapteros-lora-worker/src/lib.rs:1-273` - Worker<K> API  
【3】`tests/policy_registry_validation.rs:128` - PolicySpec.severity missing  
【4】`tests/integration_qwen.rs:68-75` - ChatTemplate struct usage  
【5】`tests/adapter_provenance.rs:18-19` - Crypto API fixes  
【6】`crates/adapteros-lora-worker/src/patch_generator.rs:14-33` - Patch API  
【7】`examples/basic_inference.rs:1-50` - Empty placeholder example  

### Best Practices Applied
- **Minimal Changes:** Only modify what's necessary per test
- **Strict Typing:** No `any`, maintain type safety
- **Citations:** Every change references specific line numbers
- **Phased Approach:** Prioritized by dependency graph
- **Documentation:** Update docs alongside code
- **Testing:** Verify after each phase

### Related Documents
- `TEST_SUITE_FIX_SUMMARY.md` - What was fixed to get compilation
- `TEST_SUITE_RETIREMENT_SUMMARY.md` - Why tests were retired
- `MLP_QKV_TEST_REPORT.md` - GPU parity test documentation
- `scripts/retire_broken_tests.sh` - Retirement automation

---

## Conclusion

This plan provides a complete, citation-backed roadmap to restore all 26 retired test files and examples. By following the phased approach and using the specific code references provided, the test suite can be fully restored in an estimated 9-13 days of focused work.

**Next Step:** Begin Phase 1 by adding the `severity` field to `PolicySpec`.

**Status Updates:** Track progress by marking completed items with ✅ in this document.

---

**Document Version:** 1.0  
**Last Updated:** October 20, 2025  
**Maintained By:** AdapterOS Development Team

