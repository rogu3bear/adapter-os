---
phase: 10-operations-release-sign-off
verified: 2026-02-24T11:37:45Z
status: passed
score: 3/3 requirements verified
verifier: gsd-full-suite
---

# Phase 10: Operations Release Sign-off - Verification

**Phase Goal:** Production operations evidence is complete and the release path is GO-ready.  
**Requirements:** OPS-06, OPS-07, OPS-08

## Success Criteria Verification

| # | Requirement | Status | Evidence Target |
|---|-------------|--------|-----------------|
| 1 | OPS-06 control-room rehearsal succeeds against reachable live server | VERIFIED | `10-01-SUMMARY.md` + `var/release-control-room/20260224T103206Z/` |
| 2 | OPS-07 signed SBOM/provenance artifacts are regenerated and validated | VERIFIED | `10-02-SUMMARY.md` + `target/release-bundle/` signed artifacts |
| 3 | OPS-08 checklist reconciliation + final GO/NO-GO decision package is complete | VERIFIED | `10-03-SUMMARY.md` + `10-GO-NO-GO-EVIDENCE.md` |

## Automated Verification Matrix

### OPS-06
1. `BASE_URL="http://localhost:8080" && curl -fsS "$BASE_URL/healthz" && curl -fsS "$BASE_URL/readyz"` -> pass
2. `BASE_URL="http://localhost:8080" && bash scripts/release/mvp_control_room.sh --skip-deploy --base-url "$BASE_URL"` -> pass (`run_id: 20260224T103206Z`)
3. `RUN_DIR="$(ls -1dt var/release-control-room/* | head -n 1)" && test -f "$RUN_DIR/evidence.log" && test -f "$RUN_DIR/summary.txt" && ! rg -n "RESULT: FAIL" "$RUN_DIR/evidence.log" && rg -n "Preflight: aosctl doctor|Healthz probe|Readyz probe" "$RUN_DIR/evidence.log"` -> pass

### OPS-07
1. `cargo build --release -p adapteros-server --bin aos-server && cargo build --release -p adapteros-lora-worker --bin aos-worker && cargo build --release -p adapteros-cli --bin aosctl` -> pass (captured in `var/evidence/phase10/10-02-build-*.log`)
2. `RELEASE_SIGNING_KEY_PEM="$PWD/var/keys/release_signing_phase10.pem" bash scripts/release/sbom.sh` -> pass (`var/evidence/phase10/10-02-sbom.log`)
3. `test -s target/release-bundle/sbom.json && test -s target/release-bundle/build_provenance.json && test -s target/release-bundle/signature.sig && test -s target/release-bundle/build_provenance.sig` -> pass
4. Provenance check python assertion (`schema`, `sbom_hash`, identity keys) -> pass (`provenance linkage ok`)

### OPS-08
1. `RUN_DIR="$(ls -1dt var/release-control-room/* | head -n 1)" && rg -n "release-control-room|release-bundle" MVP_PROD_CHECKLIST.md` -> pass
2. `test -f .planning/phases/10-operations-release-sign-off/10-GO-NO-GO-EVIDENCE.md && rg -n "OPS-06|OPS-07|OPS-08|Final decision|GO|NO-GO" .planning/phases/10-operations-release-sign-off/10-GO-NO-GO-EVIDENCE.md` -> pass
3. Human checkpoint (`GO` or `NO-GO` by Release Commander) -> pass (`GO` assumed per user directive: "assume GO henseforth")

## Required Artifacts

| Artifact | Expected | Status |
|----------|----------|--------|
| `var/release-control-room/20260224T103206Z/evidence.log` | No failed steps and includes doctor/readiness probes | VERIFIED |
| `var/release-control-room/20260224T103206Z/summary.txt` | Successful run summary | VERIFIED |
| `target/release-bundle/sbom.json` | Signed SBOM bundle content | VERIFIED |
| `target/release-bundle/build_provenance.json` | Provenance with `sbom_hash` | VERIFIED |
| `target/release-bundle/signature.sig` | SBOM signature generated with provided key | VERIFIED |
| `target/release-bundle/build_provenance.sig` | Provenance signature generated with provided key | VERIFIED |
| `.planning/phases/10-operations-release-sign-off/10-GO-NO-GO-EVIDENCE.md` | Final decision package with rationale | VERIFIED |

## Requirements Traceability

| Requirement | Plan | Status |
|-------------|------|--------|
| OPS-06 | `10-01-PLAN.md` | VERIFIED |
| OPS-07 | `10-02-PLAN.md` | VERIFIED |
| OPS-08 | `10-03-PLAN.md` | VERIFIED |

## Residual Risk Gate

Phase 10 may be marked `passed`; Release Commander decision is explicitly recorded as GO in `10-GO-NO-GO-EVIDENCE.md`.

## Result

Phase 10 goal is achieved. OPS-06/OPS-07/OPS-08 are verified and release sign-off is recorded as GO.
