# Requirements: adapterOS

**Defined:** 2026-03-04
**Core Value:** Deterministic, verifiable LoRA inference on Apple Silicon so every operator action and model response is reproducible and auditable

## v1.1.18 Requirements

Milestone focus: stabilize runtime — fix training worker spawn, clean stale state, commit dirty tree.

### Worker Lifecycle

- [ ] **WRK-01**: Training worker spawns successfully when backend starts (binary resolution fixed)
- [ ] **WRK-02**: Training worker reports healthy in service status after boot

### Runtime Hygiene

- [ ] **RTH-01**: Stale SecD socket is cleaned up on boot when no backing process exists
- [ ] **RTH-02**: Training worker degraded marker is cleared when worker successfully starts
- [ ] **RTH-03**: Backend restart counter reflects actual crash count, not dev-rebuild kickstarts

### Tree Commit

- [ ] **GIT-01**: All modified files committed in logical, atomic commits
- [ ] **GIT-02**: Working tree is clean after commit series

## v1.1.17 Requirements (Completed)

### Production Cut Contract

- [x] **REL-47-01**: Prod-mode release gate is strict/no-skip and blocks governance `blocked_external` outcomes.
- [x] **API-47-01**: Runtime/OpenAPI route closure matrix and strict allowlist policy are enforced.
- [x] **SEC-47-01**: Release-safe auth posture and tenant-isolation assertions are blocking.
- [x] **OPS-47-01**: Runbook drill evidence and release artifact signing/provenance checks are release-required.

## v1.1.16 Requirements (Completed)

### Training Worker and Enqueue Safety

- [x] **TRN-46-01**: Training start fails closed with explicit API error when no healthy training worker is available.

### Determinism and Version Compatibility

- [x] **DET-46-01**: Training preflight validates dataset algorithm versions (HKDF/parser/path-normalization) against runtime constants before enqueue.

### Primary Model Runtime Consistency

- [x] **OPS-46-01**: Primary model resolution is canonicalized so training and model status reference the same active model identity/path.

### Documentation and Error Contract Grounding

- [x] **DOC-46-01**: Training failure paths expose actionable terminal reason in job status/log APIs and are citation-grounded in phase artifacts.

## v1.1.15 Requirements (Completed)

### AdapterOps Timeline Continuity

- [x] **UX-41-01**: Adapter detail shows repository command timeline history (latest-first).

### Command Deck AdapterOps Parity

- [x] **UX-41-02**: Command deck provides adapter operation actions in adapter/update-center contexts.
- [x] **A11Y-41-01**: Command intent routing remains explicit and assistive-friendly.

### Dataset Version Contract Continuity

- [x] **VC-41-01**: Training wizard submits via typed `CreateTrainingJobRequest` with `dataset_version_id`.

### Documentation and Grounding Discipline

- [x] **DOC-41-01**: Phase artifacts include concrete code citations and best-practice references.

## Future Requirements

### Adapter Operations UX (Deferred)

- **UX-42-01**: Multi-repo aggregated adapter command timeline with server-side filtering.
- **UX-42-02**: In-command-deck version selector for direct promote/checkout execution without navigation.

## Accepted External Debt

- `GOV-16` remains accepted external debt from governance milestones until canonical required-check API capability changes from `blocked_external`.

## Out of Scope (v1.1.18)

| Feature | Reason |
|---------|--------|
| New features or capabilities | Stabilization only — fix what's broken |
| Architecture refactoring | Out of scope for this pass |
| Governance debt closure (`GOV-16`) | Remains accepted external debt |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| WRK-01 | Phase 49 | Pending |
| WRK-02 | Phase 49 | Pending |
| RTH-01 | Phase 50 | Pending |
| RTH-02 | Phase 50 | Pending |
| RTH-03 | Phase 50 | Pending |
| GIT-01 | Phase 48 | Pending |
| GIT-02 | Phase 48 | Pending |
| REL-47-01 | Phase 47 | Complete |
| API-47-01 | Phase 47 | Complete |
| SEC-47-01 | Phase 47 | Complete |
| OPS-47-01 | Phase 47 | Complete |
| TRN-46-01 | Phase 46 | Complete |
| DET-46-01 | Phase 46 | Complete |
| OPS-46-01 | Phase 46 | Complete |
| DOC-46-01 | Phase 46 | Complete |
| UX-41-01 | Phase 43 | Complete |
| UX-41-02 | Phase 44 | Complete |
| A11Y-41-01 | Phase 44 | Complete |
| VC-41-01 | Phase 45 | Complete |
| DOC-41-01 | Phase 45 | Complete |

**Coverage:**
- v1.1.18 requirements: 7 total
- Mapped to phases: 7
- Unmapped: 0

---
*Requirements defined: 2026-03-04*
*Last updated: 2026-03-04 after v1.1.18 milestone initialization*
