# Documentation Pruning Plan

**Generated:** 2026-01-25  
**Last Updated:** 2026-02-18  
**Purpose:** Maintenance status and pruning guidance. Code is authoritative; docs must align.

## Status

| Phase | Status | Notes |
|-------|--------|-------|
| Phase 1: Untracked deletion | ✅ Done | 4 files removed |
| Phase 2: High-confidence deletion | ✅ Done | 14 files removed |
| Phase 3: README broken links | ✅ Done | 39 references fixed |
| Phase 4: Orphaned file review | Pending | 84 files |
| Phase 5: Broken links | Pending | 26 files |
| Phase 6: Nuke (2026-02-18) | ✅ Done | 5 files: 4 JSON artifacts + engineering/ROUTE_MAP.md |

**Canonical sources:** See [CANONICAL_SOURCES.md](CANONICAL_SOURCES.md). When docs conflict with code, code wins.

---

## Category 1: DELETE IMMEDIATELY (Untracked Files) — ✅ COMPLETED

These files are not in git and appear to be temporary agent-generated documents:

1. ✅ **`docs/API_LAYER_ANALYSIS_REVIEW.md`** - Agent review document (549 lines)
2. ✅ **`docs/TERMINOLOGY_CLARIFICATION.md`** - Agent clarification document (492 lines)
3. ✅ **`docs/development/stability/archive/README.md`** - Archive file
4. ✅ **`docs/engineering/TRAINING_HANDLER_SPLIT_ANALYSIS.md`** - Analysis document with broken links

**Action:** Delete all 4 files immediately.

---

## Category 2: HIGH PRIORITY PRUNE (Orphaned + Outdated Intent)

These files are tracked but:
- Not referenced in `docs/README.md`
- Not referenced by any other documentation
- Likely don't match current codebase intent

### Definitely Delete (No Current Value)

| File | Reason | Size |
|------|--------|------|
| `docs/CLAUDE.md` | Agent guidance file, should be in `.claude/` | 593 bytes |
| `docs/api/CLAUDE.md` | Agent guidance file | 169 bytes |
| `docs/security/CLAUDE.md` | Agent guidance file | 169 bytes |
| `docs/cli-chat-mvp.md` | MVP spec, likely outdated | 15 KB |
| `docs/internal/cli-chat-mvp-checklist.md` | Internal checklist | 3.9 KB |
| `docs/internal/cli-inventory.md` | Internal inventory | 2.6 KB |
| `docs/opencode_integration.md` | Integration spec, likely outdated | 1.6 KB |
| `docs/policy-hash-watcher.md` | Implementation detail, not user-facing | 2.3 KB |
| `docs/reference_mode.md` | Internal reference | 1.7 KB |
| `docs/reference_runbook.md` | Internal runbook | 5.7 KB |
| `docs/api_reference_curls.md` | Should be in examples/ | 1.9 KB |
| `docs/auth_contract.md` | Contract spec, should be in contracts/ | 6.5 KB |
| `docs/pilot_reference_contract.md` | Contract spec | 1.4 KB |
| `docs/fusion_interval.md` | Duplicate of hardening/fusion_interval.md | 1.4 KB |

### Review & Possibly Delete (May Have Historical Value)

