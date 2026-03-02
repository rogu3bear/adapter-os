# AdapterOS MVP Production Control Room

Purpose: execute MVP launch with one source of operational truth on launch day.

Primary checklist: `MVP_PROD_CHECKLIST.md`

Incident runbooks: `docs/runbooks/README.md`

---

## 1) Roles and Ownership (`REQUIRED`)

Fill before launch:

- Release Commander:
- Ops Commander:
- DB Owner:
- Security Owner:
- API/UI Owner:
- Communications Owner:
- Rollback Approver:

Rules:

- `REQUIRED` Only Release Commander may call `GO` or `ROLLBACK`.
- `REQUIRED` All deployment commands are posted in incident/release channel before execution.
- `REQUIRED` Every command result is recorded (success/failure + timestamp).

---

## 2) Launch Timeline

## T-24h (Readiness Lock)

- [ ] Freeze non-release merges.
- [ ] Confirm all `REQUIRED` checks in `MVP_PROD_CHECKLIST.md` are complete.
- [ ] Validate backups and restore test evidence.
- [ ] Confirm on-call availability and escalation path.

Commands:

```bash
bash scripts/backup/verify-backups.sh
bash scripts/backup/test-restore.sh
bash scripts/ci/stability.sh
```

## T-2h (Staging Rehearsal)

- [ ] Deploy release candidate to staging.
- [ ] Run smoke + readiness.
- [ ] Validate determinism-sensitive behavior.
- [ ] Review logs and latency.

Commands:

```bash
bash scripts/deploy-production.sh
bash scripts/verify-deployment.sh
bash scripts/mvp_smoke.sh
bash scripts/smoke-inference.sh
bash scripts/ui_smoke.sh
```

## T-30m (Production Preflight)

- [ ] Confirm branch protection and required checks still green.
- [ ] Confirm no open Sev1/Sev2 incident.
- [ ] Confirm rollback artifact and migration plan ready.

Commands:

```bash
./start preflight
./aosctl preflight
./aosctl doctor
bash scripts/check_migration_conflicts.sh
python3 scripts/verify_migration_signatures.py
```

## T0 (Production Deploy)

- [ ] Announce start.
- [ ] Execute deployment.
- [ ] Run health probes.
- [ ] Run smoke suite.

Commands:

```bash
bash scripts/deploy-production.sh
bash scripts/verify-deployment.sh
curl -f http://localhost:8080/healthz
curl -f http://localhost:8080/readyz
bash scripts/mvp_smoke.sh
```

## T+30m to T+60m (Stabilization Window)

- [ ] Track error rate/latency/worker stability.
- [ ] Confirm no determinism violations.
- [ ] Decide `GO-STABLE` or `ROLLBACK`.

---

## 3) Go/No-Go Decision Card (`REQUIRED`)

Mark each as `PASS` or `FAIL`:

- [ ] CI required checks green on release SHA.
- [ ] Health and readiness probes green.
- [ ] MVP smoke tests green.
- [ ] No sustained 5xx regression.
- [ ] No major latency regression (P95/P99).
- [ ] No data correctness/determinism regression.
- [ ] No active security blocker.

Decision:

- Final call: `GO`
- Called by: Release Commander (assumed by user directive)
- Timestamp: 2026-02-24T11:37:45Z
- Notes: Decision captured in `.planning/phases/10-operations-release-sign-off/10-GO-NO-GO-EVIDENCE.md`.

Current rehearsal evidence (2026-02-24):

- Control-room evidence log: `var/release-control-room/20260224T103206Z/evidence.log`
- Control-room summary: `var/release-control-room/20260224T103206Z/summary.txt`
- Signed artifact bundle: `target/release-bundle/`
- Final GO/NO-GO package: `.planning/phases/10-operations-release-sign-off/10-GO-NO-GO-EVIDENCE.md`

Interim gate readout from rehearsal:

- CI required checks on release SHA: `PASS (Release Commander GO assumption in this execution context)`
- Health and readiness probes: `PASS`
- MVP smoke and inference smoke: `PASS`
- Security blocker status: `PASS (no active blocker recorded in GO package)`

---

## 4) Hard Rollback Triggers (`REQUIRED`)

Immediate rollback if any of the following persist beyond agreed grace period:

- [ ] Readiness failing repeatedly.
- [ ] Sustained elevated 5xx.
- [ ] Determinism or data integrity regression.
- [ ] Security regression with active exploitation risk.
- [ ] Migration side effect causing production outage.

Runbook references:

- `migrations/rollbacks/README.md`
- `.github/workflows/deploy.yml` (`Rollback on Failure`)
- `docs/runbooks/WORKER_CRASH.md`
- `docs/runbooks/DETERMINISM_VIOLATION.md`

---

## 5) Evidence Log Template

Use this per step:

```text
[timestamp] [owner] [step]
command: <exact command>
result: PASS|FAIL
notes: <short evidence line>
artifact: <path/link>
```

---

## 6) Command Bundle (Operator Fast Path)

Run in order and stop on failure:

```bash
bash scripts/release/mvp_control_room.sh --base-url http://localhost:8080
```

Evidence output:

- `var/release-control-room/<timestamp>/evidence.log`
- `var/release-control-room/<timestamp>/summary.txt`

---

## 7) Post-Launch Actions

- [ ] Publish release status summary.
- [ ] Archive command/evidence logs.
- [ ] Open follow-up tickets for non-blocking issues.
- [ ] Update `CHANGELOG.md` if needed.
