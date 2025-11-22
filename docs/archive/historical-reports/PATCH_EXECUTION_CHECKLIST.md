# Patch Execution Checklist

Quick reference for executing `COMPREHENSIVE_PATCH_PLAN.md`

## Phase 1: Policy Framework ⏳

- [ ] 1.1 Add `severity` field to `PolicySpec` struct
  - File: `crates/adapteros-policy/src/registry.rs:186-193`
  - Add `pub severity: Severity,` field
  - Add `Severity` enum (Critical, High, Medium, Low)
  
- [ ] 1.2 Add `PolicyId::severity()` method
  - File: `crates/adapteros-policy/src/registry.rs` (impl block)
  - Map all 22 policy IDs to severity levels
  
- [ ] 1.3 Update `PolicySpec::from_id()`
  - Add `severity: id.severity()` to initializer
  
- [ ] 1.4 Restore `tests/policy_registry_validation.rs`
  - Remove `#![cfg(any())]` (line 1)
  - Remove all `#[ignore = "requires ManifestV3/policy updates"]`
  - Re-enable severity test (lines 128-146)
  - Fix serialization test (lines 180-217)
  
- [ ] 1.5 Verify Phase 1
  ```bash
  cargo test --test policy_registry_validation
  # Expected: 13/13 tests pass
  ```

## Phase 2: Worker & Inference ⏳

- [ ] 2.1 Update all `InferenceRequest` usage
  - Files: `determinism_stress.rs`, `integration_qwen.rs`, `inference_integration_tests.rs`
  - New structure:
    ```rust
    InferenceRequest {
        cpid: String,
        prompt: String,
        max_tokens: usize,  // Not Option!
        require_evidence: bool,
        request_type: RequestType,
    }
    ```

- [ ] 2.2 Restore `tests/determinism_stress.rs`
  - Remove `#![cfg(any())]`
  - Implement `setup_worker()` with `Worker<MetalKernels>`
  - Update test logic with proper async/await
  
- [ ] 2.3 Restore `tests/integration_qwen.rs`
  - Remove retirement gates
  - Fix `SpecialTokens` fields (bos, eos, unk, pad)
  - Fix `ModelConfig::dimensions()` (not calculate_dimensions)
  - Update `ManifestV3` initialization
  
- [ ] 2.4 Restore `tests/inference_integration_tests.rs`
  - Similar fixes as integration_qwen.rs
  
- [ ] 2.5 Verify Phase 2
  ```bash
  cargo test --test determinism_stress -- --include-ignored
  cargo test --test integration_qwen -- --include-ignored
  cargo test --test inference_integration_tests
  ```

## Phase 3: Config & Federation ⏳

- [ ] 3.1 Research new Config API
  - File: `crates/adapteros-config/src/lib.rs`
  - Document replacements for deprecated functions
  
- [ ] 3.2 Update all config tests
  - `tests/config_precedence_test.rs`
  - `tests/config_precedence_standalone_test.rs`
  - `tests/config_precedence_simple_test.rs`
  - `tests/config_precedence.rs`
  - Remove retirement gates
  - Update API calls
  
- [ ] 3.3 Fix crypto API in federation tests
  - File: `tests/federation_signature_exchange.rs`
  - Replace `public_key.to_hex()` with `hex::encode(public_key.to_bytes())`
  - Replace `signature.unwrap()` with direct `Signature` use
  
- [ ] 3.4 Verify Phase 3
  ```bash
  cargo test --test config_precedence_test
  cargo test --test federation_signature_exchange
  ```

## Phase 4: Advanced Features ⏳

- [ ] 4.1 Restore patch proposal tests
  - `tests/patch_performance.rs`
  - Use `RequestType::PatchProposal(...)` enum variant
  - Update struct fields
  
- [ ] 4.2 Restore replay/determinism tests
  - `tests/replay_identical.rs`
  - `tests/determinism_two_node.rs`
  - `tests/determinism_golden_multi.rs`
  - Update Worker and InferenceRequest usage
  
- [ ] 4.3 Restore monitoring tests
  - `tests/advanced_monitoring.rs`
  - `tests/memory_pressure_eviction.rs`
  - Research new metrics API
  - Update field references
  
