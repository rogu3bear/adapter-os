# Staging Branches Reference

**Created:** 2025-01-15  
**Base Commit:** `8271685a1787c2ea219a1e02fc6a310a635270c1`

---

## Branch Listing

All staging branches were created from the same base commit to ensure consistency:

| Branch Name | Purpose | Files Affected |
|------------|---------|----------------|
| `staging/aos2-format` | Complete AOS 2.0 safetensors parsing | `aos2_implementation.rs` |
| `staging/keychain-integration` | macOS/Linux keychain implementation | `crates/adapteros-crypto/src/providers/keychain.rs` |
| `staging/domain-adapters-executor` | Domain adapter executor integration | `crates/adapteros-server-api/src/handlers/domain_adapters.rs` |
| `staging/determinism-policy-validation` | Backend attestation validation | `crates/adapteros-lora-worker/src/lib.rs`, `inference_pipeline.rs` |
| `staging/system-metrics-postgres` | PostgreSQL support & migrations | `crates/adapteros-system-metrics/src/database.rs` |
| `staging/streaming-api-integration` | SSE endpoint real data integration | `crates/adapteros-server-api/src/handlers.rs` |
| `staging/federation-daemon-integration` | Federation daemon startup | `crates/adapteros-server/src/main.rs` |
| `staging/repository-codegraph-integration` | Framework detection & metadata | `crates/adapteros-server-api/src/handlers.rs` |
| `staging/testing-infrastructure` | Test setup completion | `tests/ui_integration.rs` |
| `staging/ui-backend-integration` | UI component backend APIs | `ui/src/components/*.tsx` |

---

## Usage

To work on a specific incomplete feature:

```bash
# Switch to the appropriate staging branch
git checkout staging/<feature-name>

# Make changes and commit
git add <files>
git commit -m "feat: complete <feature-name> implementation"

# When complete, merge back to main
git checkout main
git merge staging/<feature-name>
```

---

## Commit Reference

All branches share the same base commit:

```
8271685a1787c2ea219a1e02fc6a310a635270c1
```

This ensures each branch starts from a known, stable state.

---

## Detailed Audit

See `INCOMPLETE_FEATURES_AUDIT.md` for complete details on each incomplete feature, including:
- Exact file locations and line numbers
- Code citations
- Completion requirements
- Risk assessments

