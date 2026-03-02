# Phase 06-01 Summary: Production Operations (OPS-01..OPS-05) - Verification Closeout

## Scope Executed
- `.planning/phases/06-production-operations/06-01-PLAN.md`
- `scripts/backup/ci-smoke.sh`
- `scripts/backup/backup.sh`
- `scripts/backup/verify-backups.sh`
- `scripts/backup/test-restore.sh`
- `scripts/release/sbom.sh`
- `scripts/release/mvp_control_room.sh`
- release artifacts under `target/release/*`

Product/script edits were made to stabilize backup/restore execution and release artifact staging.

## Commands and Outcomes (Exact)
1. `bash scripts/backup/ci-smoke.sh`
- Outcome:
  - Completed successfully end-to-end:
    - backup created
    - backup verified
    - restore path validated
  - Environment note:
    - OpenSSL AEAD (`aes-256-gcm`) unavailable on this host, flow fell back to `aes-256-cbc` with explicit warning.

2. `cargo build --release -p adapteros-server --bin aos-server --config profile.release.lto=false --config profile.release.codegen-units=16`
- Outcome:
  - Completed successfully:
    - `Finished 'release' profile [optimized] target(s) in 8m 11s`

3. `cargo build --release -p adapteros-lora-worker --bin aos-worker --config profile.release.lto=false --config profile.release.codegen-units=16`
- Outcome:
  - Completed successfully:
    - `Finished 'release' profile [optimized] target(s) in 3m 17s`

4. `cargo build --release -p adapteros-cli --bin aosctl --config profile.release.lto=false --config profile.release.codegen-units=16`
- Outcome:
  - Completed successfully:
    - `Finished 'release' profile [optimized] target(s) in 8m 01s`

5. `bash scripts/release/sbom.sh`
- Outcome:
  - Completed successfully:
    - `Skipping missing artifact: target/release/adapteros-server`
    - `Skipping missing artifact: target/release/aos_worker`
    - `SBOM + provenance staged in .../target/release-bundle`
  - Note:
    - unsigned output (expected in this run): `RELEASE_SIGNING_KEY_PEM not set`

6. `bash scripts/release/mvp_control_room.sh --skip-deploy --base-url http://localhost:8080`
- Outcome:
  - Failed at control-room doctor step:
    - `FAIL: Preflight: aosctl doctor`
    - `ERROR: stopping at failed step: Preflight: aosctl doctor`
    - doctor error: `Failed to connect to server at http://localhost:8080` (`Connection refused`)
  - Evidence artifacts produced:
    - `var/release-control-room/20260224T063105Z/evidence.log`
    - `var/release-control-room/20260224T063105Z/preflight_aosctl_doctor_.log`

## Behavior Changed
- Backup smoke now invokes subordinate scripts via `bash`, removing execute-bit dependency in this workspace.
- Backup encrypt/decrypt scripts now negotiate OpenSSL cipher support (`aes-256-gcm` preferred, `aes-256-cbc` fallback).
- Backup verification now excludes generated checksum working file to avoid self-referential mismatch.
- SBOM default artifact list now includes `aos-server` and `aos-worker` binary names used by current workspace.

## GO/NO-GO Recommendation
- **Conditional NO-GO** for full Phase 6 closeout.
- Rationale: OPS-02 and OPS-03 now have fresh passing evidence, but OPS-05 control-room run remains blocked by missing live service at `localhost:8080` during doctor/health steps.

## Residual Risk
- `OPS-05` control-room rehearsal currently depends on a reachable server (`/healthz/all`) for `aosctl doctor`; in this rehearsal run no listener existed at `http://localhost:8080`.
- SBOM/provenance artifacts were generated unsigned in this run because `RELEASE_SIGNING_KEY_PEM` was not provided.
- `OPS-04` checklist reconciliation is still pending finalization after successful control-room completion.

## Checklist
- Files changed: `scripts/backup/ci-smoke.sh`, `scripts/backup/backup.sh`, `scripts/backup/verify-backups.sh`, `scripts/backup/test-restore.sh`, `scripts/release/sbom.sh`, `.planning/phases/06-production-operations/06-01-SUMMARY.md`
- Verification run: backup smoke (pass), release builds (`aos-server`/`aos-worker`/`aosctl`) pass, SBOM script (pass, unsigned), control-room rehearsal (fail at doctor)
- Residual risks: control-room requires live server availability, signing key not supplied for signed provenance