- [ ] 4.4 Restore UI integration
  - `tests/ui_integration.rs`
  - Update server API calls
  
- [ ] 4.5 Handle CLI tests
  - `tests/cli_diag.rs`
  - Decision: Create `adapteros_cli` crate or keep retired?
  
- [ ] 4.6 Verify Phase 4
  ```bash
  cargo test --test patch_performance
  cargo test --test replay_identical
  cargo test --test advanced_monitoring
  cargo test --test ui_integration
  ```

## Phase 5: Examples ⏳

- [ ] 5.1 Implement `examples/basic_inference.rs`
  - Complete working example with Worker<MetalKernels>
  - Document requirements (model files, Metal GPU)
  
- [ ] 5.2 Implement `examples/lora_routing.rs`
  - Pure-Rust example (no mplora_mlx dependency)
  - Show router logic
  
- [ ] 5.3 Implement patch proposal examples
  - `examples/patch_proposal_api.rs`
  - `examples/patch_proposal_basic.rs`
  - `examples/patch_proposal_advanced.rs`
  - Use `RequestType::PatchProposal`
  
- [ ] 5.4 Verify Phase 5
  ```bash
  cargo run --example basic_inference
  cargo run --example lora_routing
  cargo run --example patch_proposal_basic
  ```

## Final Verification ⏳

- [ ] All tests compile
  ```bash
  cargo test --tests --examples --no-run
  ```

- [ ] No retired tests remain
  ```bash
  find tests examples -name "*.rs" -exec grep -l "cfg(any())" {} \; | wc -l
  # Expected: 0
  ```

- [ ] Core tests pass
  ```bash
  cargo test --tests
  # Expected: 150+ passed; 0 failed
  ```

- [ ] Documentation updated
  - [ ] Remove retirement warnings from README.md
  - [ ] Update TESTING_CHECKLIST.md
  - [ ] Update CURRENT_STATUS.md
  - [ ] Mark TEST_SUITE_RETIREMENT_SUMMARY.md as superseded

- [ ] Git commit
  ```bash
  git add .
  git commit -m "Complete test suite restoration
  
  All 26 retired test files and examples restored to full functionality.
  - Phase 1: Policy framework (PolicySpec.severity)
  - Phase 2: Worker & inference (InferenceRequest updates)
  - Phase 3: Config & federation (API migration)
  - Phase 4: Advanced features (patches, monitoring, UI)
  - Phase 5: Examples (working demonstrations)
  
  Refs: COMPREHENSIVE_PATCH_PLAN.md"
  ```

## Quick Commands Reference

```bash
# Check remaining retired files
find tests examples -name "*.rs" -exec grep -l "cfg(any())" {} \;

# Count TODO markers
grep -r "TODO:\|FIXME:" tests/ | wc -l

# Run specific test
cargo test --test <name>

# Run with ignored tests (Metal GPU)
cargo test --test <name> -- --include-ignored

# Compile without running
cargo test --tests --no-run

# Run all examples
for ex in basic_inference lora_routing patch_proposal_basic; do
    cargo run --example $ex
done
```

## Progress Tracking

Update this section as you complete phases:

- **Phase 1 Started:** YYYY-MM-DD
- **Phase 1 Complete:** YYYY-MM-DD
- **Phase 2 Started:** YYYY-MM-DD
- **Phase 2 Complete:** YYYY-MM-DD
- **Phase 3 Started:** YYYY-MM-DD
- **Phase 3 Complete:** YYYY-MM-DD
- **Phase 4 Started:** YYYY-MM-DD
- **Phase 4 Complete:** YYYY-MM-DD
- **Phase 5 Started:** YYYY-MM-DD
- **Phase 5 Complete:** YYYY-MM-DD
- **Final Verification:** YYYY-MM-DD

**Total Estimated Time:** 70-100 hours (9-13 days)
**Actual Time:** ___ hours (___ days)

## Notes & Blockers

Document any issues encountered:

```
Issue: 
Resolution: 
Date: 

Issue: 
Resolution: 
Date: 
```

