# Phase 6: Production Operations - Research

**Researched:** 2026-02-24
**Domain:** Production security gates, backup/restore operations, release provenance, deployment readiness
**Confidence:** HIGH

## Summary

The repository already contains the core operational machinery Phase 6 needs: a dedicated security regression workflow, backup/restore scripts with CI smoke coverage, an SBOM/provenance release bundler, an MVP launch checklist, and a control-room runner that records deployment evidence. The gap for this phase is not missing primitives; it is disciplined execution and evidence closure across `OPS-01` through `OPS-05`.

`MVP_PROD_CHECKLIST.md` explicitly encodes Phase 6 gates, including security regression tests, backup/restore commands, SBOM/provenance generation, and staging deploy validation. The roadmap success criteria align directly with existing scripts and workflows, which means Phase 6 should prioritize validation reliability, artifact traceability, and minimal drift fixes instead of adding parallel operational paths.

**Primary recommendation:** Execute a single production-operations validation track that reuses current workflows/scripts, produce auditable evidence for each OPS requirement, and only patch blockers that prevent checklist closure.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Treat `.github/workflows/security-regression-tests.yml` as the authoritative security regression gate.
- Require local command evidence parity with CI behavior for security checks.
- Use existing backup pipeline scripts only (`backup.sh`, `verify-backups.sh`, `test-restore.sh`, `ci-smoke.sh`).
- Use `scripts/release/sbom.sh` as canonical SBOM/provenance generation path.
- Treat `MVP_PROD_CHECKLIST.md` as the final launch gate for REQUIRED controls.
- Use `scripts/release/mvp_control_room.sh` and `scripts/verify-deployment.sh` for runbook/deploy validation.

### Claude's Discretion
- Validation sequencing for speed vs confidence tradeoff.
- Minimal drift fixes vs explicit defer-with-rationale when outside phase scope.
- Evidence packaging format, as long as OPS traceability is explicit.

### Deferred Ideas (OUT OF SCOPE)
- Multi-node disaster-recovery automation.
- New product/security feature development beyond validation hardening.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| OPS-01 | Security regression suite fully green | `.github/workflows/security-regression-tests.yml` runs `security_regression_suite`, crypto tests, and security-focused clippy gates; `tests/security_regression_suite.rs` and `tests/security_regression_tests.rs` provide test surfaces. |
| OPS-02 | Backup and restore pipeline tested and documented | `scripts/backup/backup.sh`, `verify-backups.sh`, `test-restore.sh`, and `ci-smoke.sh` provide executable pipeline; `MVP_PROD_CHECKLIST.md` section 6 defines required evidence. |
| OPS-03 | Release build with SBOM and provenance artifacts generated | `scripts/release/sbom.sh` stages artifacts and emits `sbom.json` + `build_provenance.json` (optional signatures); roadmap and checklist both call this out directly. |
| OPS-04 | All MVP production checklist gates satisfied | `MVP_PROD_CHECKLIST.md` enumerates REQUIRED controls and command proofs across CI/security/migrations/backup/release/deploy. |
| OPS-05 | Deployment runbook validated | `MVP_PROD_CONTROL_ROOM.md`, `scripts/release/mvp_control_room.sh`, `scripts/verify-deployment.sh`, and `docs/runbooks/README.md` establish runbook execution and incident references. |
</phase_requirements>

## Standard Stack

### Operations Surfaces
| Surface | Source | Purpose |
|---------|--------|---------|
| Security regression CI | `.github/workflows/security-regression-tests.yml` | Security test/lint/audit gate in CI |
| Security suite tests | `tests/security_regression_suite.rs`, `tests/security_regression_tests.rs` | Regression assertions for crypto, unsafe usage, access controls |
| Backup pipeline | `scripts/backup/*.sh` | Backup creation, integrity verification, restore validation |
| Release provenance | `scripts/release/sbom.sh` | Deterministic SBOM + provenance bundle generation |
| Launch gate | `MVP_PROD_CHECKLIST.md` | Required pass/fail release controls |
| Control room | `scripts/release/mvp_control_room.sh` | Ordered runbook execution with timestamped evidence logs |

## Architecture Patterns

