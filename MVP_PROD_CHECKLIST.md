# AdapterOS MVP Production Checklist

Purpose: production go-live checklist for MVP with strict pass/fail gates.

Scope: this checklist is grounded in current repo assets and scripts. Items marked `REQUIRED` block launch.

Launch execution companion: `MVP_PROD_CONTROL_ROOM.md`

Owner: fill before release.

Release window: fill before release.

---

## 0) Release Controls (`REQUIRED`)

- [ ] `REQUIRED` Freeze merge policy during launch window (only release fixes).
- [ ] `REQUIRED` Enable branch protection on default branch.
- [ ] `REQUIRED` Require pull request review from Code Owners.
- [ ] `REQUIRED` Require status checks to pass before merge.
- [ ] `REQUIRED` Disable force pushes to protected branches.
- [ ] `REQUIRED` Disable direct pushes to protected branches.
- [ ] `REQUIRED` Announce release manager and incident commander.

Evidence:
- Branch protection screenshot or policy export.
- Link to release issue/ticket.

---

## 1) Environment and Secrets (`REQUIRED`)

- [ ] `REQUIRED` Production secrets are in secret manager, not committed.
- [ ] `REQUIRED` `.env.example` reflects all required runtime keys.
- [ ] `REQUIRED` Dev bypass auth is disabled in production (`AOS_DEV_NO_AUTH` unset).
- [ ] `REQUIRED` `configs/cp.toml` production values reviewed.
- [ ] `REQUIRED` `determinism_mode` set intentionally for production.

Commands:

```bash
./start preflight
./aosctl preflight
./scripts/check-config.sh
./scripts/validate_env.sh
./scripts/check_env_paths.sh
```

Evidence:
- Preflight output attached.
- Config review sign-off.

---

## 2) CI Gates to Require (`REQUIRED`)

Remote GitHub Actions are not part of release governance for this repository.
All required gates run locally and must be captured as local evidence.

- [ ] `REQUIRED` Run local baseline gate: `bash scripts/ci/local_required_checks.sh`
- [ ] `REQUIRED` Run local release gate: `bash scripts/ci/local_release_gate.sh`
- [ ] `REQUIRED` Ensure governance preflight outcome is recorded by local gate (`capable`, `blocked_external`, or `misconfigured`)
- [ ] `REQUIRED` Treat `blocked_external` governance result as non-blocking with evidence retained
- [ ] `REQUIRED` Keep local contract suite green: `bash scripts/contracts/check_all.sh`
- [ ] `REQUIRED` Keep local stability lane green: `bash scripts/ci/stability.sh`

Commands:

```bash
bash scripts/ci/local_required_checks.sh
bash scripts/ci/local_release_gate.sh
```

Local mirrors for fast pre-release validation:

```bash
bash scripts/ci/check_openapi_drift.sh
bash scripts/ci/check_worker_contract_drift.sh
bash scripts/ci/check_anchor_contract.sh
bash scripts/ci/check_docs_grounding.sh
bash scripts/ci/check_test_artifacts.sh
bash scripts/ci/check_runtime_log_contract.sh
bash scripts/ci/check_ui_assets.sh
bash scripts/ci/check_ui_component_similarity.sh
```

---

## 3) Determinism and Contract Integrity (`REQUIRED`)

- [ ] `REQUIRED` Determinism core suite passes.
- [ ] `REQUIRED` Router determinism tests pass.
- [ ] `REQUIRED` Replay determinism tests pass.
- [ ] `REQUIRED` Fast-math flag scan passes.
- [ ] `REQUIRED` Determinism references are current and reviewed if updated.

Commands:

```bash
cargo test --test determinism_core_suite
cargo test -p adapteros-lora-router --test determinism
cargo test -p adapteros-server-api --test replay_determinism_tests
bash scripts/check_fast_math_flags.sh
bash scripts/test/run_replay_determinism_tests.sh
```

Contracts and API:

- [ ] `REQUIRED` `docs/api/openapi.json` is in sync with code.
- [ ] `REQUIRED` Route inventory/openapi coverage checks pass.
- [ ] `REQUIRED` Startup/handler/error-code contract checks pass.

Commands:

```bash
bash scripts/contracts/check_all.sh
bash scripts/contracts/check_contract_artifacts.sh
bash scripts/contracts/check_startup_contract.sh
bash scripts/contracts/check_handler_error_response_with_code.sh
python3 scripts/contracts/check_api_route_tiers.py
python3 scripts/contracts/check_ui_routes.py
python3 scripts/contracts/check_middleware_chain.py
```

