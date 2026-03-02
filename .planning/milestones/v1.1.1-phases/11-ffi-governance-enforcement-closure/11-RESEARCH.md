# Phase 11: FFI Governance Enforcement Closure - Research

**Researched:** 2026-02-24
**Domain:** Merge-gate governance and branch-protection required-check enforcement
**Confidence:** HIGH (repo-local scope), MEDIUM (external platform capability)
**Status:** Executed and reconciled (historical planning research)

## Post-Execution Reconciliation (2026-02-24)

This document reflects planning-time research assumptions and gap framing before full phase reconciliation.

Final state is recorded in:
- `.planning/phases/11-ffi-governance-enforcement-closure/11-03-SUMMARY.md`
- `.planning/phases/11-ffi-governance-enforcement-closure/11-VERIFICATION.md`
- `.planning/milestones/v1.1-MILESTONE-AUDIT.md`

Current closure accounting:
- `FFI-05` is verified with accepted external blocker debt (`HTTP 403` branch-protection API capability limit in this environment).
- No repo-actionable closure gap remains in milestone status.

## Summary

At planning time, the remaining repository-controlled rectification target was governance enforcement proof for `FFI-05`.

The planning-time gap was not CI definition. The `ffi-asan` lane existed in workflow config. The unresolved part was verifiable required-check enforcement on the protected branch, which previously failed under API access constraints (HTTP 403). This phase therefore centered on:
- proving capability,
- applying enforcement when capability exists,
- documenting prerequisites when capability is blocked,
- and updating audit/requirements traceability accordingly.

**Primary recommendation:** execute a strict 3-plan sequence: capability baseline, enforcement application, then artifact reconciliation and re-audit.

<user_constraints>
## User Constraints (from phase context and current planning state)

### Locked Decisions
- Close `FFI-05` with enforceable required-check evidence, not inferred compliance.
- Preserve existing planning/artifact structure under `.planning/`.
- Keep diffs minimal and focused on governance closure.

### Claude's Discretion
- Command-level sequencing and evidence format.
- Exact fallback wording when platform plan constraints block API operations.

### Deferred Ideas (OUT OF SCOPE)
- CI architecture expansion unrelated to `ffi-asan` required-check enforcement.
- Platform billing/plan changes themselves (can be listed as prerequisite only).
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Requirement | Evidence Anchor | Plan Coverage |
|----|-------------|-----------------|---------------|
| FFI-05 | `ffi-asan` merge-gate is enforceably required on protected branch with auditable proof | branch-protection API transcript + governance docs/checklist references + refreshed milestone audit | `11-01`, `11-02`, `11-03` |
</phase_requirements>

<plan_dependency_model>
## Plan Dependency Model

| Plan | Wave | Depends On | Requirement Focus | Why This Shape |
|------|------|------------|-------------------|----------------|
| 11-01 | 1 | none | FFI-05 capability baseline | Avoid false closure by proving platform/API capability first. |
| 11-02 | 2 | 11-01 | FFI-05 enforcement application | Apply and verify required-check enforcement only in a validated capable context. |
| 11-03 | 3 | 11-02 | FFI-05 closure reconciliation | Update requirements/audit/state once enforcement evidence is complete. |
</plan_dependency_model>

## Standard Stack

| Surface | Canonical Path | Role in Phase 11 |
|---------|----------------|------------------|
| Governance workflow definition | `.github/workflows/ci.yml` | Source for the `ffi-asan` check name and trigger context |
| Governance documentation | `docs/governance/README.md` | Policy narrative and merge-gate expectations |
| Release governance checklist | `MVP_PROD_CHECKLIST.md` | Traceable closure anchor |
| Phase 09 evidence | `.planning/phases/09-determinism-and-compatibility-revalidation/09-03-SUMMARY.md` | Baseline blocker evidence and prior failed API attempt |
| CLI/API evidence | `gh api ...` transcript captured in phase summary artifacts | Machine-verifiable enforcement proof |

## Architecture Patterns

### Pattern: Capability-gated governance
- Do not write enforcement claims until read/write capability checks pass.
- Treat plan/permission blockers as explicit prerequisites with owner and next action.

### Pattern: Evidence-first closure
- Prefer raw command transcript evidence over narrative-only claims.
- Ensure final status in `REQUIREMENTS.md` and milestone audit matches the latest evidence truth.

## Do Not Hand-Roll

| Problem | Do Not Build | Use Instead | Why |
|---------|--------------|-------------|-----|
| Governance proof | Informal statement that CI lane exists | branch-protection required-check API transcript | Required checks are branch policy, not workflow existence |
| Closure bookkeeping | Ad-hoc note files | existing `.planning` phase + milestone artifacts | Preserves GSD traceability contract |

## Common Pitfalls

### Pitfall 1: Using wrong required-check context string
**What goes wrong:** Policy is applied with a non-matching check name/context and does not enforce the intended lane.
**How to avoid:** Derive expected check names from actual workflow runs/check suites before setting enforcement.

### Pitfall 2: Partial API access interpreted as closure
**What goes wrong:** Read succeeds but write fails; closure is incorrectly marked complete.
**How to avoid:** Require both read and write verification before changing `FFI-05` to verified.

### Pitfall 3: Artifact drift after governance change
**What goes wrong:** Requirements/audit docs remain stale despite real enforcement.
**How to avoid:** Always run reconciliation plan (`11-03`) with explicit artifact updates.

## Code Examples

### Capability baseline
```bash
gh api repos/<owner>/<repo>/branches/<branch>/protection/required_status_checks
```

### Enforcement write (when capability is available)
```bash
gh api \
  --method PATCH \
  repos/<owner>/<repo>/branches/<branch>/protection/required_status_checks \
  -f strict=true \
  -F contexts[]='FFI AddressSanitizer (push)'
```

### Verification re-read
```bash
gh api repos/<owner>/<repo>/branches/<branch>/protection/required_status_checks
```

## Current State (Verified)

- `FFI-05` is reconciled as verified with accepted external blocker debt in milestone audit accounting.
- Governance attempts documented HTTP 403 blocker against required-check branch-protection API for this environment.
- Repo-local CI/config and runtime closure work for v1.1 residuals is complete with no repo-actionable blocker remaining.

## Sources

### Primary (repository-grounded)
- `.planning/milestones/v1.1-MILESTONE-AUDIT.md`
- `.planning/phases/09-determinism-and-compatibility-revalidation/09-03-SUMMARY.md`
- `.planning/REQUIREMENTS.md`
- `.planning/ROADMAP.md`
- `.github/workflows/ci.yml`
- `docs/governance/README.md`
- `MVP_PROD_CHECKLIST.md`

---
*Phase: 11-ffi-governance-enforcement-closure*
*Research completed: 2026-02-24*
*Ready for planning: no (already executed and reconciled)*