| File | Last Modified | Size | Recommendation |
|------|---------------|------|----------------|
| `docs/BOOT_PHASES.md` | 2026-01-19 | 64 KB | **Keep** - Large, likely important |
| `docs/BOOT_READYZ_TRACE.md` | 2026-01-14 | 12 KB | Review - May be outdated |
| `docs/CHAOS_TEST_ARCHITECTURE.md` | 2025-12-25 | 14 KB | Review - Testing architecture |
| `docs/CONTENT_ADDRESSING_INTEGRITY_VERIFICATION.md` | 2026-01-14 | 17 KB | Review - Security-related |
| `docs/COREML_DETERMINISM_AUDIT_TRAILS.md` | 2026-01-23 | 15 KB | **Keep** - Recent, security-related |
| `docs/CP_WORKER_HANDSHAKE_MATRIX.md` | 2026-01-04 | 11 KB | Review - Protocol documentation |
| `docs/CRYPTO_RECEIPT_INTEGRATION.md` | 2026-01-14 | 10 KB | Review - Security integration |
| `docs/DETERMINISM_INVARIANTS.md` | 2026-01-14 | 11 KB | Review - May overlap with DETERMINISM.md |
| `docs/DEVELOPMENT_SQLX.md` | 2025-12-28 | 1.5 KB | **Delete** - Development note |
| `docs/DOCUMENTATION_DRIFT.md` | 2026-01-14 | 13 KB | **Keep** - Important framework |
| `docs/ENDPOINTS_TRUTH_TABLE.md` | 2026-01-14 | 25 KB | Review - May be outdated |
| `docs/EXECUTION_CONTRACT.md` | 2026-01-19 | 16 KB | **Keep** - Important contract |
| `docs/FEDERAL_COMPLIANCE.md` | 2026-01-14 | 9.5 KB | Review - Compliance documentation |
| `docs/HOT_SWAP_SCENARIOS.md` | 2026-01-14 | 45 KB | Review - Large, may be important |
| `docs/INCIDENT_RESPONSE.md` | 2026-01-14 | 10 KB | Review - Operations-related |
| `docs/KERNEL_HASH_TRACE.md` | 2026-01-24 | 11 KB | **Keep** - Recent, security-related |
| `docs/MLX_DETERMINISM.md` | 2026-01-14 | 9 KB | Review - May overlap with MLX_GUIDE.md |
| `docs/MLX_DETERMINISM_GAPS.md` | 2026-01-13 | 12 KB | Review - May be outdated |
| `docs/NAMING_CONVENTIONS.md` | 2026-01-17 | 8.6 KB | **Keep** - Important conventions |
| `docs/OPERATIONS_RUNBOOK.md` | 2026-01-17 | 3.1 KB | Review - May overlap with OPERATIONS.md |
| `docs/PROD_GATE.md` | 2026-01-14 | 7.6 KB | Review - Production gating |
| `docs/Q15_EDGE_CASE_TESTING.md` | 2026-01-14 | 12 KB | Review - Testing documentation |
| `docs/REVIEW_WORKFLOW.md` | 2026-01-14 | 9.2 KB | Review - Workflow documentation |
| `docs/SMOKE_TESTS.md` | 2026-01-14 | 1.2 KB | **Delete** - Too small, likely outdated |
| `docs/TRACE_A_RUN.md` | 2026-01-12 | 3.1 KB | Review - Debugging guide |
| `docs/UI_API_MAPPING.md` | 2026-01-14 | 9.1 KB | Review - May be outdated |
| `docs/UI_ROUTES.md` | 2026-01-14 | 4.6 KB | Review - May be outdated |
| `docs/WORKER_SETUP.md` | 2026-01-13 | 2.1 KB | Review - Setup guide |

### Subdirectory Files (Review by Category)

#### `docs/plans/` - Planning Documents
- **Recommendation:** Move to `.plans/` or archive
- Files: 6 planning documents (embedding-benchmark, PLAN_*, cli-http-client, pagination-consolidation)
- **Action:** Archive or move to internal planning directory

#### `docs/hardening/` - Security Hardening PRs
- **Recommendation:** **Keep** - Important security documentation
- Files: 6 PR documents + 4 supporting docs
- **Action:** Keep, but verify they're still relevant

#### `docs/design/` - Design Documents
- **Recommendation:** Review individually
- Files: 5 design documents
- **Action:** Review each for current relevance

#### `docs/engineering/` - Engineering Notes
- **Recommendation:** Review individually
- Files: 5 engineering documents
- **Action:** Review each for current relevance

#### `docs/runbooks/` - Production Runbooks
- **Recommendation:** **Keep** - Critical operational docs
- Files: 7 runbooks (some already in README.md)
- **Action:** Keep, but fix broken links

#### `docs/training/` - Training Documentation
- **Recommendation:** Review individually
- Files: 3 training documents
- **Action:** Review each for current relevance

---

## Category 3: FIX BROKEN LINKS

### Critical (README.md)
**39 broken references in README.md** - These need immediate attention:

Missing files referenced in README.md:
- `CONCEPTS.md`, `CONFIG_PRECEDENCE.md`, `COREML_ATTESTATION_DETAILS.md`
- `CRYPTO.md`, `CRYPTO_SECURITY_S6_S9.md`, `DATASET_TRAINING_INTEGRATION.md`
- `DEV_BYPASS_POLICY.md`, `FFI_GUIDE.md`, `GPU_TRAINING_INTEGRATION.md`
- `INFERENCE_FLOW.md`, `METAL_BUILD_SYSTEM_INTEGRATION.md`, `METAL_TOOLCHAIN_SETUP.md`
- `MLX_BACKEND_DEPLOYMENT_GUIDE.md`, `MLX_HKDF_SEEDING.md`, `MLX_INSTALLATION_GUIDE.md`
- `MLX_MEMORY.md`, `MLX_METAL_DEVICE_ACCESS.md`, `MLX_MIGRATION_GUIDE.md`
- `MLX_QUICK_REFERENCE.md`, `MLX_ROUTER_HOTSWAP_INTEGRATION.md`, `MLX_VS_COREML_GUIDE.md`
- `OBJECTIVE_CPP_FFI_PATTERNS.md`, `POLICY_ENFORCEMENT.md`, `POLICY_ENFORCEMENT_MIDDLEWARE.md`
- `POLICY_ENFORCEMENT_MIDDLEWARE_IMPLEMENTATION_GUIDE.md`, `PRODUCTION_BACKUP_RESTORE.md`
- `PRODUCTION_MONITORING.md`, `PRODUCTION_OPERATIONS.md`, `RUNBOOK_TRAINING_LINEAGE_TRUST.md`
- `SECURE_ENCLAVE_INTEGRATION_ENHANCED.md`, `STUB_IMPLEMENTATIONS.md`
- `TRAINING_PROVENANCE.md`, `TRAINING_VERSIONING.md`, `USER_FLOW.md`
- `USER_GUIDE_DATASETS.md`, `coreml_training_backend.md`, `keychain-integration.md`
- `policy-engine-outline.md`, `secure-enclave-integration.md`

