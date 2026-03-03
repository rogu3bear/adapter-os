# Phase 12: Governance Debt Retirement - Research

**Researched:** 2026-02-24
**Domain:** Governance debt retirement, capability-gated branch protection operations, preflight automation, and planning traceability
**Confidence:** HIGH (repo-local), MEDIUM (external platform capability)

## Summary

Phase 11 closed `FFI-05` for milestone accounting as accepted external debt, but governance debt retirement is still operationally incomplete: the repo lacks a single reusable retirement playbook, lacks dedicated governance preflight automation, and still relies on manual consistency across requirements/audit artifacts.

Repository grounding confirms the necessary primitives already exist:
- canonical governance docs (`docs/governance/README.md`),
- release checklist surface (`MVP_PROD_CHECKLIST.md`),
- Phase 11 evidence and reconciliation artifacts,
- planning integrity tooling via runtime `gsd-tools`.

The smallest native approach is a three-plan sequence:
1. codify capability-retirement playbook,
2. automate governance preflight checks in `scripts/ci/`,
3. harden requirement/audit traceability around new governance requirements.

**Primary recommendation:** execute `12-01` -> `12-02` -> `12-03` sequentially to keep automation and traceability aligned to one explicit governance retirement model.

<user_constraints>
## User Constraints (from phase context and planning state)

### Locked Decisions
- Keep scope on governance debt retirement only.
- Reuse existing governance and planning surfaces; avoid parallel docs/contracts.
- Keep plans concrete, minimal, and executable.
- Preserve capability-gated truth model from Phase 11 (no false claims of enforced merge-gate proof while API capability remains blocked).

### Claude's Discretion
- Exact preflight script interface and exit-code behavior.
- Minimal document/checklist wording necessary to make retirement gating explicit.
- Exact verification command set, as long as it is repo-realistic and reproducible.

### Deferred Ideas (OUT OF SCOPE)
- Platform billing/visibility changes required to unlock branch-protection API capability.
- Broader CI or release process redesign outside governance debt retirement.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Requirement | Evidence Anchor | Plan Coverage |
|----|-------------|-----------------|---------------|
| GOV-06 | Branch-protection capability retirement playbook is defined with explicit read/write/read gate logic and blocker handling | `docs/governance/README.md`, `MVP_PROD_CHECKLIST.md`, Phase 12-01 summary evidence | `12-01` |
| GOV-07 | Governance preflight automation exists and deterministically reports capability + required-check status for target repo/branch/context | `scripts/ci/check_governance_preflight.sh`, CI invocation proof, Phase 12-02 summary | `12-02` |
| GOV-08 | Governance requirements and milestone/audit traceability remain internally consistent after Phase 12 updates | `.planning/REQUIREMENTS.md`, `.planning/ROADMAP.md`, `.planning/STATE.md`, `.planning/milestones/v1.1-MILESTONE-AUDIT.md`, Phase 12-03 summary | `12-03` |
</phase_requirements>

<plan_dependency_model>
## Plan Dependency Model

| Plan | Wave | Depends On | Requirement Focus | Why This Shape |
|------|------|------------|-------------------|----------------|
| 12-01 | 1 | none | GOV-06 | Playbook semantics must exist before automating checks against them. |
| 12-02 | 2 | 12-01 | GOV-07 | Preflight automation should encode the finalized playbook gates. |
| 12-03 | 3 | 12-02 | GOV-08 | Traceability hardening should reflect completed playbook + automation outputs. |
</plan_dependency_model>

## Standard Stack

| Surface | Canonical Path | Role in Phase 12 |
|---------|----------------|------------------|
| Governance policy notes | `docs/governance/README.md` | Capability-gated retirement playbook narrative |
| Release governance checklist | `MVP_PROD_CHECKLIST.md` | Merge-gate retirement checkpoints and evidence links |
| CI governance checks | `scripts/ci/` | Native home for governance preflight automation |
| Requirement register | `.planning/REQUIREMENTS.md` | Requirement status source of truth |
| Milestone/roadmap/status | `.planning/ROADMAP.md`, `.planning/STATE.md`, `.planning/milestones/v1.1-MILESTONE-AUDIT.md` | Debt retirement traceability and audit consistency |
| Prior governance evidence | `.planning/phases/11-ffi-governance-enforcement-closure/11-01-SUMMARY.md`, `11-02-SUMMARY.md`, `11-03-SUMMARY.md` | Baseline capability truth and accepted-debt state |

## Architecture Patterns

### Pattern: Capability-gated governance operations
- Separate "policy intent" from "platform capability."
- If required-check branch-protection APIs are blocked, preserve debt truth explicitly instead of emitting false enforcement claims.

