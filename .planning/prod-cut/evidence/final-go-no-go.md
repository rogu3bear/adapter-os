# Prod Cut Final Go/No-Go Receipt

- Generated at (UTC): 2026-03-02T08:27:55Z
- Decision: **NO-GO**
- Blocking reason: governance preflight remains `blocked_external` (`HTTP 403`), which is release-blocking by prod policy.

> 2026-03-02 policy update:
> Local release policy was revised to remove mandatory GitHub governance dependency.
> `scripts/ci/local_release_gate.sh` now defaults `LOCAL_RELEASE_GOVERNANCE_MODE=off` (optional governance with `warn|enforce`),
> so this receipt is historical for the prior strict-enforcement configuration.

## Command Receipts

1. Governance preflight (blocking):
   - Command: `bash scripts/ci/check_governance_preflight.sh --repo rogu3bear/adapter-os --branch main --required-context 'FFI AddressSanitizer (push)'`
   - Result: `status=blocked_external`, `exit_code=20`
   - Evidence: `.planning/prod-cut/evidence/governance/20260302T080306Z/preflight.log`
2. Governance capability loop + enforcement:
   - Commands:
     - `bash scripts/ci/run_governance_capability_loop.sh ...`
     - `bash scripts/ci/execute_governance_required_checks.sh ...`
   - Result: remained blocked; no capable branch transition.
   - Evidence:
     - `.planning/prod-cut/evidence/governance/20260302T080306Z/capability-loop.log`
     - `.planning/prod-cut/evidence/governance/20260302T080306Z/enforcement.log`
3. Runbook strict evidence validation:
   - Command: `RUNBOOK_DRILL_STRICT=1 bash scripts/ci/check_runbook_drill_evidence.sh`
   - Result: passed
   - Evidence:
     - `.planning/prod-cut/evidence/release/runbook_strict_check.log`
     - `.planning/prod-cut/evidence/runbooks/<scenario>/*`
4. Release artifact integrity + signing:
   - Command:
     - `OUT_DIR=target/release-bundle ARTIFACTS="target/release/aos-worker target/release/aosctl" SBOM_REQUIRE_SIGNING=1 RELEASE_SIGNING_KEY_PEM=<path> bash scripts/release/sbom.sh`
   - Result: passed, signed outputs emitted
   - Evidence:
     - `.planning/prod-cut/evidence/release/sbom.json`
     - `.planning/prod-cut/evidence/release/build_provenance.json`
     - `.planning/prod-cut/evidence/release/signature.sig`
     - `.planning/prod-cut/evidence/release/build_provenance.sig`
     - `.planning/prod-cut/evidence/release/release_verification.log`
5. Prod required checks:
   - Command: `LOCAL_REQUIRED_PROFILE=prod LOCAL_REQUIRED_CLIPPY_SCOPE=all-targets bash scripts/ci/local_required_checks.sh`
   - Result: passed (`exit_code=0`)
   - Evidence: `.planning/prod-cut/evidence/release/local_required_checks_prod-20260302T081230Z.log`
6. Strict prod gate rehearsal:
   - Command: `bash scripts/ci/local_release_gate_prod.sh`
   - Result: failed at governance preflight (after local required checks passed)
   - Evidence: `.planning/prod-cut/evidence/release/local_release_gate_prod-20260302T081822Z.log`
7. Latest rerun (this execution):
   - Governance preflight recheck:
     - Result: `status=blocked_external`, `exit_code=20`
     - Evidence: `.planning/prod-cut/evidence/governance/20260302T082747Z/preflight.log`
   - Strict prod gate rerun:
     - Result: failed at governance preflight (`exit_code=1`)
     - Evidence: `.planning/prod-cut/evidence/release/local_release_gate_prod-20260302T082710Z.log`

## Go Criteria Evaluation

1. Governance preflight capable (no `blocked_external`): **FAIL**
2. Strict runbook evidence check passes: **PASS**
3. SBOM/provenance/signatures present and verified: **PASS**
4. Local required checks pass in prod profile: **PASS**
5. `local_release_gate_prod.sh` passes end-to-end: **FAIL** (blocked at governance as expected by policy)

## Final Status

Production cut is **NO-GO** until repository/org capability is updated so governance preflight returns `status=capable` (`exit 0`).
