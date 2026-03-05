# Roadmap: adapterOS

## Milestone v1.1.18: System Stabilization (In Progress)

Fix runtime blockers preventing full-stack operation: training worker spawn PATH resolution, stale runtime state cleanup (SecD socket, degraded markers), and commit the 84-file accumulated diff to establish a clean baseline.

## Previous Milestone v1.1.17: Production Cut Closure (Complete)

Executed the canonical prod-cut contract in `.planning/PROD_CUT.md`: route contract closure, startup/determinism/security hardening, no-skip prod gating, runbook evidence readiness, and release artifact/signing enforcement.

## Previous Milestone v1.1.16: Training Pipeline Execution Hardening

Closed the training execution gap by enforcing worker/preflight readiness before enqueue, unifying primary-model resolution, and making dataset/version compatibility failures explicit and deterministic.

## Milestones

- ✅ **v1.0 milestone** - Foundation stabilization shipped (2026-02-24)
- ✅ **v1.1 Stability and Release Sign-off** - Ops/release closure shipped (2026-02-24)
- ✅ **v1.1.1 Post-v1.1 Hardening Closure** - Governance/determinism/runtime hardening shipped (2026-02-25)
- ✅ **v1.1.2 Governance Retirement Enforcement** - Capability-gated retirement flow executed with accepted external blocker branch (2026-02-25)
- ✅ **v1.1.3 Governance Drift Guardrails** - Drift/parity guardrails shipped with explicit external-blocker tech-debt acceptance (2026-02-25)
- ✅ **v1.1.4 Governance Capability Unlock and Enforcement Closure** - Phases 18-19 executed with explicit external capability debt posture (2026-02-26)
- ✅ **v1.1.5 Governance Capability Activation and Debt Retirement** - Phases 20-21 executed; canonical capable-proof debt `GOV-16` accepted as external (2026-02-26)
- ✅ **v1.1.6 QA Visual Working System Activation (macOS Dual-Browser Blocking)** - Archived in milestone history.
- ✅ **v1.1.7 Adapter Git-Like Version Control and Dataset Feed Workflows** - Archived in milestone history.
- ✅ **v1.1.8 Assistive AdapterOps Foundation and Guided Operator Flow** - Archived in milestone history.
- ✅ **v1.1.9 Adapter Git Command Surface and Feed Automation** - Archived in milestone history.
- ✅ **v1.1.10 Command Deck Validation and Assistive Refinement** - Archived in milestone history.
- ✅ **v1.1.11 Operator Command Assistive Workflow Extension** - Archived in milestone history.
- ✅ **v1.1.12 Operator Command Assistive Continuity Finalization** - Archived in milestone history.
- ✅ **v1.1.13 Operator Command Guidance Stability Pass** - Archived in milestone history.
- ✅ **v1.1.14 AdapterOps Command Language and Assistive Continuity** - Completed (phases 40-42).
- ✅ **v1.1.15 AdapterOps Timeline, Command Deck Parity, and Dataset Version Pinning** - Completed (phases 43-45).
- ✅ **v1.1.16 Training Pipeline Execution Hardening** - Completed (phase 46, 2026-02-28).
- ✅ **v1.1.17 Production Cut Closure** - Completed (phase 47, 2026-03-04); canonical scope/gates in `.planning/PROD_CUT.md`, receipts in `.planning/prod-cut/evidence/`.
- [ ] **v1.1.18 System Stabilization** - Active; fix training worker spawn, clean stale runtime state, commit dirty tree.

## Phases

- [x] **Phase 40: Command Language and Checkout-First Continuity** - Harmonized command vocabulary and default-path guidance across dashboard/update/detail surfaces. (completed 2026-02-28, v1.1.14)

- [x] **Phase 41: Dataset Feed Provenance Handoff** - Preserved repo/branch/source-version continuity into training-entry launches. (completed 2026-02-28, v1.1.14)