### Pattern: Single preflight automation entrypoint
- Follow existing `scripts/ci/check_*.sh` conventions.
- Produce deterministic output/exit behavior suitable for both local operator runs and CI gates.

### Pattern: Traceability-first planning updates
- Requirement, roadmap, state, and milestone audit must stay synchronized for governance debt status.
- Any update to one traceability surface must be reflected across the others in the same reconciliation plan.

## Do Not Hand-Roll

| Problem | Do Not Build | Use Instead | Why |
|---------|--------------|-------------|-----|
| Governance retirement flow | Ad-hoc one-off command notes | Single documented playbook in governance/checklist surfaces | Prevents future operator drift |
| Preflight checks | Manual shell sequence copied in comments | Dedicated script under `scripts/ci/` | Reusable, CI-friendly, deterministic outcomes |
| Debt traceability | Separate side ledger | Existing `.planning` requirement/roadmap/state/audit artifacts | Preserves GSD planning contract |

## Common Pitfalls

### Pitfall 1: Treating `HTTP 403` as "unknown" instead of explicit blocker
**What goes wrong:** debt status becomes ambiguous and retries are non-actionable.
**How to avoid:** classify `403` as a first-class blocked state with owner/prerequisite text.

### Pitfall 2: Automation writes policy in preflight mode
**What goes wrong:** preflight mutates branch protection unexpectedly and blurs evidence provenance.
**How to avoid:** keep preflight read-only; reserve writes for explicit enforcement/retirement runs.

### Pitfall 3: Traceability surfaces diverge after governance updates
**What goes wrong:** requirement table, roadmap status, and milestone audit disagree.
**How to avoid:** reconcile all traceability artifacts together and run targeted `gsd-tools` integrity checks.

## Code Examples

### Baseline capability probe
```bash
REPO="$(gh repo view --json nameWithOwner --jq '.nameWithOwner')"
BRANCH="$(gh repo view --json defaultBranchRef --jq '.defaultBranchRef.name')"
gh api "repos/${REPO}/branches/${BRANCH}/protection/required_status_checks"
```

### Governance preflight invocation shape
```bash
bash scripts/ci/check_governance_preflight.sh \
  --repo "rogu3bear/adapter-os" \
  --branch "main" \
  --required-context "FFI AddressSanitizer (push)"
```

### Traceability consistency checks
```bash
rg -n "GOV-06|GOV-07|GOV-08|governance debt|tech_debt" \
  .planning/REQUIREMENTS.md \
  .planning/ROADMAP.md \
  .planning/STATE.md \
  .planning/milestones/v1.1-MILESTONE-AUDIT.md -S
```

### Targeted planning integrity checks
```bash
node /Users/star/.codex/skills/gsd-codex-runtime/runtime/get-shit-done/bin/gsd-tools.cjs \
  verify artifacts .planning/phases/12-governance-debt-retirement/12-03-PLAN.md
node /Users/star/.codex/skills/gsd-codex-runtime/runtime/get-shit-done/bin/gsd-tools.cjs \
  verify key-links .planning/phases/12-governance-debt-retirement/12-03-PLAN.md
```

## Current State (Verified)

- `FFI-05` is already reconciled as verified with accepted external debt in current milestone accounting.
- Governance docs already encode required context intent (`FFI AddressSanitizer (push)`), but no explicit retirement playbook exists yet.
- Phase 12 is present in roadmap as planned and now decomposed into `12-01`/`12-02`/`12-03` planning artifacts.
- Runtime `gsd-tools` path used by prior phase artifacts is available for targeted artifact verification.

## Sources

### Primary (repository-grounded)
- `.planning/phases/11-ffi-governance-enforcement-closure/11-CONTEXT.md`
- `.planning/phases/11-ffi-governance-enforcement-closure/11-RESEARCH.md`
- `.planning/phases/11-ffi-governance-enforcement-closure/11-01-SUMMARY.md`
- `.planning/phases/11-ffi-governance-enforcement-closure/11-02-SUMMARY.md`
- `.planning/phases/11-ffi-governance-enforcement-closure/11-03-SUMMARY.md`
- `docs/governance/README.md`
- `MVP_PROD_CHECKLIST.md`
- `.planning/ROADMAP.md`
- `.planning/REQUIREMENTS.md`
- `.planning/STATE.md`
- `.planning/milestones/v1.1-MILESTONE-AUDIT.md`

---
*Phase: 12-governance-debt-retirement*
*Research completed: 2026-02-24*
*Ready for planning: yes*