**Action:** Either:
1. Remove broken references from README.md, OR
2. Create placeholder files with "TODO: Document this", OR
3. Update references to point to existing equivalent docs

### Other Files with Broken Links
- `docs/runbooks/README.md` - 15 broken links (relative path issues)
- `docs/MLX_GUIDE.md` - 6 broken links
- `docs/GLOSSARY.md` - 5 broken links
- `docs/ARCHITECTURE.md` - 3 broken links
- Various other files with 1-2 broken links

**Action:** Fix relative paths and update to point to existing files.

---

## Execution Status

✅ **Phase 1: COMPLETED** - Deleted 4 untracked files  
✅ **Phase 2: COMPLETED** - Deleted 14 high-confidence files  
✅ **Phase 3: COMPLETED** - Fixed all 39 broken references in README.md  

**Results:**
- Files deleted: 18 total
- Before: 145 markdown files
- After: 128 markdown files
- README.md broken references: 0 (was 39)

---

## Recommended Pruning Actions

### Phase 1: Immediate Deletion (4 files) ✅ COMPLETED
```bash
# Delete untracked files
rm docs/API_LAYER_ANALYSIS_REVIEW.md
rm docs/TERMINOLOGY_CLARIFICATION.md
rm docs/development/stability/archive/README.md
rm docs/engineering/TRAINING_HANDLER_SPLIT_ANALYSIS.md
```

### Phase 2: High-Confidence Deletion (14 files) ✅ COMPLETED
```bash
# Agent guidance files (should be in .claude/)
rm docs/CLAUDE.md
rm docs/api/CLAUDE.md
rm docs/security/CLAUDE.md

# Internal/development notes
rm docs/cli-chat-mvp.md
rm docs/internal/cli-chat-mvp-checklist.md
rm docs/internal/cli-inventory.md
rm docs/opencode_integration.md
rm docs/policy-hash-watcher.md
rm docs/reference_mode.md
rm docs/reference_runbook.md
rm docs/api_reference_curls.md
rm docs/DEVELOPMENT_SQLX.md
rm docs/SMOKE_TESTS.md
rm docs/fusion_interval.md  # Duplicate
```

### Phase 3: Fix README.md (39 broken references) ✅ COMPLETED
1. ✅ Removed all broken references
2. ✅ Updated links to point to existing files where appropriate
3. ✅ Consolidated redundant sections

### Phase 4: Review Orphaned Files (84 files)
1. Review each orphaned file for current relevance
2. Either:
   - Delete if outdated
   - Integrate into README.md if valuable
   - Move to archive if historical value only

### Phase 5: Fix Broken Links (26 files)
1. Fix relative path issues in runbooks/
2. Update broken links to point to existing files
3. Remove links to non-existent files

---

## Success Metrics

After pruning:
- ✅ 0 untracked files in docs/
- ✅ All files in README.md exist
- ✅ 0 broken internal links
- ✅ All tracked files are either:
  - Referenced in README.md, OR
  - Referenced by other docs, OR
  - Explicitly marked as internal/archive

---

## Next Steps

1. Phase 4: Review orphaned files; delete, integrate, or archive
2. Phase 5: Fix broken links in runbooks/ and other files
3. Run `./scripts/ci/check_docs_grounding.sh` before commits

---

## Notes

- **Code is authoritative** — When docs conflict with implementation, update docs
- **Documentation drift** — Use [DOCUMENTATION_DRIFT.md](DOCUMENTATION_DRIFT.md) for invariant validation
- **Grounding check** — Run `./scripts/ci/check_docs_grounding.sh` to catch forbidden patterns