### Pattern: Checklist-Driven Operations Closure
**What:** Treat checklist REQUIRED items as contract tests for production readiness.
**Why it works:** Prevents subjective "looks good" approvals; each gate has command-level evidence.
**Where grounded:** `MVP_PROD_CHECKLIST.md`, `MVP_PROD_CONTROL_ROOM.md`.

### Pattern: Script-First Operational Validation
**What:** Use committed scripts as canonical procedures instead of ad-hoc shell sequences.
**Why it works:** Reduces operator variance and supports repeatable CI/staging/prod rehearsal.
**Where grounded:** `scripts/backup/ci-smoke.sh`, `scripts/release/mvp_control_room.sh`, `scripts/release/sbom.sh`.

### Pattern: Evidence-by-Artifact
**What:** Keep machine-generated logs/artifacts as acceptance evidence.
**Why it works:** Gives auditable traceability for OPS requirements and rollback decisions.
**Where grounded:** `scripts/release/mvp_control_room.sh` (`var/release-control-room/<ts>/evidence.log`), SBOM/provenance outputs.

## Anti-Patterns to Avoid

- Running bespoke operational commands outside maintained scripts and treating that as equivalent evidence.
- Declaring backup success without restore validation on a clean target path.
- Generating SBOM/provenance artifacts without release binaries staged (script will skip missing artifacts).
- Marking checklist REQUIRED items complete without attached command output or artifact path.
- Treating non-blocking advisories as blocking failures without explicit policy/owner decision.

## Common Pitfalls

### Pitfall 1: Security Gate Drift Between Local and CI
**What goes wrong:** Local validations differ from workflow behavior, producing false confidence.
**How to avoid:** Mirror workflow command set when validating locally before checklist closure.

### Pitfall 2: Backup Validation Without Real Restore
**What goes wrong:** Encrypted backup exists, but restore path or permissions break at recovery time.
**How to avoid:** Always run `scripts/backup/test-restore.sh` after backup + verify.

### Pitfall 3: Provenance Artifacts Missing Due to Absent Release Binaries
**What goes wrong:** `scripts/release/sbom.sh` exits with no staged artifacts.
**How to avoid:** Run `cargo build --release` first and ensure expected binaries exist.

### Pitfall 4: Checklist Closure Without Evidence Hygiene
**What goes wrong:** Boxes checked, but no linked logs/artifacts for audit or rollback review.
**How to avoid:** Record exact command, timestamp, and artifact location for each REQUIRED item.

## Code Examples

### Security Gate Mirror
```bash
cargo test --test security_regression_suite -- --nocapture --test-threads=1
cargo test -p adapteros-crypto --test crypto_operations_tests -- --nocapture
cargo clippy --workspace -- -D unsafe_code -W clippy::all
```

### Backup -> Verify -> Restore
```bash
bash scripts/backup/backup.sh
bash scripts/backup/verify-backups.sh
bash scripts/backup/test-restore.sh
```

### Release SBOM + Provenance
```bash
cargo build --release
bash scripts/release/sbom.sh
```

### Control Room Rehearsal
```bash
bash scripts/release/mvp_control_room.sh --skip-deploy --base-url http://localhost:8080
```

## State of the Art

| Old Approach | Current Approach | Impact |
|--------------|------------------|--------|
| Ad-hoc release ops commands | Checklist + control-room scripted flow | Higher repeatability and evidence quality |
| Backup success inferred from archive presence | Explicit verify + restore scripts | Better recovery confidence |
| Artifact build without provenance linkage | `sbom.sh` build metadata + artifact hashing | Stronger release traceability |

## Current Operations Readiness (Repository Evidence)

### Present and Usable
- Security regression workflow exists and is scoped to security-critical paths.
- Backup, verify, restore, and CI smoke scripts exist under `scripts/backup/`.
- Release SBOM/provenance generator exists under `scripts/release/sbom.sh`.
- MVP checklist and control-room documents/scripts exist and align with Phase 6 criteria.
- Incident runbook index exists at `docs/runbooks/README.md`.

### Not Yet Proven in This Research Step
- End-to-end green execution status for all Phase 6 commands on current HEAD.
- Staging-equivalent deployment rehearsal success without manual intervention failures.
- Full checklist completion with evidence attached.

---
*Phase: 06-production-operations*
*Research completed: 2026-02-24*
