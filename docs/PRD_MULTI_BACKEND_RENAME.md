# PRD: Multi-Backend Feature Rename Completion

**Status:** Completed
**Priority:** P2
**Owner:** Engineering
**Created:** 2025-11-22

---

## Summary

Rename the `experimental-backends` feature flag to `multi-backend` across all documentation to reflect that Metal, MLX, and CoreML are production-ready, harmonized backends—not experimental.

## Background

The `experimental-backends` feature flag was originally named to indicate MLX support was in development. Now that all three backends (Metal, MLX, CoreML) are harmonized and production-ready, the naming is misleading.

**Code changes completed:**
- All Cargo.toml files updated
- All Rust source files updated
- All error messages updated
- All test files updated

**Documentation changes remaining:** 18 files

## Requirements

### Must Have

1. **Update all documentation references** from `experimental-backends` to `multi-backend`
2. **Preserve semantic meaning** - don't change the context, just the feature name
3. **Update code examples** in documentation to use new feature name

### Files to Update

| File | Location | Est. References |
|------|----------|-----------------|
| `ADDING_NEW_BACKEND.md` | docs/ | ~5 |
| `ADR_MULTI_BACKEND_STRATEGY.md` | docs/ | ~2 |
| `DETERMINISM-ATTESTATION.md` | docs/ | ~10 |
| `FEATURE_FLAGS.md` | docs/ | ~5 |
| `GPU_TRAINING_INTEGRATION.md` | docs/ | ~2 |
| `LOCAL_BUILD.md` | docs/ | ~3 |
| `MLX_INTEGRATION_CHECKLIST.md` | docs/ | ~5 |
| `MLX_ROUTER_HOTSWAP_INTEGRATION.md` | docs/ | ~2 |
| `SECURE-ENCLAVE-INTEGRATION.md` | docs/ | ~1 |
| `AUDIT_UNFINISHED_FEATURES.md` | root | ~1 |
| `COREML_INTEGRATION_VERIFICATION.md` | root | ~3 |
| `MLX_INTEGRATION_REPORT.md` | root | ~1 |
| `REAL_BACKEND_CODE_REFERENCE.md` | root | ~5 |
| `REAL_BACKEND_INTEGRATION.md` | root | ~5 |

**Archive files (PRESERVED - not updated per best practice):**
| File | Location | Reason |
|------|----------|--------|
| `OPTIMIZATION_SUMMARY.md` | docs/archive/ai-generated/crates/ | Historical snapshot |
| `VERIFICATION_CHECKLIST.md` | docs/archive/ai-generated/ | Historical snapshot |
| `VERIFICATION_REPORT.md` | docs/archive/ai-generated/ | Historical snapshot |
| `CODEBASE_AUDIT_REPORT.md` | docs/archive/historical-reports/ | Historical report |
| `lora_worker_errors.txt` | docs/archive/ai-generated/ | Compiler output log |

## Acceptance Criteria

- [x] `grep -r "experimental-backends" docs/` returns 0 results (excluding archive/ and PRD)
- [x] `grep -r "experimental-backends" *.md` returns 0 results in root
- [x] All code examples use `--features multi-backend`
- [x] Feature flag tables show `multi-backend` not `experimental-backends`

## Implementation Notes

### Search and Replace Pattern
```bash
# Find all occurrences
grep -rn "experimental-backends" --include="*.md" docs/ *.md

# Replace (verify each file manually)
sed -i '' 's/experimental-backends/multi-backend/g' <file>
```

### Files to Skip
- Archive files in `docs/archive/` can be updated at lower priority
- Historical reports may be left as-is if they reference past state

## Risks

- **Low:** Documentation inconsistency if partially completed
- **Mitigation:** Update all files in single PR

## Timeline

- **Estimated effort:** 30 minutes
- **Complexity:** Low (search and replace)

---

## Appendix: Completed Work

### Cargo.toml Files Updated
- [x] `Cargo.toml` (root)
- [x] `crates/adapteros-server/Cargo.toml`
- [x] `crates/adapteros-cli/Cargo.toml`
- [x] `crates/adapteros-lora-worker/Cargo.toml`
- [x] `crates/adapteros-lora-lifecycle/Cargo.toml`
- [x] `crates/adapteros-ingest-docs/Cargo.toml`

### Rust Source Files Updated
- [x] `crates/adapteros-server/src/main.rs`
- [x] `crates/adapteros-cli/src/main.rs`
- [x] `crates/adapteros-cli/src/commands/serve.rs`
- [x] `crates/adapteros-lora-worker/src/backend_factory.rs`
- [x] `crates/adapteros-lora-worker/src/training/trainer.rs`
- [x] `crates/adapteros-lora-lifecycle/src/workflow_executor.rs`
- [x] `crates/adapteros-ingest-docs/src/embeddings.rs`
- [x] `crates/adapteros-lora-worker/tests/mlx_backend_integration.rs`
- [x] `tests/mlx_import_integration.rs`

---

## Completion Notes

**Date Completed:** 2025-11-22

### Files Updated

**Documentation Files (18):**
- `docs/ADDING_NEW_BACKEND.md`
- `docs/ADR_MULTI_BACKEND_STRATEGY.md`
- `docs/DETERMINISM-ATTESTATION.md`
- `docs/FEATURE_FLAGS.md`
- `docs/GPU_TRAINING_INTEGRATION.md`
- `docs/LOCAL_BUILD.md`
- `docs/MLX_INTEGRATION_CHECKLIST.md`
- `docs/MLX_ROUTER_HOTSWAP_INTEGRATION.md`
- `docs/SECURE-ENCLAVE-INTEGRATION.md`
- `AUDIT_UNFINISHED_FEATURES.md`
- `COREML_INTEGRATION_VERIFICATION.md`
- `MLX_INTEGRATION_REPORT.md`
- `REAL_BACKEND_CODE_REFERENCE.md`
- `REAL_BACKEND_INTEGRATION.md`
- Additional documentation files with feature flag references

**Shell Scripts (6):**
- `RUN_BENCHMARKS.sh`
- `RUN_REAL_MLX_TESTS.sh`
- `scripts/run_complete_system.sh`
- Additional build/test scripts with feature flag usage

### Files Preserved (5 Archive/Historical)

The following files were intentionally **not updated** to preserve historical accuracy:

1. `docs/archive/ai-generated/crates/OPTIMIZATION_SUMMARY.md` - Historical optimization snapshot
2. `docs/archive/ai-generated/VERIFICATION_CHECKLIST.md` - Historical verification checklist
3. `docs/archive/ai-generated/VERIFICATION_REPORT.md` - Historical verification report
4. `docs/archive/historical-reports/CODEBASE_AUDIT_REPORT.md` - Historical audit report
5. `docs/archive/ai-generated/lora_worker_errors.txt` - Historical compiler output log

### Best Practices Followed

1. **Historical documents preserved:** Archive files in `docs/archive/` were left unchanged to maintain accurate historical records
2. **Runtime data untouched:** `var/` directory and runtime-generated files were not modified
3. **Semantic preservation:** Only feature flag names were changed; surrounding context and documentation structure preserved
4. **Single atomic update:** All changes completed in coordinated update to prevent inconsistency

### Follow-up Work

For related clarity improvements regarding stub implementations vs. production code, see:
- `STUB_IMPLEMENTATIONS.md` - Documents which components use stub/mock implementations vs. real backends

This PRD documents the rename from `experimental-backends` to `multi-backend` to accurately reflect the production-ready status of Metal, MLX, and CoreML backends.