- [x] **Phase 42: Assistive Guidance Parity and Validation** - Closed assistive labeling parity with citation-grounded verification/UAT artifacts. (completed 2026-02-28, v1.1.14)
- [x] **Phase 43: Repository Command Timeline** - Add operator-visible repository command timeline in adapter detail and refresh it after command operations. (completed 2026-02-28)
- [x] **Phase 44: Command Deck AdapterOps Parity** - Add command palette parity for promote/checkout/feed-dataset with selected-adapter deep-link continuity. (completed 2026-02-28)
- [x] **Phase 45: Dataset Version-Pinned Training Contract** - Move wizard submit to typed training request and carry dataset version provenance. (completed 2026-02-28)
- [x] **Phase 46: Training Pipeline Execution Hardening** - Fail closed when training worker is unavailable, enforce algorithm/version preflight before enqueue, and pin 27B model resolution for training path consistency. (completed 2026-02-28)
- [x] **Phase 47: Production Cut Contract Closure** - Execute `.planning/PROD_CUT.md` gate set with strict prod-mode policies and evidence capture.
- [ ] **Phase 48: Commit Dirty Tree** - Commit the 84-file accumulated diff in logical atomic commits to establish a clean baseline. (v1.1.18)
- [ ] **Phase 49: Training Worker Spawn Fix** - Fix binary PATH resolution so training worker spawns successfully on backend boot. (v1.1.18)
- [ ] **Phase 50: Runtime State Hygiene** - Clean stale sockets, degraded markers, and restart counters on boot. (v1.1.18)

## Phase Details

### Phase 40: Command Language and Checkout-First Continuity

**Goal**: Align command vocabulary and default-path language across dashboard, update center, and adapter detail.
**Depends on**: Phase 39 (completed)
**Requirements**: UX-40-01, NL-40-01
**Success Criteria**:

1. Checkout/promote/feed-dataset language is consistent across surfaces.
2. Recommended-path copy is concise and unambiguous.
3. Restore-first phrasing is not reintroduced.
   **Plans**: 1/1 complete

### Phase 41: Dataset Feed Provenance Handoff

**Goal**: Preserve repo/branch/source-version continuity when launching feed-dataset into training.
**Depends on**: Phase 40
**Requirements**: VC-40-01, VC-40-02
**Success Criteria**:

1. Feed launches preserve repo, branch, and source-version context.
2. Selected-version feed messaging is explicit for operators.
3. Command vocabulary continuity remains intact.
   **Plans**: 1/1 complete

### Phase 42: Assistive Guidance Parity and Validation

**Goal**: Maintain assistive parity and citation-grounded verification for command surfaces.
**Depends on**: Phase 41
**Requirements**: A11Y-40-01, A11Y-40-02, DOC-40-01, DOC-40-02
**Success Criteria**:

1. Equivalent command actions expose consistent accessible names.
2. Recommended-action guidance remains assistive-friendly.
3. Verification/UAT artifacts include code and best-practice citations.
   **Plans**: 1/1 complete

### Phase 43: Repository Command Timeline

**Goal**: Show latest-first command timeline in adapter detail Update Center so operator decisions are history-aware.
**Depends on**: Phase 42 (completed)
**Requirements**: UX-41-01
**Success Criteria**:

1. Timeline appears in adapter detail Update Center.
2. Promote/checkout refresh the timeline immediately.
3. Timeline wording remains plain and command-first.
   **Plans**: 1/1 complete

### Phase 44: Command Deck AdapterOps Parity

**Goal**: Make command deck surface and execute adapter operations with context-preserving deep links.
**Depends on**: Phase 43
**Requirements**: UX-41-02, A11Y-41-01
**Success Criteria**:

1. `Run Promote`/`Run Checkout`/`Feed Dataset` actions appear in adapter contexts.
2. Commands preserve selected adapter context into destination surfaces.
3. Update Center announces command intent from deep-linked commands.
   **Plans**: 1/1 complete

### Phase 45: Dataset Version-Pinned Training Contract