---

## 4) Security Minimum Bar (`REQUIRED`)

- [ ] `REQUIRED` Dependency vulnerability audit clean or explicitly accepted with risk sign-off.
- [ ] `REQUIRED` Security regression tests green.
- [ ] `REQUIRED` No hardcoded secrets in tracked files.
- [ ] `REQUIRED` Admin/owner endpoints access controls validated.
- [ ] `REQUIRED` `SECURITY.md` incident/reporting path confirmed.

Commands:

```bash
bash scripts/security_audit.sh
cargo audit
bash scripts/audit_api_endpoints.sh
bash scripts/contracts/check_docs_claims.sh
```

Evidence:
- Security report artifact link from CI.
- Exception list (if any) with owner and expiry date.

---

## 5) Database, Migrations, and Rollback (`REQUIRED`)

- [ ] `REQUIRED` Migration conflict checks pass.
- [ ] `REQUIRED` Migration signatures verified.
- [ ] `REQUIRED` Forward migration test passes on clean DB.
- [ ] `REQUIRED` Rollback test passes.
- [ ] `REQUIRED` Rollback procedure reviewed for impacted migrations.

Commands:

```bash
bash scripts/check_migration_conflicts.sh
bash scripts/check_migrations.sh
bash scripts/db/check_migrations.sh
python3 scripts/verify_migration_signatures.py
bash scripts/sign_migrations.sh
```

Rollback docs to review:

- `migrations/rollbacks/README.md`
- `migrations/rollbacks/QUICK_REFERENCE.md`
- `migrations/rollbacks/INDEX.md`

---

## 6) Backup and Restore (`REQUIRED`)

- [ ] `REQUIRED` Backup job configured for production.
- [ ] `REQUIRED` Backup verification succeeds.
- [ ] `REQUIRED` Restore test executed before launch.
- [ ] `REQUIRED` Backup retention window documented.

Commands:

```bash
bash scripts/backup/backup.sh
bash scripts/backup/verify-backups.sh
bash scripts/backup/ci-smoke.sh
bash scripts/backup/test-restore.sh
```

Operational assets:

- `deploy/cron/adapteros-backup`
- `scripts/backup/cron.example`

---

## 7) Build, Artifact, and Provenance (`REQUIRED`)

- [ ] `REQUIRED` Release build succeeds from clean checkout.
- [ ] `REQUIRED` SBOM generated and stored with release artifacts.
- [ ] `REQUIRED` Provenance bundle generated for release.
- [ ] `REQUIRED` Build metadata captured for traceability.

Commands:

```bash
cargo build --release
bash scripts/release/sbom.sh
bash scripts/build_metadata.sh
```

Reference local gates:

- `bash scripts/ci/local_required_checks.sh`
- `bash scripts/ci/local_release_gate.sh`

---

## 8) Staging Dress Rehearsal (`REQUIRED`)

- [ ] `REQUIRED` Deploy to staging using same path as production.
- [ ] `REQUIRED` Health checks pass in staging.
- [ ] `REQUIRED` Smoke tests pass in staging.
- [ ] `REQUIRED` Determinism spot-check passes on staging workload.
- [ ] `REQUIRED` Observability data visible in logs/metrics.

Commands:

```bash
bash scripts/deploy-production.sh
bash scripts/verify-deployment.sh
bash scripts/mvp_smoke.sh
bash scripts/smoke-inference.sh
bash scripts/ui_smoke.sh
bash scripts/demo-smoke.sh
```

Manual checks:

- `curl -f http://<staging-host>:18080/healthz`
- `curl -f http://<staging-host>:18080/readyz`

---

## 9) Production Deployment Runbook (`REQUIRED`)

Ordered launch steps:

- [ ] `REQUIRED` Confirm all required local checks are green on release commit.
- [ ] `REQUIRED` Confirm backup completed in last 24h and restore test recently passed.
- [ ] `REQUIRED` Run preflight checks on production environment.
- [ ] `REQUIRED` Apply deployment.
- [ ] `REQUIRED` Run post-deploy health and smoke checks.
- [ ] `REQUIRED` Monitor for 30-60 minutes before declaring success.

Commands:

