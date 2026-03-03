# DATASET_CURATION_WORKFLOW

Human-in-the-loop workflow for hand-authored gold data used for training, evals, and regression gates.

---

## Purpose

Establish a repeatable process to convert real operational signals into high-quality, evidence-bound gold examples.

Use this workflow when curating examples from:
- Incident timelines
- Existing runbooks
- Operator command logs
- Verified postmortems

Do not use this workflow for synthetic-only examples or unverified anecdotes.

---

## Roles and Ownership

- `Curator (owner)`: drafts/edits candidate examples and binds evidence.
- `Reviewer (peer)`: validates technical correctness and policy fit.
- `Approver (maintainer/on-call lead)`: final accept/reject decision for the weekly batch.

Minimum approval rule:
- 1 curator + 1 reviewer + 1 approver sign-off before publishing.

---

## Source Intake Pipeline

### 1. Intake sources (daily capture, weekly batch)

Create candidate rows from:
- Closed incidents with confirmed root cause
- Runbooks that were exercised during real events
- Command/log traces from production-like environments
- Postmortem action items with verified fixes

Record each candidate with:
- `candidate_id`
- `source_type` (`incident`, `runbook`, `logs`, `postmortem`)
- `source_ref` (ticket/runbook path/log bundle id)
- `date_observed`
- `owner`
- `risk_level` (`low|medium|high`)

### 2. Triage and de-duplication

Before authoring, check if a near-match already exists in the gold set.

Reject as duplicate when:
- Same failure mode, same command intent, and same resolution path already represented.

Merge/extend instead of duplicate when:
- A candidate adds only minor phrasing differences.
- A candidate shares root cause but adds stronger evidence.

### 3. Prioritization

Prioritize candidates in this order:
1. Determinism/safety regressions
2. High-frequency operational failures
3. High-blast-radius runbook flows
4. Recurring operator confusion patterns

---

## Evidence Binding Rules (Required)

Every accepted example must be auditable to real source artifacts.

### Required evidence fields per example

- `evidence.source_ref`: stable pointer (ticket id, runbook path, log archive id)
- `evidence.snapshot_time`: timestamp when evidence was captured
- `evidence.extract`: exact snippet or command/output excerpt used for labeling
- `evidence.verifier`: reviewer identity
- `evidence.verification_note`: why the example is faithful to source behavior

### Binding constraints

- No orphan examples: examples without `evidence.source_ref` are auto-rejected.
- No unverifiable paraphrase: any transformed wording must preserve original operator intent and system behavior.
- No cross-incident stitching unless explicitly labeled `composite=true` and each component source is cited.
- Redactions must preserve semantics; if redaction removes critical context, reject.
- If logs conflict with runbook guidance, prefer observed behavior and mark runbook drift.

### Provenance quality bar

- `high`: incident + logs + runbook alignment
- `medium`: incident + one corroborating source
- `low`: single source only (accept only with approver override and TODO to strengthen evidence)

---

## Authoring Standard for Gold Examples

Each example should include:
- `input_context`: realistic operator/system state
- `expected_action`: precise command or decision path
- `expected_output`: concrete expected response/result
- `failure_mode`: what goes wrong if mishandled
- `evidence`: fields from Evidence Binding Rules

Authoring rules:
- Keep language operational, not theoretical.
- Preserve real command and error semantics.
- Prefer minimal but complete context required to choose the correct action.
- Mark ambiguity explicitly instead of guessing.

---

## Acceptance Checklist (Must Pass)

- [ ] Source is real and traceable to incident/runbook/log/postmortem artifacts.
- [ ] Evidence fields are complete and reviewer-verified.
- [ ] Example is non-duplicative (or clearly extends an existing pattern).
- [ ] Expected action is deterministic and testable.
- [ ] Sensitive data is redacted without losing decision-critical meaning.
- [ ] Labeling aligns with current runbook/policy behavior (or is explicitly marked as drift).
- [ ] At least one peer reviewer confirmed technical correctness.
- [ ] Approver confirmed batch-level quality and coverage.

## Rejection Checklist (Any One Fails)

- [ ] Missing or unstable evidence reference.
- [ ] Contradicts observed system behavior without documented rationale.
- [ ] Duplicate of existing gold data with no incremental value.
- [ ] Ambiguous expected action/output.
- [ ] Over-redacted to the point of unusable supervision.
- [ ] Contains speculative or synthetic details presented as fact.
- [ ] Cannot be explained to another operator from evidence alone.

---

## Weekly Update Cadence

Run this cadence once per week (default: Monday intake freeze, Friday publish).

### Monday: Intake Freeze

- Freeze new candidate intake for the week.
- Export candidate list and assign curator/reviewer pairs.

### Tuesday-Wednesday: Curation and Review

- Curators draft/update candidates.
- Reviewers validate evidence binding and correctness.
- Resolve duplicates and merge near-matches.

### Thursday: Approval Gate

- Approver reviews the batch against acceptance/rejection checklists.
- Tag outcomes: `accepted`, `rework`, `rejected`.

### Friday: Publish + Changelog

- Publish accepted examples into the gold dataset version.
- Record weekly changelog with:
  - Counts (`new`, `updated`, `rejected`)
  - Top covered failure modes
  - Drift findings (runbook vs observed behavior)
  - Open TODOs for weak-provenance examples

### Weekly Exit Criteria

- All accepted examples have complete evidence binding.
- Rejected/rework reasons are recorded.
- Coverage summary is produced for the week.
- Next-week backlog is prioritized.

---

## Metrics to Track

- Acceptance rate per week
- Rejection reasons distribution
- Duplicate rate
- Median curation cycle time (intake -> publish)
- `% high provenance` examples
- Drift findings count (docs vs observed behavior)

If any metric degrades for 2 consecutive weeks, trigger a workflow retro.

---

## Guardrails and Exceptions

- If evidence is incomplete but operationally critical, label `temporary_exception=true`, require approver override, and add a next-week remediation TODO.
- If behavior is uncertain, do not force acceptance; park in `rework` with explicit question and required source.
- Never backfill certainty from memory when source artifacts are missing.

