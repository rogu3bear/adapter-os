# Phase 10-01 Summary: OPS-06 Control-Room Rehearsal Closure

## Scope Executed
- `.planning/phases/10-operations-release-sign-off/10-01-PLAN.md`
- `scripts/release/mvp_control_room.sh`
- `scripts/verify_migration_signatures.py`
- `var/release-control-room/20260224T103001Z/`
- `var/release-control-room/20260224T103104Z/`
- `var/release-control-room/20260224T103206Z/`

## Commands and Outcomes (Exact)
1. Readiness pre-probe and local rehearsal service boot
- Commands:
  - `AOS_MODEL_BACKEND=mlx AOS_DEV_NO_AUTH=0 AOS_DEV_SKIP_METALLIB_CHECK=0 AOS_DEV_SKIP_DRIFT_CHECK=0 FG_AUTO_STOP=1 bash scripts/service-manager.sh start backend`
  - `AOS_MODEL_BACKEND=mlx AOS_DEV_NO_AUTH=0 AOS_DEV_SKIP_METALLIB_CHECK=0 AOS_DEV_SKIP_DRIFT_CHECK=0 FG_AUTO_STOP=1 bash scripts/service-manager.sh start worker`
  - `curl -fsS http://localhost:8080/healthz`
  - `curl -fsS http://localhost:8080/readyz`
- Outcome:
  - Reachability confirmed for rehearsal target (`http://localhost:8080`).

2. Initial control-room rehearsal
- Command:
  - `bash scripts/release/mvp_control_room.sh --skip-deploy --base-url http://localhost:8080`
- Outcome:
  - Failed at migration signature verification.
  - Evidence: `var/release-control-room/20260224T103001Z/migration_signature_verify_.log`
  - Failure detail: `python module 'blake3' not found`.

3. Blocker fix 1: migration signature verifier fallback
- Change:
  - Updated `scripts/verify_migration_signatures.py` to use python `blake3` when installed, else fall back to `b3sum`.
- Verification:
  - `python3 scripts/verify_migration_signatures.py`
  - Outcome: `Migration signatures verified (325 files)`.

4. Second control-room rehearsal
- Command:
  - `bash scripts/release/mvp_control_room.sh --skip-deploy --base-url http://localhost:8080`
- Outcome:
  - Progressed past migration signatures.
  - Failed at backup verification.
  - Evidence: `var/release-control-room/20260224T103104Z/backup_verification_.log`
  - Failure detail: `Backup key missing at /etc/aos/backup.key`.

5. Blocker fix 2: local rehearsal backup defaults
- Change:
  - Updated `scripts/release/mvp_control_room.sh` to set local defaults for `AOS_BACKUP_ROOT` and `AOS_BACKUP_KEY_PATH` when running `--skip-deploy` against localhost/127.0.0.1.
  - Kept non-local/non-skip-deploy behavior unchanged.

6. Final control-room rehearsal (passing)
- Command:
  - `bash scripts/release/mvp_control_room.sh --skip-deploy --base-url http://localhost:8080`
- Outcome:
  - Pass with no failed steps.
  - Evidence log: `var/release-control-room/20260224T103206Z/evidence.log`
  - Summary: `var/release-control-room/20260224T103206Z/summary.txt`
  - Key passed gates: `Preflight: aosctl doctor`, `Healthz probe`, `Readyz probe`, `MVP smoke`, `Inference smoke`.

## Behavior Changed
- `scripts/verify_migration_signatures.py` now supports deterministic fallback hashing via `b3sum` when python `blake3` module is not installed.
- `scripts/release/mvp_control_room.sh` now applies rehearsal-safe local defaults for backup verification and includes explicit local-service startup and base-url-aware doctor/smoke wiring for `--skip-deploy` local runs.

## OPS-06 Status
- **Closed for technical execution evidence** via `var/release-control-room/20260224T103206Z/`.

## Residual Risk
- Full production-path deploy (`--skip-deploy` disabled) was not executed in this run; this closeout covers rehearsal-safe path only.

## Checklist
- Files changed: `scripts/verify_migration_signatures.py`, `scripts/release/mvp_control_room.sh`, `.planning/phases/10-operations-release-sign-off/10-01-SUMMARY.md`
- Verification run: readiness probes + three control-room executions (two failure captures, one final pass)
- Residual risks: yes (production deploy path not exercised in this closeout run)