**Goal**: Submit training with typed request and explicit dataset version when available.
**Depends on**: Phase 44
**Requirements**: VC-41-01, DOC-41-01
**Success Criteria**:

1. Wizard uses `create_training_job` with typed contract.
2. `dataset_version_id` is carried into submission when available.
3. Wizard status/review shows dataset version context.
   **Plans**: 1/1 complete

### Phase 46: Training Pipeline Execution Hardening

**Goal**: Make adapter training operationally reliable by closing silent-fast-fail paths and enforcing deterministic preflight gates before job creation.
**Depends on**: Phase 45
**Requirements**: TRN-46-01, DET-46-01, OPS-46-01, DOC-46-01
**Success Criteria**:

1. Training start rejects requests when no training worker is available with explicit error contract.
2. Dataset algorithm compatibility (HKDF/parser/path normalization) is validated before enqueue.
3. Active primary model resolution is consistent between model status and training execution path.
4. Job terminal failures persist actionable error reason in API-visible job status/log payloads.
   **Plans**: 1/1 complete

### Phase 47: Production Cut Contract Closure

**Goal**: Execute the frozen prod-cut contract in `.planning/PROD_CUT.md` and converge on strict release gating without policy ambiguity.
**Depends on**: Phase 46
**Requirements**: REL-47-01, API-47-01, SEC-47-01, OPS-47-01
**Success Criteria**:

1. Route closure artifacts are generated and route/openapi checks pass under strict prod-mode policy.
2. Startup/determinism/security assertions are blocking in required checks.
3. `local_release_gate_prod.sh` enforces governance blocking, full smoke, strict inference, and runbook evidence requirements.
4. Release artifact generation enforces provenance/signing requirements with verification log output.
   **Plans**: 1/1 complete

### Phase 48: Commit Dirty Tree

**Goal**: Commit the 84-file accumulated diff in logical atomic commits to establish a clean baseline.
**Depends on**: Phase 47 (completed)
**Requirements**: GIT-01, GIT-02
**Success Criteria**:

1. All 84 modified files committed in logical groups (by crate/domain).
2. Working tree is clean (`git status` shows no modifications).
3. Each commit is atomic and describes one logical change.

### Phase 49: Training Worker Spawn Fix

**Goal**: Fix binary PATH resolution so training worker spawns successfully on backend boot.
**Depends on**: Phase 48
**Requirements**: WRK-01, WRK-02
**Plans**: 2 plans

Plans:
- [ ] 49-01-PLAN.md -- Binary resolution fix and preflight boot gate
- [ ] 49-02-PLAN.md -- Supervisor circuit breaker and crash job cleanup

**Success Criteria**:

1. Training worker process starts when backend boots.
2. Service status shows training worker as RUNNING (not degraded).
3. `training-worker.degraded` marker file is not present after successful boot.

### Phase 50: Runtime State Hygiene

**Goal**: Clean stale sockets, degraded markers, and restart counters on boot.
**Depends on**: Phase 48
**Requirements**: RTH-01, RTH-02, RTH-03
**Plans**: 2 plans

Plans:
- [ ] 50-01-PLAN.md -- Stale socket and marker cleanup on boot
- [ ] 50-02-PLAN.md -- Supervision state JSON migration with crash-vs-rebuild discrimination

**Success Criteria**:

1. SecD socket is cleaned on boot when no backing process exists.
2. Training worker degraded marker is cleared when worker starts successfully.
3. Backend restart counter distinguishes crash restarts from dev-rebuild restarts.

## Progress

**Execution Order:**
48 -> 49, 50 (49 and 50 are independent after 48)

| Phase                                              | Plans Complete | Status   | Completed  |
| -------------------------------------------------- | -------------- | -------- | ---------- |
| 48. Commit Dirty Tree                              | 0/1            | Pending  |            |
| 49. Training Worker Spawn Fix                      | 0/2            | Pending  |            |
| 50. Runtime State Hygiene                           | 0/2            | Pending  |            |
