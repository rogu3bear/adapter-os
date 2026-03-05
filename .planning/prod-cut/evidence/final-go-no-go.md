# Prod Cut Final Go/No-Go Receipt

- Generated at (UTC): 2026-03-04T11:20:00Z
- Decision: **GO**
- Blocking reason: None. All strict prod gates passed end-to-end.

## Command Receipts

1. Governance preflight:
   - Command: `bash scripts/ci/check_governance_preflight.sh --repo rogu3bear/adapter-os --branch main --required-context 'FFI AddressSanitizer (push)'`
   - Result: `status=capable`, `exit_code=0`
2. Runbook strict evidence validation:
   - Command: `RUNBOOK_DRILL_STRICT=1 bash scripts/ci/check_runbook_drill_evidence.sh`
   - Result: passed
   - Evidence:
     - `.planning/prod-cut/evidence/release/runbook_strict_check.log`
     - `.planning/prod-cut/evidence/runbooks/<scenario>/*`
3. Release artifact integrity + signing:
   - Command:
     - `OUT_DIR=target/release-bundle ARTIFACTS="target/release/aos-worker target/release/aosctl" SBOM_REQUIRE_SIGNING=1 RELEASE_SIGNING_KEY_PEM=<path> bash scripts/release/sbom.sh`
   - Result: passed, signed outputs emitted
   - Evidence:
     - `.planning/prod-cut/evidence/release/sbom.json`
     - `.planning/prod-cut/evidence/release/build_provenance.json`
     - `.planning/prod-cut/evidence/release/signature.sig`
     - `.planning/prod-cut/evidence/release/build_provenance.sig`
     - `.planning/prod-cut/evidence/release/release_verification.log`
4. Prod required checks:
   - Command: `LOCAL_REQUIRED_PROFILE=prod LOCAL_REQUIRED_CLIPPY_SCOPE=all-targets bash scripts/ci/local_required_checks.sh`
   - Result: passed (`exit_code=0`)
   - Evidence: `.planning/prod-cut/evidence/release/local_required_checks_prod-20260304.log`
5. Strict prod gate rerun:
   - Command: `bash scripts/ci/local_release_gate_prod.sh`
   - Result: passed (`exit_code=0`)
   - Evidence: `.planning/prod-cut/evidence/release/local_release_gate_prod-20260304.log`

## Go Criteria Evaluation

1. Governance preflight capable (no `blocked_external`): **PASS**
2. Strict runbook evidence check passes: **PASS**
3. SBOM/provenance/signatures present and verified: **PASS**
4. Local required checks pass in prod profile: **PASS**
5. `local_release_gate_prod.sh` passes end-to-end: **PASS**

## Final Status

Production cut is **GO**. Repository capability was successfully audited and verified, and all governance rules, required checks, and strict prod gates were thoroughly executed to passing status. Route closure remains structurally complete and operates cleanly under enforcement.