```bash
./start preflight
./aosctl doctor
bash scripts/deploy-production.sh
bash scripts/verify-deployment.sh
bash scripts/mvp_smoke.sh
curl -f http://localhost:18080/healthz
curl -f http://localhost:18080/readyz
```

References:

- `docs/DEPLOYMENT.md`
- `docs/OPERATIONS.md`
- `docs/TROUBLESHOOTING.md`

---

## 10) Post-Deploy Monitoring (`REQUIRED`)

- [ ] `REQUIRED` Error rate within acceptable MVP threshold.
- [ ] `REQUIRED` P95/P99 latency within MVP threshold.
- [ ] `REQUIRED` Worker process stable (no crash loop).
- [ ] `REQUIRED` Disk/memory headroom healthy.
- [ ] `REQUIRED` No determinism violations observed.

Runbooks:

- `docs/runbooks/WORKER_CRASH.md`
- `docs/runbooks/INFERENCE_LATENCY_SPIKE.md`
- `docs/runbooks/MEMORY_PRESSURE.md`
- `docs/runbooks/DISK_FULL.md`
- `docs/runbooks/DETERMINISM_VIOLATION.md`

Quick commands:

```bash
ps aux | grep aos-worker
df -h var/
./aosctl doctor
```

---

## 11) Rollback Triggers and Procedure (`REQUIRED`)

Rollback triggers:

- [ ] `REQUIRED` Sustained elevated 5xx or failed readiness.
- [ ] `REQUIRED` Data correctness/determinism regression.
- [ ] `REQUIRED` Security regression with active risk.
- [ ] `REQUIRED` Migration-caused functional outage.

Rollback steps:

- [ ] `REQUIRED` Freeze traffic or route to stable pool.
- [ ] `REQUIRED` Roll back application to last known good artifact.
- [ ] `REQUIRED` If needed, execute migration rollback SQL in dependency order.
- [ ] `REQUIRED` Re-run health and smoke checks.
- [ ] `REQUIRED` Open incident timeline and capture evidence.

References:

- `migrations/rollbacks/README.md`
- `scripts/deploy-production.sh` and `scripts/verify-deployment.sh` (rollback validation path)

---

## 12) MVP Non-Goals (Explicit)

- [ ] Non-blocking advisory jobs are not launch gates unless promoted.
- [ ] Nice-to-have performance optimizations are deferred unless they affect SLOs.
- [ ] Broad refactors are deferred until after MVP stabilization.

---

## 13) Release Sign-off Sheet (`REQUIRED`)

Engineering Lead:
- Name:
- Date:
- Decision: `GO` / `NO-GO`

SRE/Operations:
- Name:
- Date:
- Decision: `GO` / `NO-GO`

Security:
- Name:
- Date:
- Decision: `GO` / `NO-GO`

Product/Owner:
- Name:
- Date:
- Decision: `GO` / `NO-GO`

Final:
- Release commit SHA:
- Deployment timestamp:
- Incident channel:
- Rollback owner:

---

## 14) One-Command Prep Bundle (Optional Convenience)

Run this before cut:

```bash
bash scripts/release/mvp_control_room.sh --skip-deploy
```

If this bundle fails, stop release and resolve before proceeding.

---

## 15) Phase 10 Evidence Reconciliation (`REQUIRED`)

This section tracks fresh OPS-06/07/08 sign-off evidence captured on 2026-02-24.

- [x] `REQUIRED` OPS-06 control-room rehearsal completed with no failed steps.
- [x] `REQUIRED` OPS-07 signed SBOM + provenance artifacts generated and linked.
- [x] `REQUIRED` OPS-08 final GO/NO-GO decision recorded by Release Commander.

Evidence (fresh run):

- Control-room evidence log: `var/release-control-room/20260224T103206Z/evidence.log`
- Control-room summary: `var/release-control-room/20260224T103206Z/summary.txt`
- Signed SBOM: `target/release-bundle/sbom.json`
- Signed provenance: `target/release-bundle/build_provenance.json`
- SBOM signature: `target/release-bundle/signature.sig`
- Provenance signature: `target/release-bundle/build_provenance.sig`
- Final decision package: `.planning/phases/10-operations-release-sign-off/10-GO-NO-GO-EVIDENCE.md`

Gate status:

- OPS-06: `PASS` (doctor + readiness + smoke gates passed in run `20260224T103206Z`)
- OPS-07: `PASS` (signed release-bundle artifacts present; provenance linkage verified)
- OPS-08: `PASS` (final decision recorded as `GO`)
