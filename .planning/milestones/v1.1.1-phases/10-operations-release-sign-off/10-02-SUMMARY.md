# Phase 10-02 Summary: OPS-07 Signed SBOM/Provenance Bundle

## Scope Executed
- `.planning/phases/10-operations-release-sign-off/10-02-PLAN.md`
- `scripts/release/sbom.sh`
- `var/evidence/phase10/`
- `target/release/`
- `target/release-bundle/`

## Commands and Outcomes (Exact)
1. Build release binaries required by bundle generation
- Commands:
  - `CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 cargo build --release -p adapteros-server --bin aos-server | tee var/evidence/phase10/10-02-build-aos-server.log`
  - `cargo build --release -p adapteros-lora-worker --bin aos-worker | tee var/evidence/phase10/10-02-build-aos-worker.log`
  - `cargo build --release -p adapteros-cli --bin aosctl | tee var/evidence/phase10/10-02-build-aosctl.log`
- Outcome:
  - All three binaries built successfully.
  - Artifacts:
    - `target/release/aos-server` (206M)
    - `target/release/aos-worker` (19M)
    - `target/release/aosctl` (39M)

2. Generate signed SBOM/provenance bundle
- Command:
  - `RELEASE_SIGNING_KEY_PEM="$PWD/var/keys/release_signing_phase10.pem" bash scripts/release/sbom.sh 2>&1 | tee var/evidence/phase10/10-02-sbom.log`
- Outcome:
  - Signed bundle generated in `target/release-bundle/`.
  - Script reported legacy-name skips (`target/release/adapteros-server`, `target/release/aos_worker`) while still staging canonical artifacts.
  - `unsigned` warning text was not emitted.

3. Validate provenance linkage and required fields
- Commands:
  - `python3 - <<'PY' ...` (schema and required provenance keys assertion)
  - `test -s target/release-bundle/sbom.json && test -s target/release-bundle/build_provenance.json && test -s target/release-bundle/signature.sig && test -s target/release-bundle/build_provenance.sig`
- Outcome:
  - `provenance linkage ok`
  - Non-empty signed artifacts present:
    - `target/release-bundle/sbom.json`
    - `target/release-bundle/build_provenance.json`
    - `target/release-bundle/signature.sig`
    - `target/release-bundle/build_provenance.sig`

## Behavior Changed
- No product/runtime behavior change in this plan closeout; this run generated and verified release evidence artifacts.

## OPS-07 Status
- **Closed for technical execution evidence** via signed `target/release-bundle/` outputs and provenance linkage check.

## Residual Risk
- Build used `CARGO_PROFILE_RELEASE_LTO=off` for practical execution time in this environment; if governance requires strict production-profile parity, perform one additional canonical-profile release build before external distribution.
- `scripts/release/sbom.sh` still logs skipped legacy artifact names; non-blocking, but should be aligned with current canonical binary names to reduce audit ambiguity.

## Checklist
- Files changed: `.planning/phases/10-operations-release-sign-off/10-02-SUMMARY.md`
- Verification run: release binary existence checks, signed bundle non-empty checks, provenance linkage assertions, unsigned-warning scan
- Residual risks: yes (profile parity follow-up + legacy artifact-name log noise)
